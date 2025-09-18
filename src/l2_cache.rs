//! L2 持久化缓存模块
//!
//! 基于 MelangeDB 实现持久化存储层

use crate::config::{L2Config, CompressionConfig};
use crate::compression::Compressor;
use crate::error::{CacheError, CacheResult};
use crate::config::LoggingConfig;
use crate::metrics::MetricsCollector;
use crate::ttl::TtlManager;
use crate::types::{CacheLayer, CacheOperation};
use crate::cache_log;
use bytes::Bytes;
use crate::melange_adapter::{MelangeAdapter, BatchOperation};
// bincode 2.0 使用模块函数
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::task;

/// L2 持久化缓存
#[derive(Debug)]
pub struct L2Cache {
    config: Arc<L2Config>,
    logging_config: Arc<LoggingConfig>,
    /// MelangeDB 适配器
    db: Arc<MelangeAdapter>,
    /// 压缩器
    compressor: Arc<Compressor>,
    /// TTL 管理器
    ttl_manager: Arc<TtlManager>,
    /// 指标收集器
    metrics: Arc<MetricsCollector>,
    /// 统计信息
    stats: Arc<RwLock<L2CacheStats>>,
    /// 磁盘使用量估算
    disk_usage: Arc<AtomicU64>,
}

/// L2 缓存统计信息
#[derive(Debug, Clone, Default)]
pub struct L2CacheStats {
    /// 读取次数
    pub reads: u64,
    /// 写入次数
    pub writes: u64,
    /// 删除次数
    pub deletes: u64,
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 压缩操作次数
    pub compactions: u64,
    /// 估算的磁盘使用量
    pub estimated_disk_usage: u64,
    /// 实际存储的条目数
    pub entry_count: u64,
    /// 平均读取延迟（毫秒）
    pub avg_read_latency_ms: f64,
    /// 平均写入延迟（毫秒）
    pub avg_write_latency_ms: f64,
}

/// 存储的元数据
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
struct StoredMetadata {
    /// 创建时间
    created_at: u64,
    /// 最后访问时间
    accessed_at: u64,
    /// 过期时间（0表示永不过期）
    expires_at: u64,
    /// 访问次数
    access_count: u64,
    /// 原始数据大小
    original_size: usize,
    /// 是否压缩
    is_compressed: bool,
    /// 数据大小
    data_size: usize,
}

/// 键前缀常量
mod key_prefixes {
    pub const DATA: &[u8] = b"d:";
    pub const METADATA: &[u8] = b"m:";
    pub const TTL_INDEX: &[u8] = b"t:";
}

impl L2Cache {
    /// 创建新的 L2 缓存实例
    pub async fn new(
        config: L2Config,
        logging_config: LoggingConfig,
    ) -> CacheResult<Self> {
        if !config.enable_l2_cache {
            return Err(CacheError::config_error("L2 缓存未启用"));
        }

        let data_dir = config.data_dir.clone()
            .unwrap_or_else(|| std::env::temp_dir().join("rat_memcache_l2"));

        // 创建数据目录
        tokio::fs::create_dir_all(&data_dir).await
            .map_err(|e| CacheError::io_error(&format!("创建数据目录失败: {}", e)))?;

        // 配置 MelangeDB 适配器
        let melange_config = crate::melange_adapter::MelangeConfig {
            compression_algorithm: if config.enable_compression {
                match config.compression_level {
                    1..=3 => crate::melange_adapter::CompressionAlgorithm::Lz4,
                    4..=6 => crate::melange_adapter::CompressionAlgorithm::Lz4,
                    7..=9 => crate::melange_adapter::CompressionAlgorithm::Lz4,
                    _ => crate::melange_adapter::CompressionAlgorithm::None,
                }
            } else {
                crate::melange_adapter::CompressionAlgorithm::None
            },
            cache_size_mb: (config.block_cache_size / (1024 * 1024)) as usize,
            max_file_size_mb: (config.max_disk_size / (1024 * 1024)) as usize,
            enable_statistics: true,
        };

        // 启动时清空缓存（如果配置了）
        if config.clear_on_startup && data_dir.exists() {
            cache_log!(logging_config, info, "清空 L2 缓存目录: {:?}", data_dir);
            tokio::fs::remove_dir_all(&data_dir).await
                .map_err(|e| CacheError::io_error(&format!("清空缓存目录失败: {}", e)))?;
            tokio::fs::create_dir_all(&data_dir).await
                .map_err(|e| CacheError::io_error(&format!("创建缓存目录失败: {}", e)))?;
        }

        // 打开 MelangeDB
        let db = Arc::new(MelangeAdapter::new(&data_dir, melange_config)?);

        cache_log!(logging_config, info, "L2 缓存初始化完成，目录: {:?}", data_dir);

        // 创建组件
        let compression_config = CompressionConfig {
            enable_lz4: config.enable_compression,
            compression_threshold: 1024, // 默认阈值
            compression_level: config.compression_level,
            auto_compression: true,
            min_compression_ratio: 0.8,
        };
        let compressor = Arc::new(Compressor::new(compression_config));

        // TTL 管理器需要配置，暂时用默认配置
        let ttl_config = crate::config::TtlConfig {
            default_ttl: None,
            max_ttl: 86400,
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: false,
        };
        let ttl_manager = Arc::new(TtlManager::new(ttl_config, logging_config.clone()).await?);

        let metrics = Arc::new(MetricsCollector::new().await?);

        let stats = Arc::new(RwLock::new(L2CacheStats::default()));
        let disk_usage = Arc::new(AtomicU64::new(0));

        Ok(Self {
            config: Arc::new(config),
            logging_config: Arc::new(logging_config),
            db,
            compressor,
            ttl_manager,
            metrics,
            stats,
            disk_usage,
        })
    }

