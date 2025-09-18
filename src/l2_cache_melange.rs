//! L2 持久化缓存模块 - MelangeDB 实现
//!
//! 基于 MelangeDB 实现持久化存储层，提供高性能的键值存储

use crate::config::L2Config;
use crate::melange_adapter::{MelangeAdapter, MelangeConfig, CompressionAlgorithm, BatchOperation};
use crate::compression::Compressor;
use crate::error::{CacheError, CacheResult};
use crate::config::LoggingConfig;
use crate::metrics::MetricsCollector;
use crate::ttl::TtlManager;
use crate::types::{CacheLayer, CacheOperation};
use crate::cache_log;
use bytes::Bytes;
use bincode::{encode_to_vec, decode_from_slice};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::task;

/// L2 持久化缓存 - MelangeDB 实现
#[derive(Debug)]
pub struct L2CacheMelange {
    config: Arc<L2Config>,
    logging_config: Arc<LoggingConfig>,
    /// MelangeDB 适配器实例
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

impl L2CacheMelange {
    /// 创建新的 L2 缓存 - MelangeDB 实现
    pub async fn new(
        config: L2Config,
        logging_config: LoggingConfig,
        compressor: Compressor,
        ttl_manager: Arc<TtlManager>,
        metrics: Arc<MetricsCollector>,
    ) -> CacheResult<Self> {
        cache_log!(logging_config, debug, "L2CacheMelange::new 开始初始化");
        cache_log!(logging_config, debug, "L2 缓存配置: {:?}", config);

        // 检查是否启用 L2 缓存
        if !config.enable_l2_cache {
            cache_log!(logging_config, debug, "L2 缓存已禁用");
            return Err(CacheError::config_error("L2 缓存已禁用"));
        }

        // 验证数据库引擎
        if config.database_engine != crate::config::DatabaseEngine::MelangeDB {
            return Err(CacheError::config_error("此实现只支持 MelangeDB 引擎"));
        }

        // 获取数据目录
        cache_log!(logging_config, debug, "获取数据目录");
        let data_dir = config.data_dir.clone().unwrap_or_else(|| {
            cache_log!(logging_config, debug, "使用临时目录作为数据目录");
            let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
            let path = temp_dir.path().to_path_buf();
            cache_log!(logging_config, debug, "临时目录路径: {:?}", path);
            std::mem::forget(temp_dir); // 防止临时目录被删除
            path
        });
        cache_log!(logging_config, debug, "最终数据目录: {:?}", data_dir);

        // 创建数据目录
        if !data_dir.exists() {
            cache_log!(logging_config, debug, "尝试创建数据目录...");
            match std::fs::create_dir_all(&data_dir) {
                Ok(_) => cache_log!(logging_config, debug, "数据目录创建成功"),
                Err(e) => {
                    cache_log!(logging_config, debug, "创建数据目录失败: {}", e);
                    return Err(CacheError::io_error(&format!("创建数据目录失败: {}", e)));
                }
            }
        }

        // 验证数据目录可写
        cache_log!(logging_config, debug, "验证数据目录写权限");
        let test_file = data_dir.join(".write_test");
        match std::fs::write(&test_file, b"test") {
            Ok(_) => {
                cache_log!(logging_config, debug, "数据目录写权限验证成功");
                let _ = std::fs::remove_file(&test_file);
            },
            Err(e) => {
                cache_log!(logging_config, debug, "数据目录写权限验证失败: {}", e);
                return Err(CacheError::io_error(&format!("数据目录不可写: {}", e)));
            }
        }

        // 处理启动时清空缓存目录的逻辑
        if config.clear_on_startup && data_dir.exists() {
            cache_log!(logging_config, debug, "配置要求启动时清空缓存目录");
            match std::fs::remove_dir_all(&data_dir) {
                Ok(_) => cache_log!(logging_config, debug, "缓存目录清空成功"),
                Err(e) => {
                    cache_log!(logging_config, debug, "清空缓存目录失败: {}", e);
                    return Err(CacheError::io_error(&format!("清空缓存目录失败: {}", e)));
                }
            }
            match std::fs::create_dir_all(&data_dir) {
                Ok(_) => cache_log!(logging_config, debug, "缓存目录重新创建成功"),
                Err(e) => {
                    cache_log!(logging_config, debug, "重新创建缓存目录失败: {}", e);
                    return Err(CacheError::io_error(&format!("重新创建缓存目录失败: {}", e)));
                }
            }
        }

        // 创建 MelangeDB 配置
        let melange_config = MelangeConfig {
            compression_algorithm: config.melange_config.compression_algorithm,
            cache_size_mb: config.melange_config.cache_size_mb,
            max_file_size_mb: config.melange_config.max_file_size_mb,
            enable_statistics: config.melange_config.enable_statistics,
        };

        // 打开 MelangeDB
        cache_log!(logging_config, debug, "尝试打开 MelangeDB 数据库，路径: {:?}", data_dir);
        let db = MelangeAdapter::new(&data_dir, melange_config)?;

        let cache = Self {
            config: Arc::new(config),
            logging_config: Arc::new(logging_config),
            db: Arc::new(db),
            compressor: Arc::new(compressor),
            ttl_manager,
            metrics,
            stats: Arc::new(RwLock::new(L2CacheStats::default())),
            disk_usage: Arc::new(AtomicU64::new(0)),
        };

        // 初始化磁盘使用量统计
        cache.update_disk_usage_estimate().await;

        cache_log!(cache.logging_config, debug, "L2 缓存（MelangeDB）已初始化，数据目录: {:?}", &data_dir);

        Ok(cache)
    }

