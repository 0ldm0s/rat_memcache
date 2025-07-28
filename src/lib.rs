//! RatMemCache - 高性能双层缓存系统
//!
//! 基于 rat_quick_threshold 智能传输和 RocksDB 持久化存储的双层缓存系统，
//! 支持多种驱逐策略、TTL 管理、数据压缩和高性能指标收集。
//!
//! # 特性
//!
//! - **双层缓存**: 内存 L1 缓存 + RocksDB L2 持久化缓存
//! - **智能传输**: 基于 rat_quick_threshold 的高性能数据传输
//! - **多种策略**: 支持 LRU、LFU、FIFO、混合策略等
//! - **TTL 支持**: 灵活的过期时间管理
//! - **数据压缩**: LZ4 压缩算法，节省存储空间
//! - **高性能指标**: 基于 rat_quick_threshold 的读写分离指标系统
//! - **结构化日志**: 基于 zerg_creep 的高性能日志系统
//! - **异步设计**: 全异步 API，支持高并发
//!
//! # 快速开始
//!
//! ```rust,no_run
//! use rat_memcache::{RatMemCacheBuilder, CacheOptions};
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建缓存实例
//!     let cache = RatMemCacheBuilder::new()
//!         .development_preset()
//!         .build()
//!         .await?;
//!
//!     // 基本操作
//!     let key = "my_key".to_string();
//!     let value = Bytes::from("my_value");
//!
//!     // 设置缓存
//!     cache.set(key.clone(), value.clone()).await?;
//!
//!     // 获取缓存
//!     if let Some(retrieved) = cache.get(&key).await? {
//!         println!("Retrieved: {:?}", retrieved);
//!     }
//!
//!     // 设置带 TTL 的缓存
//!     cache.set_with_ttl("temp_key".to_string(), Bytes::from("temp_value"), 60).await?;
//!
//!     // 获取统计信息
//!     let stats = cache.get_stats().await?;
//!     println!("Cache stats: {}", stats.format());
//!
//!     // 关闭缓存
//!     cache.shutdown().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # 高级用法
//!
//! ## 自定义配置
//!
//! ```rust,no_run
//! use rat_memcache::{
//!     RatMemCacheBuilder, 
//!     config::{L1Config, L2Config, CompressionConfig, TtlConfig},
//!     types::EvictionStrategy
//! };
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let cache = RatMemCacheBuilder::new()
//!         .l1_config(L1Config {
//!             max_memory_size: 100 * 1024 * 1024, // 100MB
//!             eviction_strategy: EvictionStrategy::LruLfu,
//!             max_entries: 10000,
//!             enable_metrics: true,
//!         })
//!         .l2_config(L2Config {
//!             data_dir: PathBuf::from("/tmp/rat_cache"),
//!             max_disk_size: 1024 * 1024 * 1024, // 1GB
//!             enable_compression: true,
//!             compression_level: 6,
//!             ..Default::default()
//!         })
//!         .compression_config(CompressionConfig {
//!             enable_lz4: true,
//!             compression_threshold: 1024, // 1KB
//!             auto_compression: true,
//!             ..Default::default()
//!         })
//!         .ttl_config(TtlConfig {
//!             default_ttl: Some(3600), // 1小时
//!             max_ttl: 86400,          // 24小时
//!             cleanup_interval: 300,   // 5分钟
//!             ..Default::default()
//!         })
//!         .build()
//!         .await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## 缓存选项
//!
//! ```rust,no_run
//! use rat_memcache::{RatMemCache, CacheOptions};
//! use bytes::Bytes;
//!
//! async fn advanced_operations(cache: &RatMemCache) -> Result<(), Box<dyn std::error::Error>> {
//!     let key = "advanced_key".to_string();
//!     let value = Bytes::from("advanced_value");
//!
//!     // 强制写入 L2 缓存
//!     let options = CacheOptions {
//!         ttl_seconds: Some(300),
//!         force_l2: true,
//!         skip_l1: false,
//!         enable_compression: Some(true),
//!     };
//!     cache.set_with_options(key.clone(), value, &options).await?;
//!
//!     // 跳过 L1，直接从 L2 读取
//!     let get_options = CacheOptions {
//!         skip_l1: true,
//!         ..Default::default()
//!     };
//!     let retrieved = cache.get_with_options(&key, &get_options).await?;
//!
//!     Ok(())
//! }
//! ```

