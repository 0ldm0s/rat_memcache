//! MelangeDB 适配器模块
//!
//! 为 rat_memcache 提供与 MelangeDB 兼容的接口，支持高性能存储操作

use crate::error::{CacheError, CacheResult};
use std::path::Path;
use std::sync::Arc;
use bytes::Bytes;

/// 压缩算法枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompressionAlgorithm {
    None,
    Lz4,
    Zstd,
}

impl Default for CompressionAlgorithm {
    fn default() -> Self {
        CompressionAlgorithm::Lz4
    }
}

/// 缓存预热策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CacheWarmupStrategy {
    /// 无预热
    None,
    /// 预热最近访问的数据
    Recent,
    /// 预热热点数据
    Hot,
    /// 全部预热
    Full,
}

impl Default for CacheWarmupStrategy {
    fn default() -> Self {
        CacheWarmupStrategy::Recent
    }
}

/// MelangeDB 适配器配置
#[derive(Debug, Clone)]
pub struct MelangeConfig {
    pub compression_algorithm: CompressionAlgorithm,
    pub cache_size_mb: usize,
    pub max_file_size_mb: usize,
    pub enable_statistics: bool,
    /// 智能flush配置
    pub smart_flush_enabled: bool,
    pub smart_flush_base_interval_ms: usize,
    pub smart_flush_min_interval_ms: usize,
    pub smart_flush_max_interval_ms: usize,
    pub smart_flush_write_rate_threshold: usize,
    pub smart_flush_accumulated_bytes_threshold: usize,
    /// 缓存预热策略
    pub cache_warmup_strategy: CacheWarmupStrategy,
    /// ZSTD压缩级别（仅当使用ZSTD压缩时有效）
    pub zstd_compression_level: Option<i32>,
}

impl Default for MelangeConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

impl MelangeConfig {
    /// 平衡模式 - 适合大多数场景的便捷方法
    pub fn balanced() -> Self {
        Self {
            compression_algorithm: CompressionAlgorithm::Lz4,
            cache_size_mb: 512,
            max_file_size_mb: 1024,
            enable_statistics: true,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 1000,
            smart_flush_write_rate_threshold: 10000,
            smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024,
            cache_warmup_strategy: CacheWarmupStrategy::Recent,
            zstd_compression_level: None, // LZ4压缩，不需要ZSTD级别
        }
    }

    /// 高性能模式 - 优先考虑性能的便捷方法
    pub fn high_performance() -> Self {
        Self {
            compression_algorithm: CompressionAlgorithm::None,
            cache_size_mb: 1024,
            max_file_size_mb: 2048,
            enable_statistics: true,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 50,
            smart_flush_min_interval_ms: 5,
            smart_flush_max_interval_ms: 200,
            smart_flush_write_rate_threshold: 20000,
            smart_flush_accumulated_bytes_threshold: 2 * 1024 * 1024,
            cache_warmup_strategy: CacheWarmupStrategy::Hot,
            zstd_compression_level: None, // 无压缩，不需要ZSTD级别
        }
    }

    /// 存储优化模式 - 优先考虑存储空间的便捷方法
    pub fn storage_optimized() -> Self {
        Self {
            compression_algorithm: CompressionAlgorithm::Zstd,
            cache_size_mb: 256,
            max_file_size_mb: 512,
            enable_statistics: true,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 200,
            smart_flush_min_interval_ms: 50,
            smart_flush_max_interval_ms: 2000,
            smart_flush_write_rate_threshold: 5000,
            smart_flush_accumulated_bytes_threshold: 8 * 1024 * 1024,
            cache_warmup_strategy: CacheWarmupStrategy::Full,
            zstd_compression_level: Some(6), // ZSTD压缩，设置级别
        }
    }

    /// 设置压缩算法
    pub fn with_compression(mut self, algorithm: CompressionAlgorithm) -> Self {
        self.compression_algorithm = algorithm;
        // 如果不是ZSTD压缩，清除ZSTD级别设置
        if algorithm != CompressionAlgorithm::Zstd {
            self.zstd_compression_level = None;
        }
        self
    }

    /// 设置缓存大小（MB）
    pub fn with_cache_size(mut self, size_mb: usize) -> Self {
        self.cache_size_mb = size_mb;
        self
    }

    /// 设置最大文件大小（MB）
    pub fn with_max_file_size(mut self, size_mb: usize) -> Self {
        self.max_file_size_mb = size_mb;
        self
    }