    /// 获取值
    pub async fn get(&self, key: &str) -> CacheResult<Option<Bytes>> {
        let start_time = Instant::now();
        let data_key = [key_prefixes::DATA, key.as_bytes()].concat();

        // 读取数据
        let result = self.db.get(&data_key)?;

        if let Some(data) = result {
            // 解码数据和元数据
            let decoded_data: ((Vec<u8>, StoredMetadata), usize) = bincode::decode_from_slice::<(Vec<u8>, StoredMetadata), _>(&data, bincode::config::standard())
                .map_err(|e| CacheError::compression_error(&format!("解码数据失败: {}", e)))?;
            let (value_bytes, metadata) = decoded_data.0;
            let value = Bytes::from(value_bytes);

            // 检查 TTL
            if metadata.expires_at > 0 && metadata.expires_at < chrono::Utc::now().timestamp() as u64 {
                // 数据已过期，删除
                self.delete(key).await?;
                return Ok(None);
            }

            // 更新统计信息
            {
                let mut stats = self.stats.write().await;
                stats.reads += 1;
                stats.hits += 1;
                stats.avg_read_latency_ms = (stats.avg_read_latency_ms * (stats.reads - 1) as f64
                    + start_time.elapsed().as_millis() as f64) / stats.reads as f64;
            }

            // 更新访问时间
            self.update_access_time(key, &metadata).await?;

            cache_log!(self.logging_config, debug, "L2 缓存命中: {} ({} bytes)", key, value.len());
            Ok(Some(value))
        } else {
            // 更新统计信息
            {
                let mut stats = self.stats.write().await;
                stats.reads += 1;
                stats.misses += 1;
                stats.avg_read_latency_ms = (stats.avg_read_latency_ms * (stats.reads - 1) as f64
                    + start_time.elapsed().as_millis() as f64) / stats.reads as f64;
            }

            cache_log!(self.logging_config, debug, "L2 缓存未命中: {}", key);
            Ok(None)
        }
    }

    /// 设置值
    pub async fn set(&self, key: &str, value: Bytes) -> CacheResult<()> {
        self.set_with_ttl(key, value, 0).await
    }

    /// 设置值（带 TTL）
    pub async fn set_with_ttl(&self, key: &str, value: Bytes, ttl_seconds: u64) -> CacheResult<()> {
        let start_time = Instant::now();
        let now = chrono::Utc::now().timestamp() as u64;

        // 准备元数据
        let metadata = StoredMetadata {
            created_at: now,
            accessed_at: now,
            expires_at: if ttl_seconds > 0 { now + ttl_seconds } else { 0 },
            access_count: 1,
            original_size: value.len(),
            is_compressed: self.config.enable_compression,
            data_size: value.len(),
        };

        // 编码数据和元数据
        let data_to_encode = (value.to_vec(), metadata);
        let encoded_data = bincode::encode_to_vec(&data_to_encode, bincode::config::standard())
            .map_err(|e| CacheError::compression_error(&format!("编码数据失败: {}", e)))?;

        let data_key = [key_prefixes::DATA, key.as_bytes()].concat();

        // 写入 MelangeDB
        self.db.put(&data_key, &encoded_data)?;

        // 更新统计信息
        {
            let mut stats = self.stats.write().await;
            stats.writes += 1;
            stats.entry_count += 1;
            stats.estimated_disk_usage = self.disk_usage.load(Ordering::Relaxed);
            stats.avg_write_latency_ms = (stats.avg_write_latency_ms * (stats.writes - 1) as f64
                + start_time.elapsed().as_millis() as f64) / stats.writes as f64;
        }

        cache_log!(self.logging_config, debug, "L2 缓存设置: {} ({} bytes)", key, value.len());
        Ok(())
    }