// 核心模块
pub mod cache;
pub mod config;
pub mod error;
pub mod types;

// 公开模块
pub mod logging;

// 内部模块
mod compression;
mod l1_cache;
mod l2_cache;
mod metrics;
mod ttl;

// 重新导出主要类型
pub use cache::{RatMemCache, RatMemCacheBuilder, CacheOptions};
pub use error::{CacheError, CacheResult};
pub use types::{CacheValue, EvictionStrategy, CacheLayer, CacheOperation};

// 重新导出配置类型
pub use config::{
    CacheConfig, CacheConfigBuilder,
    L1Config, L2Config, CompressionConfig, TtlConfig, 
    PerformanceConfig, LoggingConfig
};

// 重新导出统计类型
pub use l1_cache::L1CacheStats;
pub use l2_cache::L2CacheStats;
pub use ttl::TtlStats;

// 版本信息
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// 获取库信息
pub fn info() -> String {
    format!("{} v{} - {}", NAME, VERSION, DESCRIPTION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_library_info() {
        let info = info();
        assert!(info.contains("rat_memcache"));
        assert!(info.contains(VERSION));
    }

    #[tokio::test]
    async fn test_basic_usage() {
        let temp_dir = TempDir::new().unwrap();
        
        let cache = RatMemCacheBuilder::new()
            .development_preset().unwrap()
            .l2_config(L2Config {
                data_dir: temp_dir.path().to_path_buf(),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
            })
            .ttl_config(TtlConfig {
                default_ttl: Some(60),
                max_ttl: 3600,
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            })
            .build()
            .await
            .unwrap();
        
        // 基本操作测试
        let key = "test_key".to_string();
        let value = Bytes::from("test_value");
        
        cache.set(key.clone(), value.clone()).await.unwrap();
        
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
        
        cache.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_options() {
        let temp_dir = TempDir::new().unwrap();
        
        let cache = RatMemCacheBuilder::new()
            .development_preset().unwrap()
            .l2_config(L2Config {
                data_dir: temp_dir.path().to_path_buf(),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
            })
            .ttl_config(TtlConfig {
                default_ttl: Some(60),
                max_ttl: 3600,
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            })
            .build()
            .await
            .unwrap();
        
        let key = "options_key".to_string();
        let value = Bytes::from("options_value");
        
        // 测试缓存选项
        let options = CacheOptions {
            ttl_seconds: Some(300),
            force_l2: true,
            skip_l1: false,
            enable_compression: Some(true),
        };
        
        cache.set_with_options(key.clone(), value.clone(), &options).await.unwrap();
        
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
        
        cache.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_error_handling() {
        // 测试无效 TTL
        let temp_dir = TempDir::new().unwrap();
        
        let cache = RatMemCacheBuilder::new()
            .development_preset().unwrap()
            .l2_config(L2Config {
                data_dir: temp_dir.path().to_path_buf(),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
            })
            .ttl_config(TtlConfig {
                default_ttl: Some(60),
                max_ttl: 3600,
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            })
            .build()
            .await
            .unwrap();
        
        let key = "test_key".to_string();
        let value = Bytes::from("test_value");
        
        // 尝试设置超过最大 TTL 的值
        let result = cache.set_with_ttl(key, value, 10000).await;
        assert!(result.is_err());
        
        if let Err(CacheError::InvalidTtl { ttl_seconds: _ }) = result {
            // 预期的错误类型
        } else {
            panic!("Expected InvalidTtl error");
        }
        
        cache.shutdown().await.unwrap();
    }
}
