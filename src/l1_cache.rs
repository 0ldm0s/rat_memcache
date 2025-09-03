//! L1 内存缓存模块
//!
//! 实现基于内存的高性能缓存层，支持多种驱逐策略

use crate::config::L1Config;
use crate::compression::{Compressor, CompressionResult};
use crate::error::{CacheError, CacheResult};
use crate::config::LoggingConfig;
use crate::metrics::MetricsCollector;
use crate::ttl::TtlManager;
use crate::types::{CacheValue, EvictionStrategy, CacheLayer, CacheOperation};
use crate::{cache_log, perf_log};
use bytes::Bytes;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// L1 内存缓存
#[derive(Debug)]
pub struct L1Cache {
    config: Arc<L1Config>,
    logging_config: Arc<LoggingConfig>,
    /// 主要存储：键值对映射
    storage: Arc<DashMap<String, CacheValue>>,
    /// 智能传输路由器（已移除）
    // router: Arc<SmartTransferRouter>,
    /// 压缩器
    compressor: Arc<Compressor>,
    /// TTL 管理器
    ttl_manager: Arc<TtlManager>,
    /// 指标收集器
    metrics: Arc<MetricsCollector>,
    /// LRU 访问顺序（用于 LRU 策略）
    lru_order: Arc<Mutex<VecDeque<String>>>,
    /// LFU 访问计数（用于 LFU 策略）
    lfu_counter: Arc<DashMap<String, AtomicU64>>,
    /// FIFO 插入顺序（用于 FIFO 策略）
    fifo_order: Arc<Mutex<VecDeque<String>>>,
    /// 当前内存使用量
    memory_usage: Arc<AtomicUsize>,
    /// 当前条目数量
    entry_count: Arc<AtomicUsize>,
    /// 驱逐统计
    eviction_stats: Arc<RwLock<EvictionStats>>,
}

/// 驱逐统计信息
#[derive(Debug, Clone, Default)]
struct EvictionStats {
    /// 按策略分类的驱逐次数
    lru_evictions: u64,
    lfu_evictions: u64,
    fifo_evictions: u64,
    ttl_evictions: u64,
    /// 总驱逐次数
    total_evictions: u64,
    /// 驱逐的总字节数
    evicted_bytes: u64,
}

impl L1Cache {
    /// 创建新的 L1 缓存
    pub async fn new(
        config: L1Config,
        logging_config: LoggingConfig,
        compressor: Compressor,
        ttl_manager: Arc<TtlManager>,
        metrics: Arc<MetricsCollector>,
    ) -> CacheResult<Self> {
        let cache = Self {
            config: Arc::new(config),
            logging_config: Arc::new(logging_config),
            storage: Arc::new(DashMap::new()),
            // router: Arc::new(router),
            compressor: Arc::new(compressor),
            ttl_manager,
            metrics,
            lru_order: Arc::new(Mutex::new(VecDeque::new())),
            lfu_counter: Arc::new(DashMap::new()),
            fifo_order: Arc::new(Mutex::new(VecDeque::new())),
            memory_usage: Arc::new(AtomicUsize::new(0)),
            entry_count: Arc::new(AtomicUsize::new(0)),
            eviction_stats: Arc::new(RwLock::new(EvictionStats::default())),
        };

        cache_log!(cache.logging_config, info, "L1 缓存已初始化，最大内存: {} bytes，最大条目: {}", 
            cache.config.max_memory, cache.config.max_entries);
        
        Ok(cache)
    }

    /// 获取缓存值
    pub async fn get(&self, key: &str) -> CacheResult<Option<Bytes>> {
        let start_time = Instant::now();
        
        // 检查 TTL
        if self.ttl_manager.is_expired(key).await {
            self.remove_internal(key).await;
            self.metrics.record_cache_miss().await;
            self.metrics.record_operation_latency(CacheOperation::Get, start_time.elapsed()).await;
            return Ok(None);
        }

        if let Some(cache_value) = self.storage.get(key) {
            // 更新访问统计
            self.update_access_stats(key).await;
            
            // 解压缩数据
            let decompressed = self.compressor.decompress(
                &cache_value.data,
                cache_value.is_compressed,
            )?;
            
            self.metrics.record_cache_hit(CacheLayer::Memory).await;
            self.metrics.record_operation_latency(CacheOperation::Get, start_time.elapsed()).await;
            
            cache_log!(self.logging_config, debug, "L1 缓存命中: {}", key);
            Ok(Some(decompressed.data))
        } else {
            self.metrics.record_cache_miss().await;
            self.metrics.record_operation_latency(CacheOperation::Get, start_time.elapsed()).await;
            
            cache_log!(self.logging_config, debug, "L1 缓存未命中: {}", key);
            Ok(None)
        }
    }

