//! 配置管理模块
//!
//! 定义缓存系统的各种配置选项和构建器模式

use crate::error::{CacheError, CacheResult};
use crate::types::EvictionStrategy;
#[cfg(feature = "melange-storage")]
use crate::melange_adapter::CompressionAlgorithm;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use sysinfo::System;
use rat_logger;

/// 缓存系统主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// L1 缓存配置
    pub l1: L1Config,
    /// L2 缓存配置
    #[cfg(feature = "melange-storage")]
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
}

/// L2 持久化缓存配置
#[cfg(feature = "melange-storage")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Config {
    /// 启用 L2 缓存（MelangeDB 持久化存储）
    pub enable_l2_cache: bool,
    /// 数据目录（可选，None 时使用临时目录）
    pub data_dir: Option<PathBuf>,
    /// 启动时清空缓存目录
    #[serde(default)]
    pub clear_on_startup: bool,
    /// 最大磁盘使用量（字节）
    #[serde(default)]
    pub max_disk_size: u64,
    /// 写缓冲区大小
    #[serde(default)]
    pub write_buffer_size: usize,
    /// 最大写缓冲区数量
    #[serde(default)]
    pub max_write_buffer_number: i32,
    /// 块缓存大小
    #[serde(default)]
    pub block_cache_size: usize,
    /// 启用压缩
    #[serde(default)]
    pub enable_compression: bool,
    /// 压缩级别
    #[serde(default)]
    pub compression_level: i32,
    /// 后台线程数
    #[serde(default)]
    pub background_threads: i32,
    /// MelangeDB 压缩算法
    #[serde(default)]
    pub compression_algorithm: CompressionAlgorithm,
    /// MelangeDB 缓存大小（MB）
    #[serde(default)]
    pub cache_size_mb: usize,
    /// 最大文件大小（MB）
    #[serde(default)]
    pub max_file_size_mb: usize,
    /// 智能flush配置
    #[serde(default)]
    pub smart_flush_enabled: bool,
    #[serde(default)]
    pub smart_flush_base_interval_ms: usize,
    #[serde(default)]
    pub smart_flush_min_interval_ms: usize,
    #[serde(default)]
    pub smart_flush_max_interval_ms: usize,
    #[serde(default)]
    pub smart_flush_write_rate_threshold: usize,
    #[serde(default)]
    pub smart_flush_accumulated_bytes_threshold: usize,
    /// 缓存预热策略
    #[serde(default)]
    pub cache_warmup_strategy: CacheWarmupStrategy,
    /// ZSTD压缩级别（仅当使用ZSTD压缩时有效）
    #[serde(default)]
    pub zstd_compression_level: Option<i32>,
    /// L2 写入策略
    #[serde(default)]
    pub l2_write_strategy: String,
    /// L2 写入阈值
    #[serde(default)]
    pub l2_write_threshold: usize,
    /// L2 写入 TTL 阈值
    #[serde(default)]
    pub l2_write_ttl_threshold: u64,
}

#[cfg(feature = "melange-storage")]
impl Default for L2Config {
    fn default() -> Self {
        Self {
            enable_l2_cache: false,
            data_dir: None,
            clear_on_startup: false,
            max_disk_size: 1024 * 1024 * 1024, // 1GB
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            block_cache_size: 32 * 1024 * 1024, // 32MB
            enable_compression: true,
            compression_level: 6,
            background_threads: 2,
            compression_algorithm: CompressionAlgorithm::Lz4,
            cache_size_mb: 512,
            max_file_size_mb: 1024,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 500,
            smart_flush_write_rate_threshold: 10000,
            smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024, // 4MB
            cache_warmup_strategy: CacheWarmupStrategy::Recent,
            zstd_compression_level: None,
            l2_write_strategy: "write_through".to_string(),
            l2_write_threshold: 1024,
            l2_write_ttl_threshold: 300,
        }
    }
}


/// 缓存预热策略
#[cfg(feature = "melange-storage")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheWarmupStrategy {
    /// 无预热
    None,
    /// 预热最近访问的数据
    Recent,
    /// 预热热点数据
    Hot,
    /// 全部预热
    Full,
}