    /// 获取缓存值
    pub async fn get(&self, key: &str) -> CacheResult<Option<Bytes>> {
        let start_time = Instant::now();

        // 检查 TTL
        if self.ttl_manager.is_expired(key).await {
            self.delete_internal(key).await?;
            self.record_miss().await;
            self.record_read_latency(start_time.elapsed()).await;
            return Ok(None);
        }

        let db = Arc::clone(&self.db);
        let key_str = key.to_string();
        let compressor = Arc::clone(&self.compressor);

        // 在后台线程中执行 I/O 操作
        let result = task::spawn_blocking(move || -> CacheResult<Option<(Bytes, StoredMetadata)>> {
            // 构造数据键
            let data_key = Self::make_data_key(&key_str);
            let metadata_key = Self::make_metadata_key(&key_str);

            // 读取数据和元数据
            let data = db.get(&data_key)?;
            let metadata_bytes = db.get(&metadata_key)?;

            if let (Some(data), Some(metadata_bytes)) = (data, metadata_bytes) {
                // 反序列化元数据
                let (metadata, _): (StoredMetadata, usize) = decode_from_slice(&metadata_bytes, bincode::config::standard())
                    .map_err(|e| CacheError::serialization_error(&format!("反序列化元数据失败: {}", e)))?;

                // 解压缩数据
                let decompressed = compressor.decompress(&data, metadata.is_compressed)?;

                Ok(Some((decompressed.data, metadata)))
            } else {
                Ok(None)
            }
        }).await
        .map_err(|e| CacheError::io_error(&format!("后台任务执行失败: {}", e)))??;

        if let Some((data, mut metadata)) = result {
            // 更新访问统计
            metadata.accessed_at = crate::types::current_timestamp();
            metadata.access_count += 1;

            // 异步更新元数据
            self.update_metadata_async(key, metadata).await;

            self.record_hit().await;
            self.metrics.record_cache_hit(CacheLayer::Persistent).await;

            cache_log!(self.logging_config, debug, "L2 缓存命中: {}", key);

            self.record_read_latency(start_time.elapsed()).await;
            Ok(Some(data))
        } else {
            self.record_miss().await;
            self.metrics.record_cache_miss().await;

            cache_log!(self.logging_config, debug, "L2 缓存未命中: {}", key);

            self.record_read_latency(start_time.elapsed()).await;
            Ok(None)
        }
    }

