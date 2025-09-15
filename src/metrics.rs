//! 指标模块
//!
//! 实现高性能指标收集

use crate::error::{CacheError, CacheResult};
use crate::types::{CacheLayer, CacheOperation};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;

/// 指标收集器
/// 使用读写分离机制
#[derive(Debug)]
pub struct MetricsCollector {
    /// 智能传输路由器用于指标数据传输（已移除）
    // router: Arc<SmartTransferRouter>,
    /// 读写分离的指标存储
    read_metrics: Arc<RwLock<HashMap<String, AtomicU64>>>,
    write_metrics: Arc<RwLock<HashMap<String, AtomicU64>>>,
    /// 启动时间
    start_time: Instant,
}

/// 指标键名常量
mod metric_keys {
    // 缓存操作指标
    pub const CACHE_HITS_L1: &str = "cache.hits.l1";
    pub const CACHE_HITS_L2: &str = "cache.hits.l2";
    pub const CACHE_MISSES: &str = "cache.misses";
    pub const CACHE_SETS: &str = "cache.sets";
    pub const CACHE_DELETES: &str = "cache.deletes";
    pub const CACHE_EXPIRES: &str = "cache.expires";
    pub const CACHE_EVICTIONS: &str = "cache.evictions";
    
    // 性能指标
    pub const OPERATION_LATENCY_GET: &str = "latency.get";
    pub const OPERATION_LATENCY_SET: &str = "latency.set";
    pub const OPERATION_LATENCY_DELETE: &str = "latency.delete";
    
    // 内存指标
    pub const MEMORY_USAGE_L1: &str = "memory.usage.l1";
    pub const MEMORY_USAGE_L2: &str = "memory.usage.l2";
    pub const MEMORY_ALLOCATED: &str = "memory.allocated";
    pub const MEMORY_FREED: &str = "memory.freed";
    
    // 压缩指标
    pub const COMPRESSION_RATIO: &str = "compression.ratio";
    pub const COMPRESSION_OPERATIONS: &str = "compression.operations";
    pub const COMPRESSION_BYTES_SAVED: &str = "compression.bytes_saved";
    
    // 网络传输指标
    pub const TRANSFER_BYTES_IN: &str = "transfer.bytes.in";
    pub const TRANSFER_BYTES_OUT: &str = "transfer.bytes.out";
    pub const TRANSFER_OPERATIONS: &str = "transfer.operations";
    
    // 错误指标
    pub const ERRORS_TOTAL: &str = "errors.total";
    pub const ERRORS_TIMEOUT: &str = "errors.timeout";
    pub const ERRORS_SERIALIZATION: &str = "errors.serialization";
    pub const ERRORS_COMPRESSION: &str = "errors.compression";
}

impl MetricsCollector {
    /// 创建新的指标收集器
    pub async fn new() -> CacheResult<Self> {
        Ok(Self {
            read_metrics: Arc::new(RwLock::new(HashMap::new())),
            write_metrics: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        })
    }