    /// 设置缓存值
    pub async fn set(&self, key: String, value: Bytes, ttl_seconds: Option<u64>) -> CacheResult<()> {
        let start_time = Instant::now();
        
        // 压缩数据
        let compression_result = self.compressor.compress(&value)?;
        
        // 创建缓存值
        let cache_value = if compression_result.is_compressed {
            CacheValue::new_compressed(
                compression_result.compressed_data.to_vec(),
                compression_result.original_size,
            )
        } else {
            CacheValue::new_uncompressed(compression_result.compressed_data.to_vec())
        };

        let value_size = cache_value.size();
        
        // 检查是否需要驱逐
        self.ensure_capacity(value_size).await?;
        
        // 插入数据
        let is_update = self.storage.contains_key(&key);
        
        if let Some(old_value) = self.storage.insert(key.clone(), cache_value) {
            // 更新内存使用量
            let old_size = old_value.size();
            self.memory_usage.fetch_sub(old_size, Ordering::Relaxed);
        } else {
            // 新增条目
            self.entry_count.fetch_add(1, Ordering::Relaxed);
        }
        
        self.memory_usage.fetch_add(value_size, Ordering::Relaxed);
        
        // 更新访问统计
        if !is_update {
            self.update_insertion_stats(&key).await;
        }
        self.update_access_stats(&key).await;
        
        // 设置 TTL
        if ttl_seconds.is_some() || self.ttl_manager.get_ttl(&key).await.is_none() {
            self.ttl_manager.add_key(key.clone(), ttl_seconds).await?;
        }
        
        // 记录指标
        self.metrics.record_cache_operation(CacheOperation::Set).await;
        self.metrics.record_memory_usage(CacheLayer::Memory, self.memory_usage.load(Ordering::Relaxed) as u64).await;
        self.metrics.record_operation_latency(CacheOperation::Set, start_time.elapsed()).await;
        
        if compression_result.is_compressed {
            self.metrics.record_compression(
                compression_result.original_size as u64,
                compression_result.compressed_size as u64,
            ).await;
        }
        
        cache_log!(self.logging_config, debug, "L1 缓存设置: {} ({}压缩)", 
            key, if compression_result.is_compressed { "已" } else { "未" });
        
        Ok(())
    }

    /// 删除缓存值
    pub async fn delete(&self, key: &str) -> CacheResult<bool> {
        let start_time = Instant::now();
        
        let removed = self.remove_internal(key).await;
        
        self.metrics.record_cache_operation(CacheOperation::Delete).await;
        self.metrics.record_operation_latency(CacheOperation::Delete, start_time.elapsed()).await;
        
        if removed {
            cache_log!(self.logging_config, debug, "L1 缓存删除: {}", key);
        }
        
        Ok(removed)
    }

    /// 清空缓存
    pub async fn clear(&self) -> CacheResult<()> {
        let start_time = Instant::now();
        
        let old_count = self.entry_count.load(Ordering::Relaxed);
        
        self.storage.clear();
        self.lru_order.lock().await.clear();
        self.lfu_counter.clear();
        self.fifo_order.lock().await.clear();
        
        self.memory_usage.store(0, Ordering::Relaxed);
        self.entry_count.store(0, Ordering::Relaxed);
        
        self.metrics.record_cache_operation(CacheOperation::Clear).await;
        self.metrics.record_memory_usage(CacheLayer::Memory, 0).await;
        
        cache_log!(self.logging_config, info, "L1 缓存已清空，删除了 {} 个条目", old_count);
        
        Ok(())
    }