#[cfg(feature = "melange-storage")]
impl Default for CacheWarmupStrategy {
    fn default() -> Self {
        CacheWarmupStrategy::Recent
    }
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
    /// 数据过期时间（秒），None 表示永不过期
    pub expire_seconds: Option<u64>,
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
    /// 大值阈值（字节），超过此值的数据直接写入L2或抛弃
    pub large_value_threshold: usize,
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
    #[cfg(feature = "melange-storage")]
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
            #[cfg(feature = "melange-storage")]
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
    #[cfg(feature = "melange-storage")]
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

    /// 构建配置，所有配置项必须显式设置，并强制执行验证
    pub fn build(self) -> CacheResult<CacheConfig> {
        let l1_config = self.l1_config.ok_or_else(|| {
            CacheError::config_error("L1 配置未设置")
        })?;
        
          #[cfg(feature = "melange-storage")]
        let l2_config = self.l2_config.unwrap_or_else(L2Config::default);
        
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

            // 强制验证配置的合法性
        #[cfg(feature = "melange-storage")]
        Self::validate_config(&l1_config, &l2_config, &compression_config, &ttl_config, &performance_config)?;
        #[cfg(not(feature = "melange-storage"))]
        Self::validate_config(&l1_config, &compression_config, &ttl_config, &performance_config)?;

        let config = CacheConfig {
            l1: l1_config,
            #[cfg(feature = "melange-storage")]
            l2: l2_config,
            compression: compression_config,
            ttl: ttl_config,
            performance: performance_config,
            logging: logging_config,
        };
        
        // 最终验证整体配置的一致性
        Self::validate_overall_config(&config)?;
        
        Ok(config)
    }

    /// 验证配置的合法性
    #[cfg(feature = "melange-storage")]
    fn validate_config(
        l1_config: &L1Config,
        l2_config: &L2Config,
        compression_config: &CompressionConfig,
        ttl_config: &TtlConfig,
        performance_config: &PerformanceConfig,
    ) -> CacheResult<()> {
        // 验证 L1 配置
        if l1_config.max_memory == 0 {
            return Err(CacheError::config_error("L1 最大内存不能为 0"));
        }
        if l1_config.max_entries == 0 {
            return Err(CacheError::config_error("L1 最大条目数不能为 0"));
        }

        // 验证 L2 配置（仅在启用时验证）
        if l2_config.enable_l2_cache {
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

            // 验证 L2 写入策略
            let valid_strategies = ["always", "never", "size_based", "ttl_based", "adaptive", "write_through"];
            if !valid_strategies.contains(&l2_config.l2_write_strategy.as_str()) {
                return Err(CacheError::config_error(&format!(
                    "无效的 L2 写入策略: {}，有效值: {:?}",
                    l2_config.l2_write_strategy, valid_strategies
                )));
            }

            // 验证 L2 路径（如果指定了路径）
            if let Some(ref data_dir) = l2_config.data_dir {
                PathUtils::validate_writable_path(data_dir)?;
            }
        }

        // 验证压缩配置
        if compression_config.compression_level < 1 || compression_config.compression_level > 12 {
            return Err(CacheError::config_error("压缩级别必须在 1-12 之间"));
        }
        if compression_config.min_compression_ratio < 0.0 || compression_config.min_compression_ratio > 1.0 {
            return Err(CacheError::config_error("最小压缩比率必须在 0.0-1.0 之间"));
        }

        // 验证 TTL 配置
        if ttl_config.cleanup_interval == 0 {
            return Err(CacheError::config_error("清理间隔不能为 0"));
        }
        if ttl_config.max_cleanup_entries == 0 {
            return Err(CacheError::config_error("最大清理条目数不能为 0"));
        }
        
        // 验证性能配置
        if performance_config.worker_threads == 0 {
            return Err(CacheError::config_error("工作线程数不能为 0"));
        }
        if performance_config.batch_size == 0 {
            return Err(CacheError::config_error("批处理大小不能为 0"));
        }
        
        Ok(())
    }