    /// 启用或禁用统计信息
    pub fn with_statistics(mut self, enabled: bool) -> Self {
        self.enable_statistics = enabled;
        self
    }

    /// 配置智能flush
    pub fn with_smart_flush(
        mut self,
        enabled: bool,
        base_interval_ms: usize,
        min_interval_ms: usize,
        max_interval_ms: usize,
        write_rate_threshold: usize,
        accumulated_bytes_threshold: usize,
    ) -> Self {
        self.smart_flush_enabled = enabled;
        self.smart_flush_base_interval_ms = base_interval_ms;
        self.smart_flush_min_interval_ms = min_interval_ms;
        self.smart_flush_max_interval_ms = max_interval_ms;
        self.smart_flush_write_rate_threshold = write_rate_threshold;
        self.smart_flush_accumulated_bytes_threshold = accumulated_bytes_threshold;
        self
    }

    /// 设置缓存预热策略
    pub fn with_warmup_strategy(mut self, strategy: CacheWarmupStrategy) -> Self {
        self.cache_warmup_strategy = strategy;
        self
    }

    /// 设置ZSTD压缩级别（仅在ZSTD压缩时有效）
    pub fn with_zstd_level(mut self, level: i32) -> Self {
        if self.compression_algorithm == CompressionAlgorithm::Zstd {
            self.zstd_compression_level = Some(level);
        }
        self
    }
}

/// 批量操作项
#[derive(Debug, Clone)]
pub enum BatchOperation {
    Insert { key: Vec<u8>, value: Vec<u8> },
    Remove { key: Vec<u8> },
}

/// MelangeDB 适配器
///
/// 提供高性能的键值存储操作，支持压缩和批量操作
#[derive(Debug)]
pub struct MelangeAdapter {
    db: Arc<DbWrapper>,
    config: MelangeConfig,
}

// 使用 trait 对象来隐藏具体实现，保持单一职责
trait DatabaseBackend: Send + Sync + std::fmt::Debug {
    fn get(&self, key: &[u8]) -> CacheResult<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> CacheResult<()>;
    fn delete(&self, key: &[u8]) -> CacheResult<()>;
    fn batch_write(&self, operations: &[BatchOperation]) -> CacheResult<()>;
    fn prefix_iter(&self, prefix: &[u8]) -> CacheResult<Vec<(Vec<u8>, Vec<u8>)>>;
    fn clear(&self) -> CacheResult<()>;
    fn get_statistics(&self) -> CacheResult<DatabaseStats>;
}

// 统一的统计信息结构
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub disk_reads: u64,
    pub disk_writes: u64,
    pub total_size_bytes: u64,
    pub compression_ratio: f64,
}

// 实际的 MelangeDB 实现
#[derive(Debug)]
struct MelangeBackend {
    db: melange_db::Db,
}

impl DatabaseBackend for MelangeBackend {
    fn get(&self, key: &[u8]) -> CacheResult<Option<Vec<u8>>> {
        self.db.get(key)
            .map(|opt| opt.map(|inline_array| inline_array.to_vec()))
            .map_err(|e| CacheError::melange_db_error(&format!("读取失败: {}", e)))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> CacheResult<()> {
        let _ = self.db.insert(key, value)
            .map_err(|e| CacheError::melange_db_error(&format!("写入失败: {}", e)))?;
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> CacheResult<()> {
        let _ = self.db.remove(key)
            .map_err(|e| CacheError::melange_db_error(&format!("删除失败: {}", e)))?;
        Ok(())
    }

    fn batch_write(&self, operations: &[BatchOperation]) -> CacheResult<()> {
        let mut batch = melange_db::Batch::default();

        for operation in operations {
            match operation {
                BatchOperation::Insert { key, value } => {
                    batch.insert(key.as_slice(), value.as_slice());
                }
                BatchOperation::Remove { key } => {
                    batch.remove(key.as_slice());
                }
            }
        }

        self.db.apply_batch(batch)
            .map_err(|e| CacheError::melange_db_error(&format!("批量写入失败: {}", e)))?;
        Ok(())
    }

    fn prefix_iter(&self, prefix: &[u8]) -> CacheResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();
        let iter = self.db.iter();

        for item in iter {
            let (key, value) = item
                .map_err(|e| CacheError::melange_db_error(&format!("迭代失败: {}", e)))?;

            if key.starts_with(prefix) {
                results.push((key.to_vec(), value.to_vec()));
            }
        }

        Ok(results)
    }

    fn clear(&self) -> CacheResult<()> {
        // 获取所有键并删除
        let all_keys = self.prefix_iter(&[])?;
        let operations: Vec<BatchOperation> = all_keys
            .into_iter()
            .map(|(key, _)| BatchOperation::Remove { key })
            .collect();

        self.batch_write(&operations)?;
        Ok(())
    }

    fn get_statistics(&self) -> CacheResult<DatabaseStats> {
        // MelangeDB 的统计信息
        Ok(DatabaseStats {
            cache_hits: 0,  // 需要根据实际 API 调整
            cache_misses: 0,
            disk_reads: 0,
            disk_writes: 0,
            total_size_bytes: 0,
            compression_ratio: 0.0,
        })
    }
}

// 包装器以便统一处理
struct DbWrapper {
    backend: Box<dyn DatabaseBackend>,
}

impl std::fmt::Debug for DbWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DbWrapper")
    }
}

