//! 配置管理模块
//!
//! 定义缓存系统的各种配置选项和构建器模式

use crate::error::{CacheError, CacheResult};
use crate::types::EvictionStrategy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// 缓存系统主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// L1 缓存配置
    pub l1: L1Config,
    /// L2 缓存配置
    pub l2: L2Config,
    /// 压缩配置
    pub compression: CompressionConfig,
    /// TTL 配置
    pub ttl: TtlConfig,
    /// 性能配置
    pub performance: PerformanceConfig,
    /// 日志配置
    pub logging: LoggingConfig,
}

/// L1 内存缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Config {
    /// 最大内存使用量（字节）
    pub max_memory: usize,
    /// 最大条目数量
    pub max_entries: usize,
    /// 驱逐策略
    pub eviction_strategy: EvictionStrategy,
    /// 启用智能传输
    pub enable_smart_transfer: bool,
    /// 预分配内存池大小
    pub pool_size: usize,
}

/// L2 持久化缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Config {
    /// RocksDB 数据目录
    pub data_dir: PathBuf,
    /// 最大磁盘使用量（字节）
    pub max_disk_size: u64,
    /// 写缓冲区大小
    pub write_buffer_size: usize,
    /// 最大写缓冲区数量
    pub max_write_buffer_number: i32,
    /// 块缓存大小
    pub block_cache_size: usize,
    /// 启用压缩
    pub enable_compression: bool,
    /// 压缩级别
    pub compression_level: i32,
    /// 后台线程数
    pub background_threads: i32,
}

/// 压缩配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// 启用 LZ4 压缩
    pub enable_lz4: bool,
    /// 压缩阈值（字节），小于此值的数据不压缩
    pub compression_threshold: usize,
    /// 压缩级别（1-12，数字越大压缩率越高但速度越慢）
    pub compression_level: i32,
    /// 自动压缩检测
    pub auto_compression: bool,
    /// 压缩比率阈值，低于此比率的数据不存储压缩版本
    pub min_compression_ratio: f64,
}

/// TTL 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlConfig {
    /// 默认 TTL（秒），None 表示永不过期
    pub default_ttl: Option<u64>,
    /// 最大 TTL（秒）
    pub max_ttl: u64,
    /// TTL 检查间隔（秒）
    pub cleanup_interval: u64,
    /// 每次清理的最大条目数
    pub max_cleanup_entries: usize,
    /// 启用惰性过期（访问时检查）
    pub lazy_expiration: bool,
    /// 启用主动过期（定时清理）
    pub active_expiration: bool,
}

/// 性能配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// 工作线程数
    pub worker_threads: usize,
    /// 是否启用并发
    pub enable_concurrency: bool,
    /// 是否启用读写分离
    pub read_write_separation: bool,
    /// 批处理大小
    pub batch_size: usize,
    /// 是否启用预热
    pub enable_warmup: bool,
    /// 统计间隔（秒）
    pub stats_interval: u64,
    /// 是否启用后台统计
    pub enable_background_stats: bool,
    /// L2 写入策略
    pub l2_write_strategy: String,
    /// L2 写入阈值
    pub l2_write_threshold: usize,
    /// L2 写入 TTL 阈值
    pub l2_write_ttl_threshold: u64,
}

/// 日志配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// 日志级别
    pub level: String,
    /// 启用彩色输出
    pub enable_colors: bool,
    /// 显示时间戳
    pub show_timestamp: bool,
    /// 启用性能日志
    pub enable_performance_logs: bool,
    /// 启用操作审计日志
    pub enable_audit_logs: bool,
    /// 启用缓存操作日志
    pub enable_cache_logs: bool,
}

/// 配置构建器
#[derive(Debug)]
pub struct CacheConfigBuilder {
    l1_config: Option<L1Config>,
    l2_config: Option<L2Config>,
    compression_config: Option<CompressionConfig>,
    ttl_config: Option<TtlConfig>,
    performance_config: Option<PerformanceConfig>,
    logging_config: Option<LoggingConfig>,
}

impl CacheConfigBuilder {
    /// 创建新的配置构建器
    pub fn new() -> Self {
        Self {
            l1_config: None,
            l2_config: None,
            compression_config: None,
            ttl_config: None,
            performance_config: None,
            logging_config: None,
        }
    }

