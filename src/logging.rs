//! 日志模块
//!
//! 基于 zerg_creep 库实现高性能日志系统

use crate::config::LoggingConfig;
use crate::error::CacheResult;
use std::sync::Once;
use std::io::Write;
use chrono::Local;
use zerg_creep::logger::builder::LoggerBuilder;
use zerg_creep::logger::{Level, LevelFilter, Record};

// 重新导出 zerg_creep 的日志宏
pub use zerg_creep::{debug, error, info, trace, warn};

/// 日志管理器
#[derive(Debug)]
pub struct LogManager {
    config: LoggingConfig,
}

/// 日志级别转换
fn convert_log_level(level: &str) -> LevelFilter {
    match level.to_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" | "warning" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off" => LevelFilter::Off,
        _ => LevelFilter::Info, // 默认级别
    }
}

/// 全局日志初始化标志
static INIT: Once = Once::new();

/// RAT 缓存主题格式化函数
fn rat_cache_format(
    buf: &mut dyn Write,
    record: &Record,
) -> std::io::Result<()> {
    let level = record.metadata.level;

    // RAT 缓存主题配色方案
    let (level_color, level_bg, level_icon, prefix) = match level {
        Level::Error => ("\x1b[97m", "\x1b[41m", "💥", "[RAT-CACHE-ERROR]"), // 白字红底 - 错误
        Level::Warn => ("\x1b[30m", "\x1b[43m", "⚠️", "[RAT-CACHE-WARN]"),   // 黑字黄底 - 警告
        Level::Info => ("\x1b[97m", "\x1b[42m", "📦", "[RAT-CACHE-INFO]"),   // 白字绿底 - 信息
        Level::Debug => ("\x1b[30m", "\x1b[46m", "🔧", "[RAT-CACHE-DEBUG]"), // 黑字青底 - 调试
        Level::Trace => ("\x1b[97m", "\x1b[45m", "🔍", "[RAT-CACHE-TRACE]"), // 白字紫底 - 追踪
    };

    // 配色方案
    let timestamp_color = "\x1b[90m"; // 灰色 - 时间戳
    let message_color = "\x1b[97m";   // 亮白色 - 消息
    let prefix_color = "\x1b[93m";    // 亮黄色 - 前缀
    let reset = "\x1b[0m";

    // 获取当前时间
    let now = Local::now();
    let timestamp = now.format("%H:%M:%S%.3f");

    writeln!(
        buf,
        "{}{} {}{}{:5}{} {} {}{}{} {}{}{}",
        timestamp_color,
        timestamp, // 时间戳
        level_color,
        level_bg,
        level,
        reset,      // 状态指示器
        level_icon, // 图标
        prefix_color,
        prefix,
        reset, // 前缀
        message_color,
        record.args,
        reset // 消息内容
    )
}

/// 纯文本格式化函数（无颜色）
fn rat_cache_plain_format(
    buf: &mut dyn Write,
    record: &Record,
) -> std::io::Result<()> {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d %H:%M:%S%.3f");

    writeln!(
        buf,
        "[{}] [{}] [RAT-CACHE] {}",
        timestamp, record.metadata.level, record.args
    )
}

impl LogManager {
    /// 创建新的日志管理器
    pub fn new(config: LoggingConfig) -> Self {
        Self { config }
    }

    /// 初始化日志系统
    pub fn initialize(&self) -> CacheResult<()> {
        let config = &self.config;
        
        INIT.call_once(|| {
            let mut builder = LoggerBuilder::new();
            builder.filter(convert_log_level(&config.level));
            
            // 根据配置选择格式化器
            if config.enable_colors {
                builder.format(rat_cache_format);
            } else {
                builder.format(rat_cache_plain_format);
            }

            // 尝试初始化日志器
            match builder.try_init() {
                Ok(_) => {},
                Err(e) => {
                    // 如果已经初始化过了，这是正常的
                    use std::io::{self, Write};
                    let _ = writeln!(io::stderr(), "Logger init warning: {}", e);
                }
            }
        });

        Ok(())
    }

    /// 获取配置
    pub fn config(&self) -> &LoggingConfig {
        &self.config
    }
}

/// 便捷的初始化函数
pub fn init_logger(config: LoggingConfig) -> CacheResult<()> {
    let manager = LogManager::new(config);
    manager.initialize()
}

/// 使用默认配置初始化
pub fn init_default_logger() -> CacheResult<()> {
    let config = LoggingConfig {
        level: "info".to_string(),
        enable_colors: true,
        show_timestamp: true,
        enable_performance_logs: true,
        enable_audit_logs: true,
        enable_cache_logs: true,
    };
    init_logger(config)
}

/// 性能日志宏
#[macro_export]
macro_rules! perf_log {
    ($config:expr, $level:ident, $($arg:tt)*) => {
        if $config.enable_performance_logs {
            zerg_creep::$level!("[PERF] {}", format!($($arg)*));
        }
    };
}

/// 审计日志宏
#[macro_export]
macro_rules! audit_log {
    ($config:expr, $level:ident, $($arg:tt)*) => {
        if $config.enable_audit_logs {
            zerg_creep::$level!("[AUDIT] {}", format!($($arg)*));
        }
    };
}

/// 缓存操作日志宏
#[macro_export]
macro_rules! cache_log {
    ($config:expr, trace, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::trace!("[CACHE] {}", format!($($arg)*));
        }
    };
    ($config:expr, debug, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::debug!("[CACHE] {}", format!($($arg)*));
        }
    };
    ($config:expr, info, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::info!("[CACHE] {}", format!($($arg)*));
        }
    };
    ($config:expr, warn, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::warn!("[CACHE] {}", format!($($arg)*));
        }
    };
    ($config:expr, error, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::error!("[CACHE] {}", format!($($arg)*));
        }
    };
}

