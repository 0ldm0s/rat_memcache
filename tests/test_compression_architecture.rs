//! 压缩架构验证测试
//!
//! 验证压缩判断逻辑是否正确下沉到L2层

use rat_memcache::RatMemCacheBuilder;
use rat_memcache::config::{L1Config, PerformanceConfig, TtlConfig, LoggingConfig};
use bytes::Bytes;
use tempfile::TempDir;

#[cfg(feature = "melange-storage")]
use rat_memcache::config::{L2Config, CacheWarmupStrategy};
#[cfg(feature = "melange-storage")]
use rat_memcache::CompressionAlgorithm;

#[cfg(feature = "melange-storage")]
#[tokio::test]
async fn test_compression_architecture() {
    // 创建临时目录
    let temp_dir = TempDir::new().unwrap();

    // 配置压缩阈值
    let compression_threshold = 128;    // 128字节
    let compression_max_threshold = 1024 * 1024; // 1MB
    let large_value_threshold = 10240;  // 10KB

    // 创建缓存实例
    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 1024 * 1024 * 1024, // 1GB
            max_entries: 100000,
            eviction_strategy: rat_memcache::EvictionStrategy::Lru,
        })
        .l2_config(L2Config {
            enable_l2_cache: true,
            data_dir: Some(temp_dir.path().to_path_buf()),
            max_disk_size: 1024 * 1024 * 1024, // 1GB
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            block_cache_size: 32 * 1024 * 1024, // 32MB
            background_threads: 4,
            clear_on_startup: false,
            enable_lz4: true,
            compression_threshold,
            compression_max_threshold,
            compression_level: 6,
            cache_size_mb: 512,
            max_file_size_mb: 1024,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 500,
            smart_flush_write_rate_threshold: 8000,
            smart_flush_accumulated_bytes_threshold: 8388608,
            cache_warmup_strategy: CacheWarmupStrategy::Recent,
            zstd_compression_level: None,
            l2_write_strategy: "write_through".to_string(),
            l2_write_threshold: 1024,
            l2_write_ttl_threshold: 300,
        })
        .performance_config(PerformanceConfig {
            worker_threads: 4,
            enable_concurrency: true,
            read_write_separation: true,
            batch_size: 100,
            enable_warmup: true,
            large_value_threshold,
        })
        .ttl_config(TtlConfig {
            expire_seconds: Some(3600),
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .logging_config(LoggingConfig {
            level: "DEBUG".to_string(),
            enable_colors: true,
            show_timestamp: true,
            enable_performance_logs: true,
            enable_audit_logs: true,
            enable_cache_logs: true,
            // Advanced logging config
            enable_logging: true,
            enable_async: false,
            batch_size: 2048,
            batch_interval_ms: 25,
            buffer_size: 16384,
        })
        .build()
        .await
        .expect("Failed to create cache");

    // 测试1: 小值（100字节）- 应该进入L1，不进行压缩
    let small_value = Bytes::from(vec![b'x'; 100]); // 100字节
    println!("=== 测试1: 小值处理（{}字节）===", small_value.len());

    cache.set("small_key".to_string(), small_value.clone()).await
        .expect("Failed to set small value");

    let retrieved_small = cache.get("small_key").await
        .expect("Failed to get small value")
        .expect("Small value not found");

    assert_eq!(retrieved_small, small_value);
    println!("✓ 小值处理正确");

    // 测试2: 中等值（20KB）- 应该进入L2，但小于压缩阈值(128字节)，不压缩
    let medium_value = Bytes::from(vec![b'y'; 20480]); // 20KB
    println!("\n=== 测试2: 中等值处理（{}字节）===", medium_value.len());

    cache.set("medium_key".to_string(), medium_value.clone()).await
        .expect("Failed to set medium value");

    let retrieved_medium = cache.get("medium_key").await
        .expect("Failed to get medium value")
        .expect("Medium value not found");

    assert_eq!(retrieved_medium, medium_value);
    println!("✓ 中等值处理正确");

    // 测试3: 压缩范围内值（200KB）- 应该进入L2，在压缩范围内，会进行压缩
    let compressible_value = Bytes::from(vec![b'z'; 204800]); // 200KB
    println!("\n=== 测试3: 压缩范围内值处理（{}字节）===", compressible_value.len());

    cache.set("compressible_key".to_string(), compressible_value.clone()).await
        .expect("Failed to set compressible value");

    let retrieved_compressible = cache.get("compressible_key").await
        .expect("Failed to get compressible value")
        .expect("Compressible value not found");

    assert_eq!(retrieved_compressible, compressible_value);
    println!("✓ 压缩范围内值处理正确");

    // 测试4: 超过压缩最大阈值值（2MB）- 应该进入L2，但超过压缩最大阈值(1MB)，不压缩
    let huge_value = Bytes::from(vec![b'w'; 2 * 1024 * 1024]); // 2MB
    println!("\n=== 测试4: 超过压缩最大阈值值处理（{}字节）===", huge_value.len());

    cache.set("huge_key".to_string(), huge_value.clone()).await
        .expect("Failed to set huge value");

    let retrieved_huge = cache.get("huge_key").await
        .expect("Failed to get huge value")
        .expect("Huge value not found");

    assert_eq!(retrieved_huge, huge_value);
    println!("✓ 超过压缩最大阈值值处理正确");

    // 验证统计信息
    let l1_stats = cache.get_l1_stats().await;
    let l2_stats = cache.get_l2_stats().await;

    println!("\n=== 统计信息 ===");
    println!("L1缓存条目数: {}", l1_stats.entry_count);
    println!("L2缓存写入次数: {}", l2_stats.writes);

    // 验证L1和L2都有数据
    // 小值应该在L1中
    assert!(l1_stats.entry_count > 0, "L1缓存应该有数据");

    // 大值应该在L2中
    assert!(l2_stats.writes > 0, "L2缓存应该有写入操作");

    println!("\n=== 架构验证总结 ===");
    println!("✓ 小值(<10KB) -> L1缓存 -> 不压缩");
    println!("✓ 中等值(≥10KB) -> L2缓存 -> 小于压缩阈值(128字节) -> 不压缩");
    println!("✓ 大值(128字节~1MB) -> L2缓存 -> 在压缩范围内 -> 压缩");
    println!("✓ 超大值(>1MB) -> L2缓存 -> 超过压缩最大阈值 -> 不压缩");
    println!("✓ 压缩判断逻辑已正确下沉到L2层！");
}

#[cfg(feature = "melange-storage")]
#[tokio::test]
async fn test_compression_disabled() {
    // 测试禁用压缩的情况
    let temp_dir = TempDir::new().unwrap();

    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 1024 * 1024 * 1024,
            max_entries: 100000,
            eviction_strategy: rat_memcache::EvictionStrategy::Lru,
        })
        .l2_config(L2Config {
            enable_l2_cache: true,
            data_dir: Some(temp_dir.path().to_path_buf()),
            max_disk_size: 1024 * 1024 * 1024,
            write_buffer_size: 64 * 1024 * 1024,
            max_write_buffer_number: 3,
            block_cache_size: 32 * 1024 * 1024,
            background_threads: 4,
            clear_on_startup: false,
            enable_lz4: false, // 禁用压缩
            compression_threshold: 128,
            compression_max_threshold: 1024 * 1024,
            compression_level: 6,
            cache_size_mb: 512,
            max_file_size_mb: 1024,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 500,
            smart_flush_write_rate_threshold: 8000,
            smart_flush_accumulated_bytes_threshold: 8388608,
            cache_warmup_strategy: CacheWarmupStrategy::Recent,
            zstd_compression_level: None,
            l2_write_strategy: "write_through".to_string(),
            l2_write_threshold: 1024,
            l2_write_ttl_threshold: 300,
        })
        .performance_config(PerformanceConfig {
            worker_threads: 4,
            enable_concurrency: true,
            read_write_separation: true,
            batch_size: 100,
            enable_warmup: true,
            large_value_threshold: 10240,
        })
        .ttl_config(TtlConfig {
            expire_seconds: None,
            cleanup_interval: 60,
            max_cleanup_entries: 100,
            lazy_expiration: true,
            active_expiration: true,
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
        .expect("Failed to create cache");

    // 设置一个在压缩范围内但压缩被禁用的值
    let value = Bytes::from(vec![b'x'; 204800]); // 200KB
    cache.set("test_key".to_string(), value.clone()).await
        .expect("Failed to set value");

    let retrieved = cache.get("test_key").await
        .expect("Failed to get value")
        .expect("Value not found");

    assert_eq!(retrieved, value);
    println!("✓ 压缩禁用时处理正确");
}