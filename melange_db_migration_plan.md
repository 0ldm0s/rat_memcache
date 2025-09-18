# Rat Memcache RocksDB 到 MelangeDB 迁移方案

## 概述

本文档详细说明将 `rat_memcache` 项目中的 RocksDB 持久化存储层替换为 `melange_db` 的方案，包括支持 LZ4 压缩和无压缩两种模式的实现。

## 当前架构分析

### 现有 RocksDB 使用情况
- **位置**: `src/l2_cache.rs`
- **主要功能**:
  - 数据持久化存储
  - 元数据管理（创建时间、访问统计等）
  - TTL 过期管理
  - 压缩支持（通过外部压缩器）

### 关键接口
```rust
// RocksDB 核心操作
db.get(&key)
db.put(&key, &value)
db.delete(&key)
db.write(batch)
db.prefix_iterator(prefix)
```

## MelangeDB 集成方案

### 1. 依赖配置调整

**修改 `Cargo.toml`**:

```toml
[features]
default = ["lz4-compression", "ttl-support", "metrics", "melange-storage"]

# 移除 rocksdb-storage
melange-storage = ["melange_db"]

[dependencies]
# 移除 rocksdb 依赖
melange_db = { path = "../melange_db", optional = true }
```

### 2. 配置扩展

**修改 `src/config.rs`**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Config {
    // 现有配置...

    #[serde(default = "default_compression_algorithm")]
    pub compression_algorithm: CompressionAlgorithm,

    #[serde(default = "default_cache_size")]
    pub cache_size_mb: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    None,
    Lz4,
    Zstd,
}

fn default_compression_algorithm() -> CompressionAlgorithm {
    CompressionAlgorithm::Lz4
}

fn default_cache_size() -> usize {
    512 // MB
}
```

### 3. MelangeDB 适配器

**创建 `src/melange_adapter.rs`**:

```rust
use crate::error::{CacheError, CacheResult};
use melange_db::{Config, Db, CompressionAlgorithm as MelangeCompression};
use std::path::Path;
use std::sync::Arc;

pub struct MelangeAdapter {
    db: Arc<Db>,
}

impl MelangeAdapter {
    pub fn new<P: AsRef<Path>>(
        path: P,
        compression: CompressionAlgorithm,
        cache_size_mb: usize,
    ) -> CacheResult<Self> {
        let mut config = Config::new();

        // 设置压缩算法
        let melange_compression = match compression {
            CompressionAlgorithm::None => MelangeCompression::None,
            CompressionAlgorithm::Lz4 => MelangeCompression::Lz4,
            CompressionAlgorithm::Zstd => MelangeCompression::Zstd,
        };
        config = config.compression_algorithm(melange_compression);

        // 设置缓存大小
        config = config.cache_size(cache_size_mb * 1024 * 1024);

        let db = config.path(path).open()
            .map_err(|e| CacheError::database_error(&format!("打开 MelangeDB 失败: {}", e)))?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn get(&self, key: &[u8]) -> CacheResult<Option<Vec<u8>>> {
        self.db.get(key)
            .map_err(|e| CacheError::database_error(&format!("读取失败: {}", e)))
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> CacheResult<()> {
        self.db.insert(key, value)
            .map_err(|e| CacheError::database_error(&format!("写入失败: {}", e)))?;
        Ok(())
    }

    pub fn delete(&self, key: &[u8]) -> CacheResult<()> {
        self.db.remove(key)
            .map_err(|e| CacheError::database_error(&format!("删除失败: {}", e)))?;
        Ok(())
    }

    pub fn batch_write(&self, operations: Vec<(Vec<u8>, Vec<u8>)>) -> CacheResult<()> {
        let mut batch = self.db.batch();
        for (key, value) in operations {
            batch.insert(&key, &value);
        }
        batch.commit()
            .map_err(|e| CacheError::database_error(&format!("批量写入失败: {}", e)))?;
        Ok(())
    }

    pub fn prefix_iter(&self, prefix: &[u8]) -> CacheResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();
        let iter = self.db.iter();

        for item in iter {
            let (key, value) = item
                .map_err(|e| CacheError::database_error(&format!("迭代失败: {}", e)))?;

            if key.starts_with(prefix) {
                results.push((key.to_vec(), value.to_vec()));
            }
        }

        Ok(results)
    }
}
```

### 4. L2Cache 重构

**修改 `src/l2_cache.rs`**:

```rust
// 替换导入
use crate::melange_adapter::MelangeAdapter;
// 移除 rocksdb 导入

#[derive(Debug)]
pub struct L2Cache {
    config: Arc<L2Config>,
    logging_config: Arc<LoggingConfig>,
    /// MelangeDB 适配器实例
    db: Arc<MelangeAdapter>,
    // 其他字段保持不变...
}

impl L2Cache {
    pub async fn new(
        config: L2Config,
        logging_config: LoggingConfig,
        compressor: Compressor,
        ttl_manager: Arc<TtlManager>,
        metrics: Arc<MetricsCollector>,
    ) -> CacheResult<Self> {
        // ... 配置验证和目录创建逻辑保持不变

        // 创建 MelangeDB 适配器
        let db = MelangeAdapter::new(
            &data_dir,
            config.compression_algorithm,
            config.cache_size_mb,
        )?;

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

        // ... 剩余初始化逻辑
    }