    /// 设置缓存值
    pub async fn set(&self, key: String, value: Bytes, ttl_seconds: Option<u64>) -> CacheResult<()> {
        let start_time = Instant::now();

        // 检查磁盘空间
        self.check_disk_space(value.len()).await?;

        // 压缩数据
        let compression_result = self.compressor.compress(&value)?;

        // 创建元数据
        let metadata = StoredMetadata {
            created_at: crate::types::current_timestamp(),
            accessed_at: crate::types::current_timestamp(),
            expires_at: if let Some(ttl) = ttl_seconds {
                crate::types::current_timestamp() + ttl
            } else {
                0
            },
            access_count: 1,
            original_size: value.len(),
            is_compressed: compression_result.is_compressed,
            data_size: compression_result.compressed_data.len(),
        };

        let db = Arc::clone(&self.db);
        let key_clone = key.clone();
        let data = compression_result.compressed_data.clone();

        // 在后台线程中执行 I/O 操作
        task::spawn_blocking(move || -> CacheResult<()> {
            // 序列化元数据
            let metadata_bytes = encode_to_vec(&metadata, bincode::config::standard())
                .map_err(|e| CacheError::serialization_error(&format!("序列化元数据失败: {}", e)))?;

            // 使用批量写入
            let operations = vec![
                MelangeAdapter::insert_op(&Self::make_data_key(&key_clone), &data),
                MelangeAdapter::insert_op(&Self::make_metadata_key(&key_clone), &metadata_bytes),
            ];

            db.batch_write(operations)?;
            Ok(())
        }).await
        .map_err(|e| CacheError::io_error(&format!("后台任务执行失败: {}", e)))??;

        // 设置 TTL
        if ttl_seconds.is_some() {
            self.ttl_manager.add_key(key.clone(), ttl_seconds).await?;
        }

        // 更新统计
        self.record_write().await;
        self.disk_usage.fetch_add(compression_result.compressed_data.len() as u64, Ordering::Relaxed);

        // 记录指标
        self.metrics.record_cache_operation(CacheOperation::Set).await;
        self.metrics.record_memory_usage(CacheLayer::Persistent, self.disk_usage.load(Ordering::Relaxed)).await;

        if compression_result.is_compressed {
            self.metrics.record_compression(
                compression_result.original_size as u64,
                compression_result.compressed_size as u64,
            ).await;
        }

        cache_log!(self.logging_config, debug, "L2 缓存设置: {} ({}压缩)",
            key, if compression_result.is_compressed { "已" } else { "未" });

        self.record_write_latency(start_time.elapsed()).await;
        Ok(())
    }

    /// 删除缓存值
    pub async fn delete(&self, key: &str) -> CacheResult<bool> {
        let start_time = Instant::now();

        let deleted = self.delete_internal(key).await?;

        if deleted {
            self.record_delete().await;
            self.metrics.record_cache_operation(CacheOperation::Delete).await;

            cache_log!(self.logging_config, debug, "L2 缓存删除: {}", key);
        }

        self.record_write_latency(start_time.elapsed()).await;
        Ok(deleted)
    }

    /// 清空缓存
    pub async fn clear(&self) -> CacheResult<()> {
        let _start_time = Instant::now();

        let db = Arc::clone(&self.db);

        // 在后台线程中执行清空操作
        task::spawn_blocking(move || -> CacheResult<()> {
            db.clear()?;
            Ok(())
        }).await
        .map_err(|e| CacheError::io_error(&format!("后台任务执行失败: {}", e)))??;

        // 重置统计
        self.disk_usage.store(0, Ordering::Relaxed);
        let mut stats = self.stats.write().await;
        stats.entry_count = 0;
        drop(stats);

        self.metrics.record_cache_operation(CacheOperation::Clear).await;
        self.metrics.record_memory_usage(CacheLayer::Persistent, 0).await;

        cache_log!(self.logging_config, debug, "L2 缓存已清空");

        Ok(())
    }