    /// 增加读指标计数
    pub async fn increment_read_metric(&self, key: &str, value: u64) {
        let read_metrics = self.read_metrics.read().await;
        if let Some(counter) = read_metrics.get(key) {
            counter.fetch_add(value, Ordering::Relaxed);
        } else {
            drop(read_metrics);
            let mut write_guard = self.read_metrics.write().await;
            write_guard.entry(key.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(value, Ordering::Relaxed);
        }
    }

    /// 增加写指标计数
    pub async fn increment_write_metric(&self, key: &str, value: u64) {
        let write_metrics = self.write_metrics.read().await;
        if let Some(counter) = write_metrics.get(key) {
            counter.fetch_add(value, Ordering::Relaxed);
        } else {
            drop(write_metrics);
            let mut write_guard = self.write_metrics.write().await;
            write_guard.entry(key.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(value, Ordering::Relaxed);
        }
    }

    /// 设置指标值
    pub async fn set_metric(&self, key: &str, value: u64, is_read_metric: bool) {
        if is_read_metric {
            let read_metrics = self.read_metrics.read().await;
            if let Some(counter) = read_metrics.get(key) {
                counter.store(value, Ordering::Relaxed);
            } else {
                drop(read_metrics);
                let mut write_guard = self.read_metrics.write().await;
                write_guard.entry(key.to_string())
                    .or_insert_with(|| AtomicU64::new(0))
                    .store(value, Ordering::Relaxed);
            }
        } else {
            let write_metrics = self.write_metrics.read().await;
            if let Some(counter) = write_metrics.get(key) {
                counter.store(value, Ordering::Relaxed);
            } else {
                drop(write_metrics);
                let mut write_guard = self.write_metrics.write().await;
                write_guard.entry(key.to_string())
                    .or_insert_with(|| AtomicU64::new(0))
                    .store(value, Ordering::Relaxed);
            }
        }
    }

    /// 获取指标值
    pub async fn get_metric(&self, key: &str, is_read_metric: bool) -> u64 {
        if is_read_metric {
            let read_metrics = self.read_metrics.read().await;
            read_metrics.get(key)
                .map(|counter| counter.load(Ordering::Relaxed))
                .unwrap_or(0)
        } else {
            let write_metrics = self.write_metrics.read().await;
            write_metrics.get(key)
                .map(|counter| counter.load(Ordering::Relaxed))
                .unwrap_or(0)
        }
    }

    /// 记录缓存命中
    pub async fn record_cache_hit(&self, layer: CacheLayer) {
        match layer {
            CacheLayer::Memory => {
                self.increment_read_metric(metric_keys::CACHE_HITS_L1, 1).await;
            }
            CacheLayer::Persistent => {
                self.increment_read_metric(metric_keys::CACHE_HITS_L2, 1).await;
            }
        }
    }

    /// 记录缓存未命中
    pub async fn record_cache_miss(&self) {
        self.increment_read_metric(metric_keys::CACHE_MISSES, 1).await;
    }

    /// 记录缓存操作
    pub async fn record_cache_operation(&self, operation: CacheOperation) {
        match operation {
            CacheOperation::Set => {
                self.increment_write_metric(metric_keys::CACHE_SETS, 1).await;
            }
            CacheOperation::Delete => {
                self.increment_write_metric(metric_keys::CACHE_DELETES, 1).await;
            }
            CacheOperation::Expire => {
                self.increment_write_metric(metric_keys::CACHE_EXPIRES, 1).await;
            }
            _ => {}
        }
    }

    /// 记录缓存驱逐
    pub async fn record_cache_eviction(&self) {
        self.increment_write_metric(metric_keys::CACHE_EVICTIONS, 1).await;
    }

    /// 记录操作延迟
    pub async fn record_operation_latency(&self, operation: CacheOperation, duration: Duration) {
        let latency_ms = duration.as_millis() as u64;
        
        let key = match operation {
            CacheOperation::Get => metric_keys::OPERATION_LATENCY_GET,
            CacheOperation::Set => metric_keys::OPERATION_LATENCY_SET,
            CacheOperation::Delete => metric_keys::OPERATION_LATENCY_DELETE,
            _ => return,
        };
        
        // 使用移动平均来记录延迟
        let current = self.get_metric(key, true).await;
        let new_avg = if current == 0 {
            latency_ms
        } else {
            (current * 9 + latency_ms) / 10  // 简单的移动平均
        };
        
        self.set_metric(key, new_avg, true).await;
    }

    /// 记录内存使用情况
    pub async fn record_memory_usage(&self, layer: CacheLayer, bytes: u64) {
        let key = match layer {
            CacheLayer::Memory => metric_keys::MEMORY_USAGE_L1,
            CacheLayer::Persistent => metric_keys::MEMORY_USAGE_L2,
        };
        
        self.set_metric(key, bytes, false).await;
    }

    /// 记录内存分配
    pub async fn record_memory_allocation(&self, bytes: u64) {
        self.increment_write_metric(metric_keys::MEMORY_ALLOCATED, bytes).await;
    }

    /// 记录内存释放
    pub async fn record_memory_deallocation(&self, bytes: u64) {
        self.increment_write_metric(metric_keys::MEMORY_FREED, bytes).await;
    }

    /// 记录压缩指标
    pub async fn record_compression(&self, original_size: u64, compressed_size: u64) {
        self.increment_write_metric(metric_keys::COMPRESSION_OPERATIONS, 1).await;
        
        if compressed_size < original_size {
            let bytes_saved = original_size - compressed_size;
            self.increment_write_metric(metric_keys::COMPRESSION_BYTES_SAVED, bytes_saved).await;
            
            // 更新压缩比率（百分比）
            let ratio = (compressed_size as f64 / original_size as f64 * 100.0) as u64;
            self.set_metric(metric_keys::COMPRESSION_RATIO, ratio, false).await;
        }
    }

    /// 记录网络传输
    pub async fn record_transfer(&self, bytes_in: u64, bytes_out: u64) {
        if bytes_in > 0 {
            self.increment_write_metric(metric_keys::TRANSFER_BYTES_IN, bytes_in).await;
        }
        if bytes_out > 0 {
            self.increment_write_metric(metric_keys::TRANSFER_BYTES_OUT, bytes_out).await;
        }
        self.increment_write_metric(metric_keys::TRANSFER_OPERATIONS, 1).await;
    }

    /// 记录错误
    pub async fn record_error(&self, error: &CacheError) {
        self.increment_write_metric(metric_keys::ERRORS_TOTAL, 1).await;
        
        // 根据错误类型记录具体错误指标
        match error {
            CacheError::SerializationError { .. } => {
                self.increment_write_metric(metric_keys::ERRORS_SERIALIZATION, 1).await;
            }
            CacheError::CompressionError { .. } => {
                self.increment_write_metric(metric_keys::ERRORS_COMPRESSION, 1).await;
            }
            _ => {}
        }
    }

    /// 获取所有指标的快照
    pub async fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        let read_metrics = self.read_metrics.read().await;
        let write_metrics = self.write_metrics.read().await;
        
        let mut snapshot = MetricsSnapshot::new();
        
        // 收集读指标
        for (key, counter) in read_metrics.iter() {
            snapshot.read_metrics.insert(key.clone(), counter.load(Ordering::Relaxed));
        }
        
        // 收集写指标
        for (key, counter) in write_metrics.iter() {
            snapshot.write_metrics.insert(key.clone(), counter.load(Ordering::Relaxed));
        }
        
        snapshot.uptime = self.start_time.elapsed();
        snapshot
    }

    /// 重置所有指标
    pub async fn reset_metrics(&self) {
        let read_metrics = self.read_metrics.write().await;
        let write_metrics = self.write_metrics.write().await;
        
        for counter in read_metrics.values() {
            counter.store(0, Ordering::Relaxed);
        }
        
        for counter in write_metrics.values() {
            counter.store(0, Ordering::Relaxed);
        }
    }

    /// 获取运行时间
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// 计算缓存命中率
    pub async fn calculate_hit_rate(&self) -> f64 {
        let l1_hits = self.get_metric(metric_keys::CACHE_HITS_L1, true).await;
        let l2_hits = self.get_metric(metric_keys::CACHE_HITS_L2, true).await;
        let misses = self.get_metric(metric_keys::CACHE_MISSES, true).await;
        
        let total_requests = l1_hits + l2_hits + misses;
        if total_requests == 0 {
            return 0.0;
        }
        
        (l1_hits + l2_hits) as f64 / total_requests as f64
    }

    /// 计算平均延迟
    pub async fn calculate_average_latency(&self) -> f64 {
        let get_latency = self.get_metric(metric_keys::OPERATION_LATENCY_GET, true).await;
        let set_latency = self.get_metric(metric_keys::OPERATION_LATENCY_SET, true).await;
        let delete_latency = self.get_metric(metric_keys::OPERATION_LATENCY_DELETE, true).await;
        
        let total_latency = get_latency + set_latency + delete_latency;
        if total_latency == 0 {
            return 0.0;
        }
        
        total_latency as f64 / 3.0
    }
}

/// 指标快照
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub read_metrics: HashMap<String, u64>,
    pub write_metrics: HashMap<String, u64>,
    pub uptime: Duration,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl MetricsSnapshot {
    /// 创建新的指标快照
    pub fn new() -> Self {
        Self {
            read_metrics: HashMap::new(),
            write_metrics: HashMap::new(),
            uptime: Duration::default(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// 获取指标值
    pub fn get_metric(&self, key: &str, is_read_metric: bool) -> u64 {
        if is_read_metric {
            self.read_metrics.get(key).copied().unwrap_or(0)
        } else {
            self.write_metrics.get(key).copied().unwrap_or(0)
        }
    }

    /// 计算缓存命中率
    pub fn hit_rate(&self) -> f64 {
        let l1_hits = self.get_metric(metric_keys::CACHE_HITS_L1, true);
        let l2_hits = self.get_metric(metric_keys::CACHE_HITS_L2, true);
        let misses = self.get_metric(metric_keys::CACHE_MISSES, true);
        
        let total_requests = l1_hits + l2_hits + misses;
        if total_requests == 0 {
            return 0.0;
        }
        
        (l1_hits + l2_hits) as f64 / total_requests as f64
    }

    /// 格式化为人类可读的字符串
    pub fn format(&self) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("=== 缓存指标快照 ({}UTC) ===\n", 
            self.timestamp.format("%Y-%m-%d %H:%M:%S ")));
        output.push_str(&format!("运行时间: {:.2}秒\n", self.uptime.as_secs_f64()));
        output.push_str(&format!("缓存命中率: {:.2}%\n", self.hit_rate() * 100.0));
        
        output.push_str("\n--- 读指标 ---\n");
        for (key, value) in &self.read_metrics {
            output.push_str(&format!("{}: {}\n", key, value));
        }
        
        output.push_str("\n--- 写指标 ---\n");
        for (key, value) in &self.write_metrics {
            output.push_str(&format!("{}: {}\n", key, value));
        }
        
        output
    }
}

impl Default for MetricsSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new().await;
        assert!(collector.is_ok());
    }