    /// 设置 L1 配置
    pub fn with_l1_config(mut self, config: L1Config) -> Self {
        self.l1_config = Some(config);
        self
    }

    /// 设置 L2 配置
    pub fn with_l2_config(mut self, config: L2Config) -> Self {
        self.l2_config = Some(config);
        self
    }

    /// 设置压缩配置
    pub fn with_compression_config(mut self, config: CompressionConfig) -> Self {
        self.compression_config = Some(config);
        self
    }

    /// 设置 TTL 配置
    pub fn with_ttl_config(mut self, config: TtlConfig) -> Self {
        self.ttl_config = Some(config);
        self
    }

    /// 设置性能配置
    pub fn with_performance_config(mut self, config: PerformanceConfig) -> Self {
        self.performance_config = Some(config);
        self
    }

    /// 设置日志配置
    pub fn with_logging_config(mut self, config: LoggingConfig) -> Self {
        self.logging_config = Some(config);
        self
    }

    /// 构建配置，所有配置项必须显式设置
    pub fn build(self) -> CacheResult<CacheConfig> {
        let l1_config = self.l1_config.ok_or_else(|| {
            CacheError::config_error("L1 配置未设置")
        })?;
        
        let l2_config = self.l2_config.ok_or_else(|| {
            CacheError::config_error("L2 配置未设置")
        })?;
        
        let compression_config = self.compression_config.ok_or_else(|| {
            CacheError::config_error("压缩配置未设置")
        })?;
        
        let ttl_config = self.ttl_config.ok_or_else(|| {
            CacheError::config_error("TTL 配置未设置")
        })?;
        
        let performance_config = self.performance_config.ok_or_else(|| {
            CacheError::config_error("性能配置未设置")
        })?;
        
        let logging_config = self.logging_config.ok_or_else(|| {
            CacheError::config_error("日志配置未设置")
        })?;

        // 验证配置的合法性
        Self::validate_config(&l1_config, &l2_config, &compression_config, &ttl_config)?;

        Ok(CacheConfig {
            l1: l1_config,
            l2: l2_config,
            compression: compression_config,
            ttl: ttl_config,
            performance: performance_config,
            logging: logging_config,
        })
    }

    /// 验证配置的合法性
    fn validate_config(
        l1_config: &L1Config,
        l2_config: &L2Config,
        compression_config: &CompressionConfig,
        ttl_config: &TtlConfig,
    ) -> CacheResult<()> {
        // 验证 L1 配置
        if l1_config.max_memory == 0 {
            return Err(CacheError::config_error("L1 最大内存不能为 0"));
        }
        if l1_config.max_entries == 0 {
            return Err(CacheError::config_error("L1 最大条目数不能为 0"));
        }
        if l1_config.pool_size == 0 {
            return Err(CacheError::config_error("内存池大小不能为 0"));
        }

        // 验证 L2 配置
        if l2_config.max_disk_size == 0 {
            return Err(CacheError::config_error("L2 最大磁盘大小不能为 0"));
        }
        if l2_config.write_buffer_size == 0 {
            return Err(CacheError::config_error("写缓冲区大小不能为 0"));
        }
        if l2_config.max_write_buffer_number <= 0 {
            return Err(CacheError::config_error("最大写缓冲区数量必须大于 0"));
        }
        if l2_config.background_threads <= 0 {
            return Err(CacheError::config_error("后台线程数必须大于 0"));
        }

        // 验证压缩配置
        if compression_config.compression_level < 1 || compression_config.compression_level > 12 {
            return Err(CacheError::config_error("压缩级别必须在 1-12 之间"));
        }
        if compression_config.min_compression_ratio < 0.0 || compression_config.min_compression_ratio > 1.0 {
            return Err(CacheError::config_error("最小压缩比率必须在 0.0-1.0 之间"));
        }

        // 验证 TTL 配置
        if ttl_config.max_ttl == 0 {
            return Err(CacheError::config_error("最大 TTL 不能为 0"));
        }
        if ttl_config.cleanup_interval == 0 {
            return Err(CacheError::config_error("清理间隔不能为 0"));
        }
        if ttl_config.max_cleanup_entries == 0 {
            return Err(CacheError::config_error("最大清理条目数不能为 0"));
        }
        if let Some(default_ttl) = ttl_config.default_ttl {
            if default_ttl > ttl_config.max_ttl {
                return Err(CacheError::config_error("默认 TTL 不能大于最大 TTL"));
            }
        }

        Ok(())
    }
}

