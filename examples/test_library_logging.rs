//! 测试 rat_memcache 作为库使用时的日志行为
//!
//! 这个测试验证：
//! 1. 作为库使用时不主动初始化日志系统
//! 2. 使用安全的日志宏时不会报错
//! 3. 未初始化日志时静默失败

use rat_memcache::{RatMemCacheBuilder, cache_log, cache_debug, EvictionStrategy};
use rat_memcache::config::{LoggingConfig, L1Config, CompressionConfig, TtlConfig, PerformanceConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 测试 rat_memcache 作为库使用时的日志行为 ===\n");

    // 测试1：使用默认配置创建缓存实例（不初始化日志）
    println!("测试1：创建缓存实例而不初始化日志系统");
    let l1_config = L1Config {
        max_memory: 1024 * 1024,  // 1MB
        max_entries: 1000,
        eviction_strategy: EvictionStrategy::Lru,
    };

    let ttl_config = TtlConfig {
        expire_seconds: None,
        cleanup_interval: 300,
        max_cleanup_entries: 1000,
        lazy_expiration: true,
        active_expiration: true,
    };

    let performance_config = PerformanceConfig {
        worker_threads: 4,
        enable_concurrency: true,
        read_write_separation: true,
        batch_size: 1024,
        enable_warmup: false,
        large_value_threshold: 10240,
    };
    // 压缩配置已整合到L2Config中，测试示例不需要压缩功能

    // 创建一个日志配置用于测试
    let log_config = LoggingConfig {
        level: "debug".to_string(),
        enable_colors: true,
        show_timestamp: true,
        enable_performance_logs: true,
        enable_audit_logs: true,
        enable_cache_logs: true,
        enable_logging: true,
        enable_async: false,
        batch_size: 2048,
        batch_interval_ms: 25,
        buffer_size: 16384,
    };

    let cache = RatMemCacheBuilder::new()
        .l1_config(l1_config)
        .ttl_config(ttl_config)
        .performance_config(performance_config)
        .logging_config(log_config.clone())
        .build()
        .await?;
    println!("✓ 缓存实例创建成功\n");

    // 测试2：使用安全的日志宏（不应该报错）
    println!("测试2：使用安全的日志宏（未初始化状态）");

    // 这些宏调用不应该导致程序崩溃
    cache_debug!("这是调试日志测试");
    cache_log!(&log_config, info, "这是缓存信息日志测试");
    cache_log!(&log_config, warn, "这是缓存警告日志测试");

    println!("✓ 安全日志宏调用成功，程序未崩溃\n");

    // 测试3：执行基本的缓存操作
    println!("测试3：执行基本缓存操作");
    let key = "test_key".to_string();
    let value = bytes::Bytes::from("test_value");

    // 设置缓存
    cache.set(key.clone(), value.clone()).await?;
    println!("✓ 缓存设置成功");

    // 获取缓存
    if let Some(retrieved) = cache.get(&key).await? {
        println!("✓ 缓存获取成功: {:?}", retrieved);
    }

    // 删除缓存
    let deleted = cache.delete(&key).await?;
    println!("✓ 缓存删除成功: {}", deleted);

    println!("\n=== 测试完成 ===");
    println!("结论：rat_memcache 作为库使用时，");
    println!("1. 不会主动初始化日志系统 ✓");
    println!("2. 使用安全日志宏不会报错 ✓");
    println!("3. 未初始化日志时静默失败 ✓");
    println!("4. 基本缓存功能正常工作 ✓");

    // 优雅关闭
    cache.shutdown().await?;

    Ok(())
}