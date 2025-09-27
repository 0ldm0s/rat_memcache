//! 简单的缓存测试，验证日志输出问题
//!
//! 这个测试验证：
//! 1. SET和GET操作是否有debug日志输出
//! 2. 日志输出是否完整

use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{LoggingConfig, L1Config, TtlConfig, PerformanceConfig};
use rat_memcache::logging::init_logger;
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 简单缓存测试 ===\n");

    // 创建日志配置
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

    // 初始化日志系统
    println!("📝 初始化日志系统...");
    init_logger(log_config.clone())?;
    println!("✅ 日志系统初始化完成\n");

    // 创建缓存配置
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

    println!("📝 创建缓存实例...");
    let cache = RatMemCacheBuilder::new()
        .l1_config(l1_config)
        .ttl_config(ttl_config)
        .performance_config(performance_config)
        .logging_config(log_config)
        .build()
        .await?;
    println!("✅ 缓存实例创建成功\n");

    // 测试多个SET和GET操作
    println!("🧪 测试1：SET操作");
    let key1 = "test_key_1";
    let value1 = Bytes::from("test_value_1");
    println!("   SET {}: {} bytes", key1, value1.len());
    cache.set(key1.to_string(), value1).await?;
    println!("   ✅ SET完成\n");

    println!("🧪 测试2：GET操作");
    println!("   GET {}", key1);
    let result = cache.get(key1).await?;
    println!("   ✅ GET完成: found={}\n", result.is_some());

    println!("🧪 测试3：第二个SET操作");
    let key2 = "test_key_2";
    let value2 = Bytes::from("test_value_2");
    println!("   SET {}: {} bytes", key2, value2.len());
    cache.set(key2.to_string(), value2).await?;
    println!("   ✅ SET完成\n");

    println!("🧪 测试4：第二个GET操作");
    println!("   GET {}", key2);
    let result = cache.get(key2).await?;
    println!("   ✅ GET完成: found={}\n", result.is_some());

    println!("🧪 测试5：重复GET操作");
    println!("   GET {} (应该从缓存获取)", key1);
    let result = cache.get(key1).await?;
    println!("   ✅ GET完成: found={}\n", result.is_some());

    println!("🧪 测试6：重复GET操作");
    println!("   GET {} (应该从缓存获取)", key2);
    let result = cache.get(key2).await?;
    println!("   ✅ GET完成: found={}\n", result.is_some());

    println!("=== 检查输出 ===");
    println!("请检查上面的输出中是否包含以下日志：");
    println!("1. 🎯 [RatMemCache] SET 操作");
    println!("2. 🎯 [RatMemCache] GET 操作");
    println!("3. 🎯 [RatMemCache] GET 结果");

    cache.shutdown().await?;
    println!("🔚 测试完成");

    Ok(())
}