    /// 压缩数据库（MelangeDB 版本）
    pub async fn compact(&self) -> CacheResult<()> {
        let start_time = Instant::now();

        // MelangeDB 可能不需要手动压缩，但我们仍然记录统计
        let mut stats = self.stats.write().await;
        stats.compactions += 1;
        drop(stats);

        // 重新计算磁盘使用量
        self.update_disk_usage_estimate().await;

        cache_log!(self.logging_config, debug, "L2 缓存压缩完成，耗时: {:.2}ms",
            start_time.elapsed().as_millis());

        Ok(())
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> L2CacheStats {
        let mut stats = self.stats.read().await.clone();
        stats.estimated_disk_usage = self.disk_usage.load(Ordering::Relaxed);
        stats
    }

    /// 检查是否包含键
    pub async fn contains_key(&self, key: &str) -> CacheResult<bool> {
        let db = Arc::clone(&self.db);
        let key_str = key.to_string();

        let exists = task::spawn_blocking(move || -> CacheResult<bool> {
            let data_key = Self::make_data_key(&key_str);
            let result = db.get(&data_key)?;
            Ok(result.is_some())
        }).await
        .map_err(|e| CacheError::io_error(&format!("后台任务执行失败: {}", e)))??;

        Ok(exists)
    }

    /// 获取所有键
    pub async fn keys(&self) -> CacheResult<Vec<String>> {
        let db = Arc::clone(&self.db);

        let keys = task::spawn_blocking(move || -> CacheResult<Vec<String>> {
            let data_prefix = key_prefixes::DATA;
            let results = db.prefix_iter(data_prefix)?;

            let mut keys = Vec::new();
            for (key, _) in results {
                if key.starts_with(data_prefix) {
                    let original_key = String::from_utf8_lossy(&key[data_prefix.len()..]).to_string();
                    keys.push(original_key);
                }
            }

            Ok(keys)
        }).await
        .map_err(|e| CacheError::io_error(&format!("后台任务执行失败: {}", e)))??;

        Ok(keys)
    }

    /// 获取缓存大小
    pub async fn len(&self) -> CacheResult<usize> {
        let stats = self.stats.read().await;
        Ok(stats.entry_count as usize)
    }

    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> CacheResult<bool> {
        let len = self.len().await?;
        Ok(len == 0)
    }

    /// 内部删除方法
    async fn delete_internal(&self, key: &str) -> CacheResult<bool> {
        let db = Arc::clone(&self.db);
        let key_str = key.to_string();

        let deleted = task::spawn_blocking(move || -> CacheResult<bool> {
            let data_key = Self::make_data_key(&key_str);
            let metadata_key = Self::make_metadata_key(&key_str);

            // 检查键是否存在
            let exists = db.get(&data_key)?;

            if exists.is_some() {
                // 删除数据和元数据
                let operations = vec![
                    MelangeAdapter::delete_op(&data_key),
                    MelangeAdapter::delete_op(&metadata_key),
                ];

                db.batch_write(operations)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }).await
        .map_err(|e| CacheError::io_error(&format!("后台任务执行失败: {}", e)))??;

        if deleted {
            // 移除 TTL
            self.ttl_manager.remove_key(key).await;
        }

        Ok(deleted)
    }

    /// 异步更新元数据
    async fn update_metadata_async(&self, key: &str, metadata: StoredMetadata) {
        let db = Arc::clone(&self.db);
        let key_str = key.to_string();

        let _ = task::spawn_blocking(move || -> CacheResult<()> {
            let metadata_key = Self::make_metadata_key(&key_str);
            let metadata_bytes = encode_to_vec(&metadata, bincode::config::standard())
                .map_err(|e| CacheError::serialization_error(&format!("序列化元数据失败: {}", e)))?;

            db.put(&metadata_key, &metadata_bytes)?;
            Ok(())
        }).await;
    }

    /// 检查磁盘空间
    async fn check_disk_space(&self, required_size: usize) -> CacheResult<()> {
        let current_usage = self.disk_usage.load(Ordering::Relaxed);
        if current_usage + required_size as u64 > self.config.max_disk_size {
            let current_usage = self.disk_usage.load(Ordering::Relaxed) as usize;
            return Err(CacheError::cache_full(current_usage + required_size, self.config.max_disk_size as usize));
        }
        Ok(())
    }

    /// 更新磁盘使用量估算
    async fn update_disk_usage_estimate(&self) {
        let db = Arc::clone(&self.db);

        let _ = task::spawn_blocking(move || -> CacheResult<(u64, u64)> {
            let data_prefix = key_prefixes::DATA;
            let results = db.prefix_iter(data_prefix)?;

            let mut total_size = 0u64;
            let mut entry_count = 0u64;

            for (_, value) in results {
                total_size += value.len() as u64;
                entry_count += 1;
            }

            Ok((total_size, entry_count))
        }).await
        .map(|result| {
            if let Ok((size, count)) = result {
                self.disk_usage.store(size, Ordering::Relaxed);

                let stats_clone = Arc::clone(&self.stats);
                tokio::spawn(async move {
                    let mut stats = stats_clone.write().await;
                    stats.entry_count = count;
                });
            }
        });
    }

    /// 构造数据键
    fn make_data_key(key: &str) -> Vec<u8> {
        let mut data_key = Vec::with_capacity(key_prefixes::DATA.len() + key.len());
        data_key.extend_from_slice(key_prefixes::DATA);
        data_key.extend_from_slice(key.as_bytes());
        data_key
    }

    /// 构造元数据键
    fn make_metadata_key(key: &str) -> Vec<u8> {
        let mut metadata_key = Vec::with_capacity(key_prefixes::METADATA.len() + key.len());
        metadata_key.extend_from_slice(key_prefixes::METADATA);
        metadata_key.extend_from_slice(key.as_bytes());
        metadata_key
    }

    /// 记录命中
    async fn record_hit(&self) {
        let mut stats = self.stats.write().await;
        stats.hits += 1;
        stats.reads += 1;
    }

    /// 记录未命中
    async fn record_miss(&self) {
        let mut stats = self.stats.write().await;
        stats.misses += 1;
        stats.reads += 1;
    }

    /// 记录写入
    async fn record_write(&self) {
        let mut stats = self.stats.write().await;
        stats.writes += 1;
    }

    /// 记录删除
    async fn record_delete(&self) {
        let mut stats = self.stats.write().await;
        stats.deletes += 1;
    }

    /// 记录读取延迟
    async fn record_read_latency(&self, duration: std::time::Duration) {
        let latency_ms = duration.as_millis() as f64;
        let mut stats = self.stats.write().await;

        if stats.avg_read_latency_ms == 0.0 {
            stats.avg_read_latency_ms = latency_ms;
        } else {
            stats.avg_read_latency_ms = (stats.avg_read_latency_ms * 0.9) + (latency_ms * 0.1);
        }
    }

    /// 记录写入延迟
    async fn record_write_latency(&self, duration: std::time::Duration) {
        let latency_ms = duration.as_millis() as f64;
        let mut stats = self.stats.write().await;

        if stats.avg_write_latency_ms == 0.0 {
            stats.avg_write_latency_ms = latency_ms;
        } else {
            stats.avg_write_latency_ms = (stats.avg_write_latency_ms * 0.9) + (latency_ms * 0.1);
        }
    }
}

impl L2CacheStats {
    /// 计算命中率
    pub fn hit_rate(&self) -> f64 {
        if self.reads == 0 {
            return 0.0;
        }
        self.hits as f64 / self.reads as f64
    }

    /// 格式化统计信息
    pub fn format(&self) -> String {
        format!(
            "L2 缓存统计 (MelangeDB):\n\
             条目数: {}\n\
             磁盘使用: {} bytes\n\
             读取: {} 次 (命中: {}, 未命中: {}, 命中率: {:.1}%)\n\
             写入: {} 次\n\
             删除: {} 次\n\
             压缩: {} 次\n\
             平均读取延迟: {:.2}ms\n\
             平均写入延迟: {:.2}ms",
            self.entry_count,
            self.estimated_disk_usage,
            self.reads, self.hits, self.misses, self.hit_rate() * 100.0,
            self.writes,
            self.deletes,
            self.compactions,
            self.avg_read_latency_ms,
            self.avg_write_latency_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{L2Config, LoggingConfig, CompressionConfig, TtlConfig, DatabaseEngine, MelangeSpecificConfig};
    use crate::compression::Compressor;
    use crate::ttl::TtlManager;
    use crate::metrics::MetricsCollector;
    use tempfile::TempDir;

    async fn create_test_cache() -> (L2CacheMelange, TempDir) {
        let temp_dir = TempDir::new().unwrap();

        let l2_config = L2Config {
            enable_l2_cache: true,
            data_dir: Some(temp_dir.path().to_path_buf()),
            max_disk_size: 10 * 1024 * 1024, // 10MB
            write_buffer_size: 1024 * 1024,  // 1MB
            max_write_buffer_number: 3,
            block_cache_size: 512 * 1024,    // 512KB
            enable_compression: true,
            compression_level: 6,
            background_threads: 2,
            clear_on_startup: false,
            database_engine: DatabaseEngine::MelangeDB,
            melange_config: MelangeSpecificConfig {
                compression_algorithm: CompressionAlgorithm::Lz4,
                cache_size_mb: 256,
                max_file_size_mb: 512,
                enable_statistics: true,
            },
        };

        let logging_config = LoggingConfig {
            level: "debug".to_string(),
            enable_colors: false,
            show_timestamp: false,
            enable_performance_logs: true,
            enable_audit_logs: false,
            enable_cache_logs: true,
        };

        let compression_config = CompressionConfig {
            enable_lz4: true,
            compression_threshold: 100,
            compression_level: 4,
            auto_compression: true,
            min_compression_ratio: 0.8,
        };

        let ttl_config = TtlConfig {
            default_ttl: Some(60),
            max_ttl: 3600,
            cleanup_interval: 60,
            max_cleanup_entries: 100,
            lazy_expiration: true,
            active_expiration: false, // 测试中禁用主动过期
        };

        let compressor = Compressor::new(compression_config);
        let ttl_manager = Arc::new(TtlManager::new(ttl_config, logging_config.clone()).await.unwrap());
        let metrics = Arc::new(MetricsCollector::new().await.unwrap());

        let cache = L2CacheMelange::new(l2_config, logging_config, compressor, ttl_manager, metrics).await.unwrap();

        (cache, temp_dir)
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let (cache, _temp_dir) = create_test_cache().await;
        let is_empty = cache.is_empty().await.unwrap();
        assert!(is_empty);
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let (cache, _temp_dir) = create_test_cache().await;
        let key = "test_key".to_string();
        let value = Bytes::from("test_value");

        cache.set(key.clone(), value.clone(), None).await.unwrap();

        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
    }

    #[tokio::test]
    async fn test_delete() {
        let (cache, _temp_dir) = create_test_cache().await;
        let key = "test_key".to_string();
        let value = Bytes::from("test_value");

        cache.set(key.clone(), value, None).await.unwrap();
        assert!(cache.contains_key(&key).await.unwrap());

        let deleted = cache.delete(&key).await.unwrap();
        assert!(deleted);
        assert!(!cache.contains_key(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_clear() {
        let (cache, _temp_dir) = create_test_cache().await;

        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = Bytes::from(format!("value_{}", i));
            cache.set(key, value, None).await.unwrap();
        }

        let len_before = cache.len().await.unwrap();
        assert!(len_before > 0);

        cache.clear().await.unwrap();

        let is_empty = cache.is_empty().await.unwrap();
        assert!(is_empty);
    }

    #[tokio::test]
    async fn test_keys() {
        let (cache, _temp_dir) = create_test_cache().await;

        let test_keys = vec!["key1", "key2", "key3"];

        for key in &test_keys {
            let value = Bytes::from(format!("value_{}", key));
            cache.set(key.to_string(), value, None).await.unwrap();
        }

        let mut keys = cache.keys().await.unwrap();
        keys.sort();

        let mut expected = test_keys.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        expected.sort();

        assert_eq!(keys, expected);
    }

    #[tokio::test]
    async fn test_stats() {
        let (cache, _temp_dir) = create_test_cache().await;

        // 执行一些操作
        cache.set("key1".to_string(), Bytes::from("value1"), None).await.unwrap();
        cache.get("key1").await.unwrap();
        cache.get("nonexistent").await.unwrap();
        cache.delete("key1").await.unwrap();

        let stats = cache.get_stats().await;
        assert!(stats.reads > 0);
        assert!(stats.writes > 0);
        assert!(stats.hits > 0);
        assert!(stats.misses > 0);
        assert!(stats.deletes > 0);
    }

    #[tokio::test]
    async fn test_compression_algorithms() {
        let temp_dir = TempDir::new().unwrap();

        for compression in [CompressionAlgorithm::None, CompressionAlgorithm::Lz4] {
            let l2_config = L2Config {
                enable_l2_cache: true,
                data_dir: Some(temp_dir.path().to_path_buf()),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
                clear_on_startup: false,
                database_engine: DatabaseEngine::MelangeDB,
                melange_config: MelangeSpecificConfig {
                    compression_algorithm: compression,
                    cache_size_mb: 256,
                    max_file_size_mb: 512,
                    enable_statistics: true,
                },
            };

            let logging_config = LoggingConfig {
                level: "debug".to_string(),
                enable_colors: false,
                show_timestamp: false,
                enable_performance_logs: true,
                enable_audit_logs: false,
                enable_cache_logs: true,
            };

            let compression_config = CompressionConfig {
                enable_lz4: true,
                compression_threshold: 100,
                compression_level: 4,
                auto_compression: true,
                min_compression_ratio: 0.8,
            };

            let ttl_config = TtlConfig {
                default_ttl: Some(60),
                max_ttl: 3600,
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            };

            let compressor = Compressor::new(compression_config);
            let ttl_manager = Arc::new(TtlManager::new(ttl_config, logging_config.clone()).await.unwrap());
            let metrics = Arc::new(MetricsCollector::new().await.unwrap());

            let cache = L2CacheMelange::new(l2_config, logging_config, compressor, ttl_manager, metrics).await.unwrap();

            let key = "compression_test";
            let value = Bytes::from("this is a test value for compression");

            cache.set(key.to_string(), value.clone(), None).await.unwrap();
            let retrieved = cache.get(key).await.unwrap();
            assert_eq!(retrieved, Some(value));
        }
    }
}