    /// 验证配置的合法性（不包含L2）
    #[cfg(not(feature = "melange-storage"))]
    fn validate_config(
        l1_config: &L1Config,
        compression_config: &CompressionConfig,
        ttl_config: &TtlConfig,
        performance_config: &PerformanceConfig,
    ) -> CacheResult<()> {
        // 验证 L1 配置
        if l1_config.max_memory == 0 {
            return Err(CacheError::config_error("L1 最大内存不能为 0"));
        }
        if l1_config.max_entries == 0 {
            return Err(CacheError::config_error("L1 最大条目数不能为 0"));
        }

        // 验证压缩配置
        if compression_config.compression_level < 1 || compression_config.compression_level > 12 {
            return Err(CacheError::config_error("压缩级别必须在 1-12 之间"));
        }
        if compression_config.min_compression_ratio < 0.0 || compression_config.min_compression_ratio > 1.0 {
            return Err(CacheError::config_error("最小压缩比率必须在 0.0-1.0 之间"));
        }

        // 验证 TTL 配置
        if ttl_config.cleanup_interval == 0 {
            return Err(CacheError::config_error("清理间隔不能为 0"));
        }
        if ttl_config.max_cleanup_entries == 0 {
            return Err(CacheError::config_error("最大清理条目数不能为 0"));
        }

        // 验证性能配置
        if performance_config.worker_threads == 0 {
            return Err(CacheError::config_error("工作线程数不能为 0"));
        }
        if performance_config.batch_size == 0 {
            return Err(CacheError::config_error("批处理大小不能为 0"));
        }
        
        Ok(())
    }

    /// 验证整体配置的一致性
    fn validate_overall_config(config: &CacheConfig) -> CacheResult<()> {
        // 检查内存使用是否合理
        let system_info = SystemInfo::get();
        let l1_memory_mb = config.l1.max_memory / (1024 * 1024);
        
        // 只有当可用内存大于 0 时才进行检查
        if system_info.available_memory > 0 {
            let available_memory_mb = system_info.available_memory / (1024 * 1024);
            
            if config.l1.max_memory > (system_info.available_memory as usize / 2) {
                return Err(CacheError::config_error(&format!(
                    "L1 缓存内存 ({} MB) 超过可用内存的一半 ({} MB)，可能导致系统不稳定",
                    l1_memory_mb, available_memory_mb / 2
                )));
            }
        } else {
            rat_logger::debug!("无法获取可用内存信息，跳过内存检查");
        }
        
        // 检查工作线程数是否合理
        if config.performance.worker_threads > system_info.cpu_count * 4 {
            return Err(CacheError::config_error(&format!(
                "工作线程数 ({}) 超过 CPU 核心数的 4 倍 ({}×4={})",
                config.performance.worker_threads,
                system_info.cpu_count,
                system_info.cpu_count * 4
            )));
        }
        
        // 检查 L2 和压缩配置的一致性
        #[cfg(feature = "melange-storage")]
        if config.l2.enable_l2_cache && config.l2.enable_compression && !config.compression.enable_lz4 {
            return Err(CacheError::config_error(
                "L2 缓存启用了压缩，但全局压缩配置未启用 LZ4"
            ));
        }
        
        Ok(())
    }
}

impl Default for CacheConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 系统信息获取工具
struct SystemInfo {
    total_memory: u64,
    available_memory: u64,
    cpu_count: usize,
}

impl SystemInfo {
    /// 获取当前系统信息
    fn get() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        
        Self {
            total_memory: sys.total_memory(),
            available_memory: sys.available_memory(),
            cpu_count: sys.cpus().len(),
        }
    }
    
    /// 计算推荐的 L1 缓存大小（可用内存的 25%，但不超过 2GB）
    fn recommended_l1_memory(&self) -> usize {
        let quarter_memory = (self.available_memory / 4) as usize;
        let max_l1_memory = 2 * 1024 * 1024 * 1024; // 2GB
        quarter_memory.min(max_l1_memory)
    }
    
    /// 计算推荐的工作线程数（CPU 核心数的 2 倍，但不超过 32）
    fn recommended_worker_threads(&self) -> usize {
        (self.cpu_count * 2).min(32).max(4)
    }
    
}

/// 跨平台路径工具
struct PathUtils;