impl MelangeAdapter {
    /// 创建新的 MelangeDB 适配器
    pub fn new<P: AsRef<Path>>(
        path: P,
        config: MelangeConfig,
    ) -> CacheResult<Self> {
        let melange_config = create_melange_config(&config);

        let db = melange_config.path(path).open()
            .map_err(|e| CacheError::melange_db_error(&format!("打开 MelangeDB 失败: {}", e)))?;

        let backend = Box::new(MelangeBackend { db });

        Ok(Self {
            db: Arc::new(DbWrapper { backend }),
            config,
        })
    }

    /// 获取键对应的值
    pub fn get(&self, key: &[u8]) -> CacheResult<Option<Vec<u8>>> {
        self.db.backend.get(key)
    }

    /// 设置键值对
    pub fn put(&self, key: &[u8], value: &[u8]) -> CacheResult<()> {
        self.db.backend.put(key, value)
    }

    /// 删除键
    pub fn delete(&self, key: &[u8]) -> CacheResult<()> {
        self.db.backend.delete(key)
    }

    /// 批量写入操作
    pub fn batch_write(&self, operations: Vec<BatchOperation>) -> CacheResult<()> {
        self.db.backend.batch_write(&operations)
    }

    /// 前缀迭代
    pub fn prefix_iter(&self, prefix: &[u8]) -> CacheResult<Vec<(Vec<u8>, Vec<u8>)>> {
        self.db.backend.prefix_iter(prefix)
    }

    /// 清空数据库
    pub fn clear(&self) -> CacheResult<()> {
        self.db.backend.clear()
    }

    /// 获取统计信息
    pub fn get_statistics(&self) -> CacheResult<DatabaseStats> {
        self.db.backend.get_statistics()
    }

    /// 创建批量操作
    pub fn insert_op(key: &[u8], value: &[u8]) -> BatchOperation {
        BatchOperation::Insert {
            key: key.to_vec(),
            value: value.to_vec(),
        }
    }