    /// 获取缓存统计信息
    pub async fn get_stats(&self) -> L1CacheStats {
        let eviction_stats = self.eviction_stats.read().clone();
        
        L1CacheStats {
            entry_count: self.entry_count.load(Ordering::Relaxed),
            memory_usage: self.memory_usage.load(Ordering::Relaxed),
            max_memory: self.config.max_memory,
            max_entries: self.config.max_entries,
            memory_utilization: self.memory_usage.load(Ordering::Relaxed) as f64 / self.config.max_memory as f64,
            entry_utilization: self.entry_count.load(Ordering::Relaxed) as f64 / self.config.max_entries as f64,
            eviction_stats,
        }
    }

    /// 检查是否包含键
    pub fn contains_key(&self, key: &str) -> bool {
        self.storage.contains_key(key)
    }

    /// 获取所有键
    pub fn keys(&self) -> Vec<String> {
        self.storage.iter().map(|entry| entry.key().clone()).collect()
    }

    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.entry_count.load(Ordering::Relaxed)
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 内部删除方法
    async fn remove_internal(&self, key: &str) -> bool {
        if let Some((_, old_value)) = self.storage.remove(key) {
            // 更新内存使用量和条目数
            let old_size = old_value.size();
            self.memory_usage.fetch_sub(old_size, Ordering::Relaxed);
            self.entry_count.fetch_sub(1, Ordering::Relaxed);
            
            // 清理访问统计
            self.cleanup_access_stats(key).await;
            
            // 移除 TTL
            self.ttl_manager.remove_key(key).await;
            
            true
        } else {
            false
        }
    }

    /// 确保有足够的容量
    async fn ensure_capacity(&self, required_size: usize) -> CacheResult<()> {
        let current_memory = self.memory_usage.load(Ordering::Relaxed);
        let current_entries = self.entry_count.load(Ordering::Relaxed);
        
        // 检查内存限制
        if current_memory + required_size > self.config.max_memory {
            let needed_space = current_memory + required_size - self.config.max_memory;
            if self.evict_by_memory(required_size).await.is_err() {
                return Err(CacheError::out_of_memory(needed_space));
            }
        }
        
        // 检查条目数限制
        if current_entries >= self.config.max_entries {
            if self.evict_by_count(1).await.is_err() {
                return Err(CacheError::cache_full(current_entries, self.config.max_entries));
            }
        }
        
        Ok(())
    }

