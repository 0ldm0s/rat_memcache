//! 测试 cache_log 宏是否正常工作

use rat_memcache::{cache_log, cache_debug};
use rat_memcache::config::LoggingConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 测试 cache_log 宏 ===\n");

    // 创建一个debug级别的日志配置
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

    println!("📝 测试1：直接调用 cache_log 宏");
    cache_log!(&log_config, debug, "这是一条debug日志测试");
    cache_log!(&log_config, info, "这是一条info日志测试");
    cache_log!(&log_config, warn, "这是一条warn日志测试");
    cache_log!(&log_config, error, "这是一条error日志测试");
    println!("✓ cache_log 宏调用完成");

    println!("\n📝 测试2：调用 cache_debug 宏");
    cache_debug!("这是cache_debug宏测试");
    println!("✓ cache_debug 宏调用完成");

    println!("\n=== 如果上面看不到日志输出，说明 rat_logger 需要初始化 ===");

    Ok(())
}