    /// 创建删除操作
    pub fn delete_op(key: &[u8]) -> BatchOperation {
        BatchOperation::Remove {
            key: key.to_vec(),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &MelangeConfig {
        &self.config
    }
}

// 辅助函数：创建 MelangeDB 配置
fn create_melange_config(config: &MelangeConfig) -> melange_db::Config {
    let mut melange_config = melange_db::Config::new()
        .cache_capacity_bytes(config.cache_size_mb * 1024 * 1024)
        .compression_algorithm(match config.compression_algorithm {
            CompressionAlgorithm::None => melange_db::CompressionAlgorithm::None,
            CompressionAlgorithm::Lz4 => melange_db::CompressionAlgorithm::Lz4,
            CompressionAlgorithm::Zstd => melange_db::CompressionAlgorithm::Zstd,
        })
        .flush_every_ms(None); // 禁用传统自动flush，使用智能flush

    // 仅在ZSTD压缩时设置压缩级别
    if let Some(level) = config.zstd_compression_level {
        melange_config = melange_config.zstd_compression_level(level);
    }

    // 配置智能flush
    melange_config.smart_flush_config = melange_db::smart_flush::SmartFlushConfig {
        enabled: config.smart_flush_enabled,
        base_interval_ms: config.smart_flush_base_interval_ms,
        min_interval_ms: config.smart_flush_min_interval_ms,
        max_interval_ms: config.smart_flush_max_interval_ms,
        write_rate_threshold: config.smart_flush_write_rate_threshold as u64,
        accumulated_bytes_threshold: config.smart_flush_accumulated_bytes_threshold,
    };

    // 配置缓存预热策略
    melange_config.cache_warmup_strategy = match config.cache_warmup_strategy {
        CacheWarmupStrategy::None => melange_db::CacheWarmupStrategy::None,
        CacheWarmupStrategy::Recent => melange_db::CacheWarmupStrategy::Recent,
        CacheWarmupStrategy::Hot => melange_db::CacheWarmupStrategy::Hot,
        CacheWarmupStrategy::Full => melange_db::CacheWarmupStrategy::Full,
    };

    melange_config
}

// 便捷函数：直接操作 Bytes 类型
impl MelangeAdapter {
    /// 获取键对应的值（Bytes 版本）
    pub fn get_bytes(&self, key: &[u8]) -> CacheResult<Option<Bytes>> {
        self.get(key).map(|opt| opt.map(Bytes::from))
    }

    /// 设置键值对（Bytes 版本）
    pub fn put_bytes(&self, key: &[u8], value: &Bytes) -> CacheResult<()> {
        self.put(key, value.as_ref())
    }

    /// 批量写入（Bytes 版本）
    pub fn batch_write_bytes(&self, operations: Vec<(Vec<u8>, Bytes)>) -> CacheResult<()> {
        let ops: Vec<BatchOperation> = operations
            .into_iter()
            .map(|(key, value)| BatchOperation::Insert { key, value: value.to_vec() })
            .collect();
        self.batch_write(ops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_melange_adapter_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = MelangeConfig::default();

        let adapter = MelangeAdapter::new(temp_dir.path(), config).unwrap();
        assert_eq!(adapter.config().compression_algorithm, CompressionAlgorithm::Lz4);
    }

    #[test]
    fn test_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = MelangeConfig::default();
        let adapter = MelangeAdapter::new(temp_dir.path(), config).unwrap();

        // 测试插入和获取
        let key = b"test_key";
        let value = b"test_value";

        adapter.put(key, value).unwrap();
        let retrieved = adapter.get(key).unwrap();
        assert_eq!(retrieved, Some(value.to_vec()));

        // 测试删除
        adapter.delete(key).unwrap();
        let retrieved = adapter.get(key).unwrap();
        assert_eq!(retrieved, None);
    }

    #[test]
    fn test_batch_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = MelangeConfig::default();
        let adapter = MelangeAdapter::new(temp_dir.path(), config).unwrap();

        // 准备批量操作
        let operations = vec![
            MelangeAdapter::insert_op(b"key1", b"value1"),
            MelangeAdapter::insert_op(b"key2", b"value2"),
            MelangeAdapter::delete_op(b"key1"),
        ];

        adapter.batch_write(operations).unwrap();

        // 验证结果
        assert_eq!(adapter.get(b"key1").unwrap(), None);
        assert_eq!(adapter.get(b"key2").unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_prefix_iteration() {
        let temp_dir = TempDir::new().unwrap();
        let config = MelangeConfig::default();
        let adapter = MelangeAdapter::new(temp_dir.path(), config).unwrap();

        // 插入带前缀的键
        adapter.put(b"data:key1", b"value1").unwrap();
        adapter.put(b"data:key2", b"value2").unwrap();
        adapter.put(b"meta:key3", b"value3").unwrap();

        // 前缀迭代
        let results = adapter.prefix_iter(b"data:").unwrap();
        assert_eq!(results.len(), 2);

        let keys: Vec<&[u8]> = results.iter().map(|(k, _)| k.as_slice()).collect();
        assert!(keys.contains(&b"data:key1".as_slice()));
        assert!(keys.contains(&b"data:key2".as_slice()));
    }

    #[test]
    fn test_compression_algorithms() {
        let temp_dir = TempDir::new().unwrap();

        for compression in [CompressionAlgorithm::None, CompressionAlgorithm::Lz4] {
            let config = MelangeConfig {
                compression_algorithm: compression,
                ..Default::default()
            };

            let adapter = MelangeAdapter::new(temp_dir.path(), config).unwrap();
            let key = b"compression_test";
            let value = b"this is a test value for compression";

            adapter.put(key, value).unwrap();
            let retrieved = adapter.get(key).unwrap();
            assert_eq!(retrieved, Some(value.to_vec()));
        }
    }
}