    // 修改所有数据库操作方法
    async fn get(&self, key: &str) -> CacheResult<Option<Bytes>> {
        // 使用 db.get() 替代 rocksdb 操作
        let data_key = Self::make_data_key(key);
        let metadata_key = Self::make_metadata_key(key);

        let data = self.db.get(&data_key)?;
        let metadata_bytes = self.db.get(&metadata_key)?;

        // ... 剩余逻辑保持不变
    }

    async fn set(&self, key: String, value: Bytes, ttl_seconds: Option<u64>) -> CacheResult<()> {
        // ... 压缩和元数据创建逻辑保持不变

        // 使用批量写入
        let operations = vec![
            (Self::make_data_key(&key), compression_result.compressed_data),
            (Self::make_metadata_key(&key), metadata_bytes),
        ];

        self.db.batch_write(operations)?;

        // ... 剩余逻辑
    }

    // 类似修改 delete、clear、keys 等方法
}
```

### 5. 压缩集成方案

MelangeDB 支持编译时特性选择压缩算法：

**特性配置**:
```toml
[features]
# 压缩特性（互斥）
compression-none = ["melange_db/compression-none"]
compression-lz4 = ["melange_db/compression-lz4"]
compression-zstd = ["melange_db/compression-zstd"]

# 默认使用 LZ4 压缩
default = ["compression-lz4", "ttl-support", "metrics", "melange-storage"]
```

**运行时验证**:
```rust
fn validate_compression_config(config: &L2Config) -> CacheResult<()> {
    #[cfg(feature = "compression-none")]
    if matches!(config.compression_algorithm, CompressionAlgorithm::Lz4 | CompressionAlgorithm::Zstd) {
        return Err(CacheError::config_error(
            "编译时禁用了压缩，但配置要求使用压缩"
        ));
    }

    #[cfg(feature = "compression-lz4")]
    if matches!(config.compression_algorithm, CompressionAlgorithm::Zstd) {
        return Err(CacheError::config_error(
            "编译时只支持 LZ4 压缩，但配置要求使用 Zstd"
        ));
    }

    // 类似验证其他特性组合
    Ok(())
}
```

### 6. 迁移步骤

1. **备份现有数据**: 导出重要的缓存数据
2. **逐步替换**:
   - 先实现 MelangeDB 适配器
   - 逐步替换 L2Cache 中的 RocksDB 调用
   - 保持接口兼容性
3. **测试验证**:
   - 功能测试：确保所有操作正常工作
   - 性能测试：对比迁移前后的性能差异
   - 压缩测试：验证不同压缩模式的效果
4. **数据迁移工具**（可选）:
   - 开发从 RocksDB 到 MelangeDB 的数据迁移工具

### 7. 性能优化考虑

- **缓存配置**: 根据内存大小调整 MelangeDB 缓存
- **批处理优化**: 利用 MelangeDB 的批量操作特性
- **压缩权衡**: 根据硬件性能选择合适的压缩级别
- **监控指标**: 添加 MelangeDB 特定的性能指标

## 预期收益

1. **性能提升**: MelangeDB 针对现代硬件优化
2. **内存效率**: 更好的缓存管理和内存使用
3. **压缩灵活性**: 支持多种压缩算法选择
4. **维护性**: 更现代的代码库和更好的文档

## 风险与缓解

1. **数据兼容性**: 确保数据格式兼容，提供迁移工具
2. **API 差异**: 通过适配器层封装差异
3. **性能回归**:  thorough 性能测试和调优
4. **特性缺失**: 评估 MelangeDB 是否支持所有必需特性

## 时间估算

- 适配器开发: 2-3 天
- L2Cache 重构: 3-4 天
- 测试验证: 2-3 天
- 性能调优: 2-3 天
- 文档更新: 1 天

**总计**: 约 2 周

## 后续优化

1. **异步操作**: 利用 MelangeDB 的异步特性
2. **高级特性**: 使用布隆过滤器、SIMD 优化等
3. **监控集成**: 更详细的性能指标收集
4. **自动化测试**: 增加集成测试和性能基准测试