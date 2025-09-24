//! RatMemCache - 高性能双层缓存系统
//!
//! 基于 MelangeDB 持久化存储的双层缓存系统，
//! 支持多种驱逐策略、TTL 管理、数据压缩和高性能指标收集。
//!
//! # 特性
//!
//! - **双层缓存**: 内存 L1 缓存 + MelangeDB L2 持久化缓存
//! - **高性能传输**: 优化的 TCP 网络传输
//! - **多种策略**: 支持 LRU、LFU、FIFO、混合策略等
//! - **TTL 支持**: 灵活的过期时间管理
//! - **数据压缩**: LZ4 压缩算法，节省存储空间
//! - **高性能指标**: 读写分离指标系统
//! - **结构化日志**: 基于 rat_logger 的高性能日志系统
//! - **异步设计**: 全异步 API，支持高并发
//!
//! # 快速开始
//!
//! 创建缓存实例并使用基本功能。
//!
//! # 高级用法
//!
//! ## 自定义配置
//!
//! 可以通过构建器模式进行详细的配置。
//!
//! ## 缓存选项
//!
//! 可以使用 CacheOptions 来精细控制缓存行为。

// 核心模块
pub mod cache;
pub mod config;
pub mod error;
pub mod types;

// 公开模块
pub mod logging;
pub mod streaming_protocol;

// 内部模块
mod compression;
mod l1_cache;
#[cfg(feature = "melange-storage")]
mod l2_cache;
#[cfg(feature = "melange-storage")]
mod melange_adapter;
mod ttl;


// 重新导出主要类型
pub use cache::{RatMemCache, RatMemCacheBuilder, CacheOptions};

pub use error::{CacheError, CacheResult};
pub use types::{CacheValue, EvictionStrategy, CacheLayer, CacheOperation};

// 重新导出配置类型
pub use config::{
    CacheConfig, CacheConfigBuilder,
    L1Config, TtlConfig,
    PerformanceConfig, LoggingConfig
};
#[cfg(feature = "melange-storage")]
pub use config::{L2Config, CacheWarmupStrategy};

// 重新导出 MelangeDB 相关类型
#[cfg(feature = "melange-storage")]
pub use melange_adapter::{MelangeAdapter, MelangeConfig, CompressionAlgorithm, BatchOperation};

// 重新导出统计类型
pub use l1_cache::L1CacheStats;
#[cfg(feature = "melange-storage")]
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