impl PathUtils {
    /// 获取跨平台的默认缓存目录
    fn default_cache_dir() -> CacheResult<PathBuf> {
        rat_logger::debug!("获取默认缓存目录");
        // 使用系统临时目录，确保跨平台兼容性
        let temp_dir = std::env::temp_dir();
        rat_logger::debug!("系统临时目录: {:?}", temp_dir);
        
        let cache_dir = temp_dir.join("rat_memcache");
        rat_logger::debug!("缓存目录路径: {:?}", cache_dir);
        
        rat_logger::debug!("尝试创建缓存目录...");
        match std::fs::create_dir_all(&cache_dir) {
            Ok(_) => rat_logger::debug!("缓存目录创建成功"),
            Err(e) => {
                rat_logger::debug!("创建缓存目录失败: {}", e);
                return Err(CacheError::config_error(&format!("创建缓存目录失败: {}", e)));
            }
        }
        
        // 返回系统临时目录中的缓存目录路径
        rat_logger::debug!("返回缓存目录: {:?}", cache_dir);
        Ok(cache_dir)
    }
    
    /// 验证路径是否可写
    fn validate_writable_path(path: &PathBuf) -> CacheResult<()> {
        rat_logger::debug!("验证路径是否可写: {:?}", path);
        rat_logger::debug!("路径是否存在: {}", path.exists());
        
        // 确保目标目录存在（包括所有父目录）
        if !path.exists() {
            rat_logger::debug!("目标目录不存在，尝试创建: {:?}", path);
            match std::fs::create_dir_all(path) {
                Ok(_) => rat_logger::debug!("目标目录创建成功"),
                Err(e) => {
                    rat_logger::debug!("创建目标目录失败: {}", e);
                    return Err(CacheError::config_error(&format!("创建目录失败: {}", e)));
                }
            }
        }
        
        // 尝试创建测试文件
        let test_file = path.join(".write_test");
        rat_logger::debug!("尝试创建测试文件: {:?}", test_file);
        match std::fs::write(&test_file, b"test") {
            Ok(_) => rat_logger::debug!("测试文件创建成功"),
            Err(e) => {
                rat_logger::debug!("创建测试文件失败: {}", e);
                return Err(CacheError::config_error(&format!("路径不可写: {}", e)));
            }
        }
        
        // 清理测试文件
        rat_logger::debug!("尝试删除测试文件...");
        match std::fs::remove_file(&test_file) {
            Ok(_) => rat_logger::debug!("测试文件删除成功"),
            Err(e) => rat_logger::debug!("删除测试文件失败: {}", e)
        }
        
        rat_logger::debug!("路径验证成功");
        Ok(())
    }
}

// 默认值函数
#[cfg(feature = "melange-storage")]
fn default_melange_compression() -> CompressionAlgorithm {
    CompressionAlgorithm::Lz4
}

#[cfg(feature = "melange-storage")]
fn default_melange_cache_size() -> usize {
    512 // MB
}

#[cfg(feature = "melange-storage")]
fn default_melange_max_file_size() -> usize {
    1024 // MB
}

#[cfg(feature = "melange-storage")]
fn default_melange_stats_enabled() -> bool {
    true
}

#[cfg(feature = "melange-storage")]
fn default_melange_smart_flush_enabled() -> bool {
    true
}

#[cfg(feature = "melange-storage")]
fn default_melange_smart_flush_base_interval() -> usize {
    100 // ms
}

#[cfg(feature = "melange-storage")]
fn default_melange_smart_flush_min_interval() -> usize {
    20 // ms
}

#[cfg(feature = "melange-storage")]
fn default_melange_smart_flush_max_interval() -> usize {
    500 // ms (Surface Book 2优化配置)
}

#[cfg(feature = "melange-storage")]
fn default_melange_smart_flush_write_threshold() -> usize {
    10000
}

#[cfg(feature = "melange-storage")]
fn default_melange_smart_flush_bytes_threshold() -> usize {
    4 * 1024 * 1024 // 4MB
}

#[cfg(feature = "melange-storage")]
fn default_melange_warmup_strategy() -> CacheWarmupStrategy {
    CacheWarmupStrategy::Recent
}