/// 压缩操作日志宏
#[macro_export]
macro_rules! compression_log {
    (trace, $($arg:tt)*) => {
        zerg_creep::trace!("[COMPRESSION] {}", format!($($arg)*));
    };
    (debug, $($arg:tt)*) => {
        zerg_creep::debug!("[COMPRESSION] {}", format!($($arg)*));
    };
    (info, $($arg:tt)*) => {
        zerg_creep::info!("[COMPRESSION] {}", format!($($arg)*));
    };
    (warn, $($arg:tt)*) => {
        zerg_creep::warn!("[COMPRESSION] {}", format!($($arg)*));
    };
    (error, $($arg:tt)*) => {
        zerg_creep::error!("[COMPRESSION] {}", format!($($arg)*));
    };
}

/// TTL 操作日志宏
#[macro_export]
macro_rules! ttl_log {
    ($config:expr, trace, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::trace!("[TTL] {}", format!($($arg)*));
        }
    };
    ($config:expr, debug, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::debug!("[TTL] {}", format!($($arg)*));
        }
    };
    ($config:expr, info, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::info!("[TTL] {}", format!($($arg)*));
        }
    };
    ($config:expr, warn, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::warn!("[TTL] {}", format!($($arg)*));
        }
    };
    ($config:expr, error, $($arg:tt)*) => {
        if $config.enable_cache_logs {
            zerg_creep::error!("[TTL] {}", format!($($arg)*));
        }
    };
}

/// 智能传输日志宏
#[macro_export]
macro_rules! transfer_log {
    (trace, $($arg:tt)*) => {
        zerg_creep::trace!("[TRANSFER] {}", format!($($arg)*));
    };
    (debug, $($arg:tt)*) => {
        zerg_creep::debug!("[TRANSFER] {}", format!($($arg)*));
    };
    (info, $($arg:tt)*) => {
        zerg_creep::info!("[TRANSFER] {}", format!($($arg)*));
    };
    (warn, $($arg:tt)*) => {
        zerg_creep::warn!("[TRANSFER] {}", format!($($arg)*));
    };
    (error, $($arg:tt)*) => {
        zerg_creep::error!("[TRANSFER] {}", format!($($arg)*));
    };
}

/// 性能监控日志结构
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub operation: String,
    pub duration_ms: f64,
    pub success: bool,
    pub details: Option<String>,
}

impl PerformanceMetrics {
    /// 创建新的性能指标
    pub fn new(operation: String, duration_ms: f64, success: bool) -> Self {
        Self {
            operation,
            duration_ms,
            success,
            details: None,
        }
    }

    /// 添加详细信息
    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    /// 记录性能日志
    pub fn log(&self, config: &LoggingConfig) {
        if !config.enable_performance_logs {
            return;
        }

        let status = if self.success { "SUCCESS" } else { "FAILED" };
        let details = self.details.as_deref().unwrap_or("");
        
        if self.success {
            perf_log!(config, info, 
                "Operation: {} | Duration: {:.2}ms | Status: {} | Details: {}",
                self.operation, self.duration_ms, status, details
            );
        } else {
            perf_log!(config, warn,
                "Operation: {} | Duration: {:.2}ms | Status: {} | Details: {}",
                self.operation, self.duration_ms, status, details
            );
        }
    }
}

/// 审计日志结构
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub event_type: String,
    pub user_id: Option<String>,
    pub resource: String,
    pub action: String,
    pub result: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl AuditEvent {
    /// 创建新的审计事件
    pub fn new(event_type: String, resource: String, action: String, result: String) -> Self {
        Self {
            event_type,
            user_id: None,
            resource,
            action,
            result,
            timestamp: chrono::Utc::now(),
        }
    }

    /// 设置用户 ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// 记录审计日志
    pub fn log(&self, config: &LoggingConfig) {
        if !config.enable_audit_logs {
            return;
        }

        let user_info = self.user_id.as_deref().unwrap_or("anonymous");
        
        audit_log!(config, info,
            "Type: {} | User: {} | Resource: {} | Action: {} | Result: {} | Time: {}",
            self.event_type, user_info, self.resource, self.action, self.result,
            self.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
}

/// 日志工具函数
pub mod utils {
    use super::*;
    use std::time::Instant;

    /// 性能计时器
    pub struct Timer {
        start: Instant,
        operation: String,
    }

    impl Timer {
        /// 开始计时
        pub fn start(operation: String) -> Self {
            Self {
                start: Instant::now(),
                operation,
            }
        }

        /// 结束计时并记录日志
        pub fn finish(self, config: &LoggingConfig, success: bool) -> PerformanceMetrics {
            let duration = self.start.elapsed();
            let duration_ms = duration.as_secs_f64() * 1000.0;
            
            let metrics = PerformanceMetrics::new(self.operation, duration_ms, success);
            metrics.log(config);
            metrics
        }

        /// 结束计时并记录带详细信息的日志
        pub fn finish_with_details(
            self, 
            config: &LoggingConfig, 
            success: bool, 
            details: String
        ) -> PerformanceMetrics {
            let duration = self.start.elapsed();
            let duration_ms = duration.as_secs_f64() * 1000.0;
            
            let metrics = PerformanceMetrics::new(self.operation, duration_ms, success)
                .with_details(details);
            metrics.log(config);
            metrics
        }
    }

    /// 格式化字节大小
    pub fn format_bytes(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }

    /// 格式化持续时间
    pub fn format_duration(duration_ms: f64) -> String {
        if duration_ms < 1.0 {
            format!("{:.3}ms", duration_ms)
        } else if duration_ms < 1000.0 {
            format!("{:.2}ms", duration_ms)
        } else {
            format!("{:.2}s", duration_ms / 1000.0)
        }
    }
}