#[cfg(all(test, feature = "melange-storage"))]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::time::{sleep, Duration};
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
            .l1_config(L1Config {
                max_memory: 1024 * 1024 * 1024, // 1GB
                max_entries: 100_000,
                eviction_strategy: EvictionStrategy::Lru,
            })
            .l2_config(L2Config {
                enable_l2_cache: true,
                data_dir: Some(temp_dir.path().to_path_buf()),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_lz4: true,
                compression_threshold: 128,
                compression_max_threshold: 1024 * 1024,
                compression_level: 6,
                background_threads: 2,
                clear_on_startup: false,
                cache_size_mb: 256,
                max_file_size_mb: 512,
                smart_flush_enabled: true,
                smart_flush_base_interval_ms: 100,
                smart_flush_min_interval_ms: 20,
                smart_flush_max_interval_ms: 500,
                smart_flush_write_rate_threshold: 10000,
                smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024,
                cache_warmup_strategy: crate::config::CacheWarmupStrategy::Recent,
                zstd_compression_level: None,
                l2_write_strategy: "write_through".to_string(),
                l2_write_threshold: 1024,
                l2_write_ttl_threshold: 300,
            })
            .ttl_config(TtlConfig {
                expire_seconds: Some(60),
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            })
                        .performance_config(PerformanceConfig {
                worker_threads: 4,
                enable_concurrency: true,
                read_write_separation: true,
                batch_size: 100,
                enable_warmup: false,
                large_value_threshold: 10240, // 10KB
            })
            .logging_config(LoggingConfig {
                level: "debug".to_string(),
                enable_colors: false,
                show_timestamp: false,
                enable_performance_logs: true,
                enable_audit_logs: false,
                enable_cache_logs: true,
                enable_logging: true,
                enable_async: false,
                batch_size: 2048,
                batch_interval_ms: 25,
                buffer_size: 16384,
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
            .l1_config(L1Config {
                max_memory: 1024 * 1024 * 1024, // 1GB
                max_entries: 100_000,
                eviction_strategy: EvictionStrategy::Lru,
            })
            .l2_config(L2Config {
                enable_l2_cache: true,
                data_dir: Some(temp_dir.path().to_path_buf()),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_lz4: true,
                compression_threshold: 128,
                compression_max_threshold: 1024 * 1024,
                compression_level: 6,
                background_threads: 2,
                clear_on_startup: false,
                cache_size_mb: 256,
                max_file_size_mb: 512,
                smart_flush_enabled: true,
                smart_flush_base_interval_ms: 100,
                smart_flush_min_interval_ms: 20,
                smart_flush_max_interval_ms: 500,
                smart_flush_write_rate_threshold: 10000,
                smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024,
                cache_warmup_strategy: crate::config::CacheWarmupStrategy::Recent,
                zstd_compression_level: None,
                l2_write_strategy: "write_through".to_string(),
                l2_write_threshold: 1024,
                l2_write_ttl_threshold: 300,
            })
            .ttl_config(TtlConfig {
                expire_seconds: Some(60),
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            })
                        .performance_config(PerformanceConfig {
                worker_threads: 4,
                enable_concurrency: true,
                read_write_separation: true,
                batch_size: 100,
                enable_warmup: false,
                large_value_threshold: 10240, // 10KB
            })
            .logging_config(LoggingConfig {
                level: "debug".to_string(),
                enable_colors: false,
                show_timestamp: false,
                enable_performance_logs: true,
                enable_audit_logs: false,
                enable_cache_logs: true,
                enable_logging: true,
                enable_async: false,
                batch_size: 2048,
                batch_interval_ms: 25,
                buffer_size: 16384,
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
        // 测试错误处理场景
        let temp_dir = TempDir::new().unwrap();

        let cache = RatMemCacheBuilder::new()
            .l1_config(L1Config {
                max_memory: 1024 * 1024 * 1024, // 1GB
                max_entries: 100_000,
                eviction_strategy: EvictionStrategy::Lru,
            })
            .l2_config(L2Config {
                enable_l2_cache: true,
                data_dir: Some(temp_dir.path().to_path_buf()),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_lz4: true,
                compression_threshold: 128,
                compression_max_threshold: 1024 * 1024,
                compression_level: 6,
                background_threads: 2,
                clear_on_startup: false,
                cache_size_mb: 256,
                max_file_size_mb: 512,
                smart_flush_enabled: true,
                smart_flush_base_interval_ms: 100,
                smart_flush_min_interval_ms: 20,
                smart_flush_max_interval_ms: 500,
                smart_flush_write_rate_threshold: 10000,
                smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024,
                cache_warmup_strategy: crate::config::CacheWarmupStrategy::Recent,
                zstd_compression_level: None,
                l2_write_strategy: "write_through".to_string(),
                l2_write_threshold: 1024,
                l2_write_ttl_threshold: 300,
            })
            .ttl_config(TtlConfig {
                expire_seconds: Some(60),
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false,
            })
                        .performance_config(PerformanceConfig {
                worker_threads: 4,
                enable_concurrency: true,
                read_write_separation: true,
                batch_size: 100,
                enable_warmup: false,
                large_value_threshold: 10240, // 10KB
            })
            .logging_config(LoggingConfig {
                level: "debug".to_string(),
                enable_colors: false,
                show_timestamp: false,
                enable_performance_logs: true,
                enable_audit_logs: false,
                enable_cache_logs: true,
                enable_logging: true,
                enable_async: false,
                batch_size: 2048,
                batch_interval_ms: 25,
                buffer_size: 16384,
            })
            .build()
            .await
            .unwrap();

        let key = "test_key".to_string();
        let value = Bytes::from("test_value");

        // 测试1: 设置一个很大的 TTL 值（现在应该成功，因为没有TTL限制）
        let result = cache.set_with_ttl(key.clone(), value.clone(), 10000).await;
        assert!(result.is_ok(), "设置大TTL值应该成功");

        // 验证值确实被设置了
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some(), "设置的值应该能够获取到");
        assert_eq!(retrieved.unwrap(), value, "获取的值应该与设置的值相同");

        // 测试2: 设置 TTL 为 0（永不过期）
        let result = cache.set_with_ttl(key.clone(), value.clone(), 0).await;
        assert!(result.is_ok(), "设置TTL为0应该成功");

        // 测试3: 验证 TTL 功能正常工作
        let short_ttl_key = "short_ttl";
        cache.set_with_ttl(short_ttl_key.to_string(), value.clone(), 1).await.unwrap();

        // 立即获取应该成功
        let retrieved = cache.get(short_ttl_key).await.unwrap();
        assert!(retrieved.is_some(), "短TTL键应该能够立即获取");

        // 等待过期
        sleep(Duration::from_millis(1500)).await;

        // 现在应该过期了
        let retrieved = cache.get(short_ttl_key).await.unwrap();
        assert!(retrieved.is_none(), "短TTL键应该已经过期");

        cache.shutdown().await.unwrap();
    }
}