    /// 删除值
    pub async fn delete(&self, key: &str) -> CacheResult<bool> {
        let data_key = [key_prefixes::DATA, key.as_bytes()].concat();

        // 检查是否存在
        let exists = self.db.get(&data_key)?
            .is_some();

        if exists {
            // 删除数据
            self.db.delete(&data_key)?;

            // 更新统计信息
            {
                let mut stats = self.stats.write().await;
                stats.deletes += 1;
                stats.entry_count = stats.entry_count.saturating_sub(1);
            }

            cache_log!(self.logging_config, debug, "L2 缓存删除: {}", key);
        }

        Ok(exists)
    }

    /// 清空缓存
    pub async fn clear(&self) -> CacheResult<()> {
        // 获取所有键并删除
        let all_data = self.db.prefix_iter(key_prefixes::DATA)?;
        let operations: Vec<BatchOperation> = all_data
            .into_iter()
            .map(|(key, _)| MelangeAdapter::delete_op(&key))
            .collect();

        // 批量删除
        if !operations.is_empty() {
            self.db.batch_write(operations)?;
        }

        // 重置统计信息
        {
            let mut stats = self.stats.write().await;
            *stats = L2CacheStats::default();
        }

        cache_log!(self.logging_config, info, "L2 缓存已清空");
        Ok(())
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> L2CacheStats {
        self.stats.read().await.clone()
    }

    /// 更新访问时间
    async fn update_access_time(&self, key: &str, metadata: &StoredMetadata) -> CacheResult<()> {
        let data_key = [key_prefixes::DATA, key.as_bytes()].concat();

        // 读取当前数据
        let current_data = self.db.get(&data_key)?;

        if let Some(data) = current_data {
            // 解码
            let decoded_data: ((Vec<u8>, StoredMetadata), usize) = bincode::decode_from_slice::<(Vec<u8>, StoredMetadata), _>(&data, bincode::config::standard())
                .map_err(|e| CacheError::compression_error(&format!("解码数据失败: {}", e)))?;
            let (value_bytes, mut current_metadata) = decoded_data.0;
            let value = Bytes::from(value_bytes);

            // 更新访问时间和计数
            current_metadata.accessed_at = chrono::Utc::now().timestamp() as u64;
            current_metadata.access_count += 1;

            // 重新编码并写入
            let data_to_encode = (value.to_vec(), current_metadata);
            let encoded_data = bincode::encode_to_vec(&data_to_encode, bincode::config::standard())
                .map_err(|e| CacheError::compression_error(&format!("重新编码数据失败: {}", e)))?;

            self.db.put(&data_key, &encoded_data)?;
        }

        Ok(())
    }

    /// 执行 TTL 清理
    pub async fn cleanup_expired(&self) -> CacheResult<usize> {
        let now = chrono::Utc::now().timestamp() as u64;
        let mut operations = Vec::new();
        let mut cleaned_count = 0;

        // 遍历所有键
        let all_data = self.db.prefix_iter(key_prefixes::DATA)?;

        for (key, value) in all_data {
            // 解码元数据
            if let Ok(decoded_data) = bincode::decode_from_slice::<(Vec<u8>, StoredMetadata), _>(&value, bincode::config::standard()) {
                let (_, metadata) = decoded_data.0;
                if metadata.expires_at > 0 && metadata.expires_at < now {
                    // 数据已过期，添加到批量删除
                    operations.push(MelangeAdapter::delete_op(&key));
                    cleaned_count += 1;
                }
            }
        }

        // 执行批量删除
        if cleaned_count > 0 {
            self.db.batch_write(operations)?;

            // 更新统计信息
            {
                let mut stats = self.stats.write().await;
                stats.entry_count = stats.entry_count.saturating_sub(cleaned_count);
            }
        }

        cache_log!(self.logging_config, info, "L2 缓存清理了 {} 个过期条目", cleaned_count);
        Ok(cleaned_count as usize)
    }

    /// 检查键是否存在
    pub async fn contains_key(&self, key: &str) -> CacheResult<bool> {
        let data_key = [key_prefixes::DATA, key.as_bytes()].concat();
        let result = self.db.get(&data_key)?;
        Ok(result.is_some())
    }

    /// 获取所有键
    pub async fn keys(&self) -> CacheResult<Vec<String>> {
        let all_data = self.db.prefix_iter(key_prefixes::DATA)?;
        let mut keys = Vec::new();

        for (key, _) in all_data {
            if let Some(key_str) = std::str::from_utf8(&key[key_prefixes::DATA.len()..]).ok() {
                keys.push(key_str.to_string());
            }
        }

        Ok(keys)
    }

    /// 压缩数据库
    pub async fn compact(&self) -> CacheResult<()> {
        // MelangeDB 通常会自动压缩，这里留空或调用相应的压缩方法
        cache_log!(self.logging_config, info, "L2 缓存压缩操作");
        Ok(())
    }

    /// 关闭缓存
    pub async fn shutdown(&self) -> CacheResult<()> {
        // MelangeDB 会在 Drop 时自动关闭
        cache_log!(self.logging_config, info, "L2 缓存已关闭");
        Ok(())
    }
}