    /// 按内存使用量驱逐
    async fn evict_by_memory(&self, required_size: usize) -> CacheResult<()> {
        let target_memory = self.config.max_memory - required_size;
        let mut evicted_bytes = 0;
        let mut evicted_count = 0;
        
        while self.memory_usage.load(Ordering::Relaxed) > target_memory && !self.storage.is_empty() {
            if let Some(key) = self.select_eviction_candidate().await {
                if let Some((_, value)) = self.storage.remove(&key) {
                    let size = value.size();
                    evicted_bytes += size;
                    evicted_count += 1;
                    
                    self.memory_usage.fetch_sub(size, Ordering::Relaxed);
                    self.entry_count.fetch_sub(1, Ordering::Relaxed);
                    
                    self.cleanup_access_stats(&key).await;
                    self.ttl_manager.remove_key(&key).await;
                    
                    cache_log!(self.logging_config, debug, "驱逐键: {} ({}字节)", key, size);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        if evicted_count > 0 {
            self.update_eviction_stats(evicted_count, evicted_bytes).await;
            self.metrics.record_cache_eviction().await;
            
            cache_log!(self.logging_config, info, "内存驱逐完成: {} 个条目，{} 字节", 
                evicted_count, evicted_bytes);
        }
        
        Ok(())
    }

    /// 按条目数驱逐
    async fn evict_by_count(&self, required_count: usize) -> CacheResult<()> {
        let mut evicted_bytes = 0;
        let mut evicted_count = 0;
        
        for _ in 0..required_count {
            if let Some(key) = self.select_eviction_candidate().await {
                if let Some((_, value)) = self.storage.remove(&key) {
                    let size = value.size();
                    evicted_bytes += size;
                    evicted_count += 1;
                    
                    self.memory_usage.fetch_sub(size, Ordering::Relaxed);
                    self.entry_count.fetch_sub(1, Ordering::Relaxed);
                    
                    self.cleanup_access_stats(&key).await;
                    self.ttl_manager.remove_key(&key).await;
                    
                    cache_log!(self.logging_config, debug, "驱逐键: {} ({}字节)", key, size);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        if evicted_count > 0 {
            self.update_eviction_stats(evicted_count, evicted_bytes).await;
            self.metrics.record_cache_eviction().await;
            
            cache_log!(self.logging_config, info, "条目驱逐完成: {} 个条目，{} 字节", 
                evicted_count, evicted_bytes);
        }
        
        Ok(())
    }

    /// 选择驱逐候选者
    async fn select_eviction_candidate(&self) -> Option<String> {
        match self.config.eviction_strategy {
            EvictionStrategy::Lru => self.select_lru_candidate().await,
            EvictionStrategy::Lfu => self.select_lfu_candidate().await,
            EvictionStrategy::Fifo => self.select_fifo_candidate().await,
            EvictionStrategy::LruLfu => self.select_lru_lfu_candidate().await,
            EvictionStrategy::TtlBased => self.select_ttl_candidate().await,
        }
    }

    /// 选择 LRU 候选者
    async fn select_lru_candidate(&self) -> Option<String> {
        let mut lru_order = self.lru_order.lock().await;
        lru_order.pop_front()
    }

    /// 选择 LFU 候选者
    async fn select_lfu_candidate(&self) -> Option<String> {
        let mut min_count = u64::MAX;
        let mut candidate = None;
        
        for entry in self.lfu_counter.iter() {
            let count = entry.value().load(Ordering::Relaxed);
            if count < min_count {
                min_count = count;
                candidate = Some(entry.key().clone());
            }
        }
        
        candidate
    }

    /// 选择 FIFO 候选者
    async fn select_fifo_candidate(&self) -> Option<String> {
        let mut fifo_order = self.fifo_order.lock().await;
        fifo_order.pop_front()
    }

    /// 选择 LRU+LFU 混合候选者
    async fn select_lru_lfu_candidate(&self) -> Option<String> {
        // 70% 概率使用 LRU，30% 概率使用 LFU
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos().hash(&mut hasher);
        let random_value = (hasher.finish() % 100) as f64 / 100.0;
        
        if random_value < 0.7 {
            self.select_lru_candidate().await
        } else {
            self.select_lfu_candidate().await
        }
    }

    /// 选择基于 TTL 的候选者
    async fn select_ttl_candidate(&self) -> Option<String> {
        // 优先选择即将过期的键
        let expired_keys = self.ttl_manager.get_expired_keys(1).await;
        if !expired_keys.is_empty() {
            return Some(expired_keys[0].clone());
        }
        
        // 如果没有过期键，回退到 LRU
        self.select_lru_candidate().await
    }

    /// 更新访问统计
    async fn update_access_stats(&self, key: &str) {
        // 更新 LRU
        let mut lru_order = self.lru_order.lock().await;
        lru_order.retain(|k| k != key);
        lru_order.push_back(key.to_string());
        drop(lru_order);
        
        // 更新 LFU
        self.lfu_counter.entry(key.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 更新插入统计
    async fn update_insertion_stats(&self, key: &str) {
        // 更新 FIFO
        let mut fifo_order = self.fifo_order.lock().await;
        fifo_order.push_back(key.to_string());
    }

    /// 清理访问统计
    async fn cleanup_access_stats(&self, key: &str) {
        // 清理 LRU
        let mut lru_order = self.lru_order.lock().await;
        lru_order.retain(|k| k != key);
        drop(lru_order);
        
        // 清理 LFU
        self.lfu_counter.remove(key);
        
        // 清理 FIFO
        let mut fifo_order = self.fifo_order.lock().await;
        fifo_order.retain(|k| k != key);
    }

    /// 更新驱逐统计
    async fn update_eviction_stats(&self, count: usize, bytes: usize) {
        let mut stats = self.eviction_stats.write();
        stats.total_evictions += count as u64;
        stats.evicted_bytes += bytes as u64;
        
        match self.config.eviction_strategy {
            EvictionStrategy::Lru => stats.lru_evictions += count as u64,
            EvictionStrategy::Lfu => stats.lfu_evictions += count as u64,
            EvictionStrategy::Fifo => stats.fifo_evictions += count as u64,
            EvictionStrategy::TtlBased => stats.ttl_evictions += count as u64,
            EvictionStrategy::LruLfu => {
                // 按比例分配
                stats.lru_evictions += (count as f64 * 0.7) as u64;
                stats.lfu_evictions += (count as f64 * 0.3) as u64;
            }
        }
    }
}

/// L1 缓存统计信息
#[derive(Debug, Clone)]
pub struct L1CacheStats {
    pub entry_count: usize,
    pub memory_usage: usize,
    pub max_memory: usize,
    pub max_entries: usize,
    pub memory_utilization: f64,
    pub entry_utilization: f64,
    pub eviction_stats: EvictionStats,
}

impl L1CacheStats {
    /// 格式化统计信息
    pub fn format(&self) -> String {
        format!(
            "L1 缓存统计:\n\
             条目数: {}/{}({:.1}%)\n\
             内存使用: {}/{} bytes ({:.1}%)\n\
             总驱逐: {} 次 ({} bytes)\n\
             LRU驱逐: {}, LFU驱逐: {}, FIFO驱逐: {}, TTL驱逐: {}",
            self.entry_count, self.max_entries, self.entry_utilization * 100.0,
            self.memory_usage, self.max_memory, self.memory_utilization * 100.0,
            self.eviction_stats.total_evictions, self.eviction_stats.evicted_bytes,
            self.eviction_stats.lru_evictions, self.eviction_stats.lfu_evictions,
            self.eviction_stats.fifo_evictions, self.eviction_stats.ttl_evictions
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{L1Config, LoggingConfig, CompressionConfig, TtlConfig};
    use crate::compression::Compressor;
    use crate::ttl::TtlManager;
    use crate::metrics::MetricsCollector;
    use bytes::Bytes;

    async fn create_test_cache() -> L1Cache {
        let l1_config = L1Config {
            max_memory: 1024 * 1024, // 1MB
            max_entries: 1000,
            eviction_strategy: EvictionStrategy::Lru,
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
            active_expiration: true,
        };
        
        let compressor = Compressor::new(compression_config);
        let ttl_manager = Arc::new(TtlManager::new(ttl_config, logging_config.clone()).await.unwrap());
        let metrics = Arc::new(MetricsCollector::new().await.unwrap());
        
        L1Cache::new(l1_config, logging_config, compressor, ttl_manager, metrics).await.unwrap()
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = create_test_cache().await;
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let cache = create_test_cache().await;
        let key = "test_key".to_string();
        let value = Bytes::from("test_value");
        
        cache.set(key.clone(), value.clone(), None).await.unwrap();
        
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
    }

    #[tokio::test]
    async fn test_delete() {
        let cache = create_test_cache().await;
        let key = "test_key".to_string();
        let value = Bytes::from("test_value");
        
        cache.set(key.clone(), value, None).await.unwrap();
        assert!(cache.contains_key(&key));
        
        let deleted = cache.delete(&key).await.unwrap();
        assert!(deleted);
        assert!(!cache.contains_key(&key));
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = create_test_cache().await;
        
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = Bytes::from(format!("value_{}", i));
            cache.set(key, value, None).await.unwrap();
        }
        
        assert_eq!(cache.len(), 10);
        
        cache.clear().await.unwrap();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_eviction() {
        let mut l1_config = L1Config {
            max_memory: 1024, // 很小的内存限制
            max_entries: 5,    // 很小的条目限制
            eviction_strategy: EvictionStrategy::Lru,
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
            enable_lz4: false, // 禁用压缩以便测试
            compression_threshold: 1000,
            compression_level: 4,
            auto_compression: false,
            min_compression_ratio: 0.8,
        };
        
        let ttl_config = TtlConfig {
            default_ttl: None,
            max_ttl: 3600,
            cleanup_interval: 60,
            max_cleanup_entries: 100,
            lazy_expiration: true,
            active_expiration: false,
        };
        
        let compressor = Compressor::new(compression_config);
        let ttl_manager = Arc::new(TtlManager::new(ttl_config, logging_config.clone()).await.unwrap());
        let metrics = Arc::new(MetricsCollector::new().await.unwrap());
        
        let cache = L1Cache::new(l1_config, logging_config, compressor, ttl_manager, metrics).await.unwrap();
        
        // 插入超过限制的条目
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = Bytes::from(vec![b'x'; 200]); // 200字节的值
            cache.set(key, value, None).await.unwrap();
        }
        
        // 应该触发驱逐
        assert!(cache.len() <= 5);
        
        let stats = cache.get_stats().await;
        assert!(stats.eviction_stats.total_evictions > 0);
    }
}