    #[tokio::test]
    async fn test_increment_metrics() {
        let collector = MetricsCollector::new().await.unwrap();
        
        collector.increment_read_metric("test.read", 5).await;
        collector.increment_write_metric("test.write", 10).await;
        
        assert_eq!(collector.get_metric("test.read", true).await, 5);
        assert_eq!(collector.get_metric("test.write", false).await, 10);
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let collector = MetricsCollector::new().await.unwrap();
        
        collector.record_cache_hit(CacheLayer::Memory).await;
        collector.record_cache_hit(CacheLayer::Persistent).await;
        collector.record_cache_miss().await;
        
        assert_eq!(collector.get_metric(metric_keys::CACHE_HITS_L1, true).await, 1);
        assert_eq!(collector.get_metric(metric_keys::CACHE_HITS_L2, true).await, 1);
        assert_eq!(collector.get_metric(metric_keys::CACHE_MISSES, true).await, 1);
    }

    #[tokio::test]
    async fn test_hit_rate_calculation() {
        let collector = MetricsCollector::new().await.unwrap();
        
        // 记录一些命中和未命中
        collector.record_cache_hit(CacheLayer::Memory).await;
        collector.record_cache_hit(CacheLayer::Memory).await;
        collector.record_cache_hit(CacheLayer::Persistent).await;
        collector.record_cache_miss().await;
        
        let hit_rate = collector.calculate_hit_rate().await;
        assert!((hit_rate - 0.75).abs() < 0.01); // 3/4 = 0.75
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let collector = MetricsCollector::new().await.unwrap();
        
        collector.increment_read_metric("test.metric", 42).await;
        
        let snapshot = collector.get_metrics_snapshot().await;
        assert_eq!(snapshot.get_metric("test.metric", true), 42);
        assert!(snapshot.uptime.as_millis() > 0);
    }

    #[tokio::test]
    async fn test_reset_metrics() {
        let collector = MetricsCollector::new().await.unwrap();
        
        collector.increment_read_metric("test.metric", 100).await;
        assert_eq!(collector.get_metric("test.metric", true).await, 100);
        
        collector.reset_metrics().await;
        assert_eq!(collector.get_metric("test.metric", true).await, 0);
    }
}