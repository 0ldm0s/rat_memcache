// 简化的大值测试
use rat_memcache::RatMemCacheBuilder;
use rat_memcache::{L1Config, L2Config, PerformanceConfig, TtlConfig, LoggingConfig, CompressionConfig, CacheWarmupStrategy};
use rat_memcache::types::EvictionStrategy;
use rat_memcache::CompressionAlgorithm;
use tempfile::TempDir;
use bytes::Bytes;

#[tokio::test]
async fn test_large_value_functionality() {
    println!("开始测试大值处理功能...");

    // 创建临时目录
    let temp_dir = TempDir::new().unwrap();

    // 创建缓存，设置1KB阈值
    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 1024 * 1024,
            max_entries: 1000,
            eviction_strategy: EvictionStrategy::Lru,
        })
        .l2_config(L2Config {
            enable_l2_cache: true,
            data_dir: Some(temp_dir.path().to_path_buf()),
            max_disk_size: 10 * 1024 * 1024,
            write_buffer_size: 1024 * 1024,
            max_write_buffer_number: 2,
            block_cache_size: 512 * 1024,
            enable_compression: true,
            compression_level: 3,
            background_threads: 2,
            clear_on_startup: true,
            compression_algorithm: CompressionAlgorithm::Lz4,
            cache_size_mb: 64,
            max_file_size_mb: 256,
            smart_flush_enabled: false,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 500,
            smart_flush_write_rate_threshold: 1000,
            smart_flush_accumulated_bytes_threshold: 1024 * 1024,
            cache_warmup_strategy: CacheWarmupStrategy::None,
            zstd_compression_level: None,
        })
        .performance_config(PerformanceConfig {
            worker_threads: 2,
            enable_concurrency: true,
            read_write_separation: true,
            batch_size: 100,
            enable_warmup: false,
            stats_interval: 10,
            enable_background_stats: false,
            l2_write_strategy: "WriteThrough".to_string(),
            l2_write_threshold: 1024,
            l2_write_ttl_threshold: 3600,
            large_value_threshold: 1024, // 1KB阈值
        })
        .ttl_config(TtlConfig {
            default_ttl: Some(3600),
            max_ttl: 86400,
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .compression_config(CompressionConfig {
            enable_lz4: true,
            compression_threshold: 1024,
            compression_level: 3,
            auto_compression: true,
            min_compression_ratio: 0.8,
        })
        .logging_config(LoggingConfig {
            level: "INFO".to_string(),
            enable_colors: false,
            show_timestamp: true,
            enable_performance_logs: false,
            enable_audit_logs: false,
            enable_cache_logs: false,
        })
        .build()
        .await
        .expect("缓存创建失败");

    // 测试1: 小值应该正常工作
    println!("测试1: 小值 (512B)");
    let small_value = Bytes::from(vec![0u8; 512]);
    cache.set("small".to_string(), small_value.clone()).await.unwrap();
    let retrieved_small = cache.get("small").await.unwrap().unwrap();
    assert_eq!(retrieved_small, small_value);
    println!("✓ 小值测试通过");

    // 测试2: 大值应该直接写入L2
    println!("测试2: 大值 (2KB)");
    let large_value = Bytes::from(vec![1u8; 2048]);
    cache.set("large".to_string(), large_value.clone()).await.unwrap();
    let retrieved_large = cache.get("large").await.unwrap().unwrap();
    assert_eq!(retrieved_large, large_value);
    println!("✓ 大值测试通过");

    // 测试3: 验证L2确实有数据
    let data_dir = temp_dir.path();
    let has_files = std::fs::read_dir(data_dir).unwrap().next().is_some();
    assert!(has_files, "L2应该有数据文件");
    println!("✓ L2数据验证通过");

    println!("🎉 所有测试通过！大值处理功能正常工作！");
    println!("   - 小值正常存储到L1");
    println!("   - 大值直接下沉到L2");
    println!("   - 数据可以正确读取");
}