//! 测试L1缓存不进行压缩和解压缩的示例
//!
//! 这个示例验证L1缓存是否正确地避免了压缩/解压缩操作，
//! 确保内存缓存的性能优势。

use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use rat_memcache::config::{L1Config, TtlConfig, PerformanceConfig, LoggingConfig};
use rat_memcache::types::EvictionStrategy;
use bytes::Bytes;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 测试L1缓存压缩/解压缩行为");
    println!("📋 测试目标: 验证L1缓存不进行压缩和解压缩操作");

    // 创建仅L1缓存的配置（不启用L2）
    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 64 * 1024 * 1024, // 64MB
            max_entries: 1000,
            eviction_strategy: EvictionStrategy::Lru,
        })
        .ttl_config(TtlConfig {
            expire_seconds: Some(60),
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .performance_config(PerformanceConfig {
            worker_threads: 2,
            enable_concurrency: true,
            read_write_separation: false,
            batch_size: 100,
            enable_warmup: false,
            large_value_threshold: 10240, // 10KB（默认值）
        })
        .logging_config(LoggingConfig {
            level: "debug".to_string(),  // 启用debug日志观察行为
            enable_colors: true,
            show_timestamp: true,
            enable_performance_logs: true,
            enable_audit_logs: false,
            enable_cache_logs: true,
            enable_logging: true,
            enable_async: false,
            batch_size: 1000,
            batch_interval_ms: 100,
            buffer_size: 8192,
        })
        .build()
        .await?;

    println!("✅ 缓存创建成功（仅L1，无L2）");

    // 测试数据1: 小数据（不应该压缩）
    let small_key = "small_data";
    let small_value = Bytes::from("这是一些小数据，不应该被压缩");

    println!("\n📝 测试1: 小数据存储和获取");
    println!("   键: {}", small_key);
    println!("   值: {:?}", std::str::from_utf8(&small_value)?);
    println!("   大小: {} 字节", small_value.len());

    // 测试set操作
    let set_start = Instant::now();
    cache.set(small_key.to_string(), small_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 测试get操作
    let get_start = Instant::now();
    let retrieved = cache.get(small_key).await?;
    let get_duration = get_start.elapsed();

    match retrieved {
        Some(value) => {
            println!("   ✅ GET成功: {:?}", std::str::from_utf8(&value)?);
            println!("   ⏱️  GET操作耗时: {:?}", get_duration);
            println!("   🔍 数据一致性: {}", value == small_value);
        }
        None => {
            println!("   ❌ GET失败: 数据未找到");
        }
    }

    // 测试数据2: 重复数据（传统上会压缩，但L1不应该）
    let repeat_key = "repeat_data";
    let repeat_value = Bytes::from("A".repeat(5000)); // 5000字节的重复数据（小于10KB阈值）

    println!("\n📝 测试2: 重复数据存储和获取");
    println!("   键: {}", repeat_key);
    println!("   值: {}个重复的'A'字符", repeat_value.len());
    println!("   大小: {} 字节", repeat_value.len());

    // 测试set操作
    let set_start = Instant::now();
    cache.set(repeat_key.to_string(), repeat_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 多次get操作测试性能
    println!("   🔄 执行多次GET操作测试性能...");
    let mut total_get_time = std::time::Duration::new(0, 0);
    let iterations = 100;

    for i in 0..iterations {
        let get_start = Instant::now();
        let retrieved = cache.get(repeat_key).await?;
        let get_duration = get_start.elapsed();
        total_get_time += get_duration;

        if i == 0 {
            match retrieved {
                Some(value) => {
                    println!("     ✅ 首次GET成功，大小: {} 字节", value.len());
                }
                None => {
                    println!("     ❌ 首次GET失败: 数据未找到");
                    break;
                }
            }
        }

        // 短暂间隔避免过于频繁的访问
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    let avg_get_time = total_get_time / iterations;
    println!("     📊 平均GET耗时: {:?} ({}次迭代)", avg_get_time, iterations);

    // 性能判断标准：如果L1缓存不进行解压缩，平均GET时间应该非常短（< 50微秒）
    if avg_get_time.as_micros() < 50 {
        println!("     ✅ 性能测试通过：L1缓存响应时间符合预期（无解压缩开销）");
    } else {
        println!("     ⚠️  性能测试警告：L1缓存响应时间偏慢，可能仍有解压缩开销");
    }

    // 测试数据2b: 大值测试（超过阈值应该被抛弃）
    let large_key = "large_data";
    let large_value = Bytes::from("B".repeat(15000)); // 15KB（超过10KB阈值）

    println!("\n📝 测试2b: 大值处理测试");
    println!("   键: {}", large_key);
    println!("   值: {}个重复的'B'字符", large_value.len());
    println!("   大小: {} 字节 (超过10KB阈值)", large_value.len());

    // 测试set操作（大值应该被抛弃）
    let set_start = Instant::now();
    cache.set(large_key.to_string(), large_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 测试get操作（应该返回None，因为大值被抛弃了）
    let get_start = Instant::now();
    let retrieved = cache.get(large_key).await?;
    let get_duration = get_start.elapsed();

    match retrieved {
        Some(value) => {
            println!("   ❌ 大值测试失败：数据不应该被存储（大小: {} 字节）", value.len());
        }
        None => {
            println!("   ✅ 大值测试通过：超过阈值的数据被正确抛弃");
            println!("   ⏱️  GET操作耗时: {:?} (返回None)", get_duration);
        }
    }

    // 测试数据3: JSON数据（结构化数据）
    let json_key = "json_data";
    let json_value = Bytes::from(r#"{"name":"测试","type":"JSON数据","items":[1,2,3,4,5],"description":"这是一段用于测试的JSON格式数据，包含各种类型的信息"}"#);

    println!("\n📝 测试3: JSON数据存储和获取");
    println!("   键: {}", json_key);
    println!("   值: JSON格式数据");
    println!("   大小: {} 字节", json_value.len());

    // 测试set操作
    let set_start = Instant::now();
    cache.set(json_key.to_string(), json_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 测试get操作
    let get_start = Instant::now();
    let retrieved = cache.get(json_key).await?;
    let get_duration = get_start.elapsed();

    match retrieved {
        Some(value) => {
            println!("   ✅ GET成功，大小: {} 字节", value.len());
            println!("   ⏱️  GET操作耗时: {:?}", get_duration);
            println!("   🔍 数据一致性: {}", value == json_value);
        }
        None => {
            println!("   ❌ GET失败: 数据未找到");
        }
    }

    // 验证缓存统计
    println!("\n📊 缓存统计信息:");
    let l1_stats = cache.get_l1_stats().await;
    println!("   📈 L1缓存条目数: {}", l1_stats.entry_count);
    println!("   📈 L1缓存内存使用: {} 字节", l1_stats.memory_usage);
    println!("   📈 L1缓存内存利用率: {:.2}%", l1_stats.memory_utilization * 100.0);
    println!("   📈 L1缓存条目利用率: {:.2}%", l1_stats.entry_utilization * 100.0);

    // 检查是否有L2缓存（应该没有）
    #[cfg(feature = "melange-storage")]
    {
        let l2_stats = cache.get_l2_stats().await;
        println!("   📈 L2缓存写入次数: {}", l2_stats.writes);
        if l2_stats.writes == 0 {
            println!("   ✅ 确认：没有L2缓存写入操作");
        } else {
            println!("   ⚠️  警告：检测到L2缓存写入操作（{}次）", l2_stats.writes);
        }
    }

    println!("\n✅ L1缓存压缩/解压缩测试完成");
    println!("📋 结论:");
    println!("   - L1缓存应该直接存储和返回原始数据");
    println!("   - 不应该进行压缩操作（节省CPU）");
    println!("   - 不应该进行解压缩操作（提升性能）");
    println!("   - 内存缓存命中应该非常快速（亚毫秒级）");

    Ok(())
}