impl Default for CacheConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 预设配置模板
impl CacheConfig {
    /// 开发环境配置
    pub fn development() -> CacheResult<Self> {
        CacheConfigBuilder::new()
            .with_l1_config(L1Config {
                max_memory: 64 * 1024 * 1024, // 64MB
                max_entries: 10_000,
                eviction_strategy: EvictionStrategy::Lru,
                enable_smart_transfer: true,
                pool_size: 1024,
            })
            .with_l2_config(L2Config {
                data_dir: PathBuf::from("./cache_data"),
                max_disk_size: 1024 * 1024 * 1024, // 1GB
                write_buffer_size: 64 * 1024 * 1024, // 64MB
                max_write_buffer_number: 3,
                block_cache_size: 32 * 1024 * 1024, // 32MB
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
            })
            .with_compression_config(CompressionConfig {
                enable_lz4: true,
                compression_threshold: 1024, // 1KB
                compression_level: 4,
                auto_compression: true,
                min_compression_ratio: 0.8,
            })
            .with_ttl_config(TtlConfig {
                default_ttl: Some(3600), // 1小时
                max_ttl: 86400, // 24小时
                cleanup_interval: 300, // 5分钟
                max_cleanup_entries: 1000,
                lazy_expiration: true,
                active_expiration: true,
            })
            .with_performance_config(PerformanceConfig {
                worker_threads: 4,
                enable_concurrency: true,
                read_write_separation: true,
                batch_size: 100,
                enable_warmup: false,
                stats_interval: 60,
                enable_background_stats: true,
                l2_write_strategy: "adaptive".to_string(),
                l2_write_threshold: 4096,
                l2_write_ttl_threshold: 300,
            })
            .with_logging_config(LoggingConfig {
                level: "debug".to_string(),
                enable_colors: true,
                show_timestamp: true,
                enable_performance_logs: true,
                enable_audit_logs: false,
                enable_cache_logs: true,
            })
            .build()
    }

    /// 生产环境配置
    pub fn production() -> CacheResult<Self> {
        CacheConfigBuilder::new()
            .with_l1_config(L1Config {
                max_memory: 512 * 1024 * 1024, // 512MB
                max_entries: 100_000,
                eviction_strategy: EvictionStrategy::LruLfu,
                enable_smart_transfer: true,
                pool_size: 4096,
            })
            .with_l2_config(L2Config {
                data_dir: PathBuf::from("/var/lib/rat_memcache"),
                max_disk_size: 10 * 1024 * 1024 * 1024, // 10GB
                write_buffer_size: 128 * 1024 * 1024, // 128MB
                max_write_buffer_number: 6,
                block_cache_size: 256 * 1024 * 1024, // 256MB
                enable_compression: true,
                compression_level: 9,
                background_threads: 8,
            })
            .with_compression_config(CompressionConfig {
                enable_lz4: true,
                compression_threshold: 512, // 512B
                compression_level: 6,
                auto_compression: true,
                min_compression_ratio: 0.7,
            })
            .with_ttl_config(TtlConfig {
                default_ttl: Some(7200), // 2小时
                max_ttl: 604800, // 7天
                cleanup_interval: 60, // 1分钟
                max_cleanup_entries: 5000,
                lazy_expiration: true,
                active_expiration: true,
            })
            .with_performance_config(PerformanceConfig {
                worker_threads: 16,
                enable_concurrency: true,
                read_write_separation: true,
                batch_size: 500,
                enable_warmup: true,
                stats_interval: 30,
                enable_background_stats: true,
                l2_write_strategy: "adaptive".to_string(),
                l2_write_threshold: 2048,
                l2_write_ttl_threshold: 600,
            })
            .with_logging_config(LoggingConfig {
                level: "info".to_string(),
                enable_colors: false,
                show_timestamp: true,
                enable_performance_logs: true,
                enable_audit_logs: true,
                enable_cache_logs: true,
            })
            .build()
    }
}