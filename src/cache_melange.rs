//! 双层缓存主模块 - MelangeDB 版本
//!
//! 整合 L1 内存缓存和 L2 MelangeDB 持久化缓存，提供统一的缓存接口

use crate::compression::Compressor;
use crate::config::{CacheConfig, CacheConfigBuilder, DatabaseEngine};
use crate::error::{CacheError, CacheResult};
use crate::l1_cache::{L1Cache, L1CacheStats};
use crate::l2_cache_melange::{L2CacheMelange, L2CacheStats};
use crate::logging::LogManager;
use crate::metrics::MetricsCollector;
use crate::ttl::TtlManager;
use crate::types::{CacheLayer, CacheOperation};
use crate::{cache_log, perf_log, transfer_log};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

/// 双层缓存系统 - MelangeDB 版本
#[derive(Debug)]
pub struct RatMemCacheMelange {
    /// 配置
    config: Arc<CacheConfig>,
    /// L1 内存缓存
    l1_cache: Arc<L1Cache>,
    /// L2 持久化缓存（可选，仅在启用时存在）
    l2_cache: Option<Arc<L2CacheMelange>>,
    /// TTL 管理器
    ttl_manager: Arc<TtlManager>,
    /// 指标收集器
    metrics: Arc<MetricsCollector>,
    /// 日志管理器
    log_manager: Arc<LogManager>,
    /// 压缩器
    compressor: Arc<Compressor>,
    /// 运行状态
    is_running: Arc<RwLock<bool>>,
}

/// 缓存构建器 - MelangeDB 版本
#[derive(Debug)]
pub struct RatMemCacheMelangeBuilder {
    config_builder: CacheConfigBuilder,
}

/// 缓存操作选项
#[derive(Debug, Clone)]
pub struct CacheOptionsMelange {
    /// TTL（秒）
    pub ttl_seconds: Option<u64>,
    /// 是否强制写入 L2
    pub force_l2: bool,
    /// 是否跳过 L1
    pub skip_l1: bool,
    /// 是否启用压缩
    pub enable_compression: Option<bool>,
}

impl Default for CacheOptionsMelange {
    fn default() -> Self {
        Self {
            ttl_seconds: None,
            force_l2: false,
            skip_l1: false,
            enable_compression: None,
        }
    }
}

impl RatMemCacheMelangeBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            config_builder: CacheConfigBuilder::new(),
        }
    }

    /// 设置 L1 缓存配置
    pub fn l1_config(mut self, config: crate::config::L1Config) -> Self {
        self.config_builder = self.config_builder.with_l1_config(config);
        self
    }

    /// 设置 L2 缓存配置
    pub fn l2_config(mut self, config: crate::config::L2Config) -> Self {
        self.config_builder = self.config_builder.with_l2_config(config);
        self
    }

    /// 设置压缩配置
    pub fn compression_config(mut self, config: crate::config::CompressionConfig) -> Self {
        self.config_builder = self.config_builder.with_compression_config(config);
        self
    }

    /// 设置 TTL 配置
    pub fn ttl_config(mut self, config: crate::config::TtlConfig) -> Self {
        self.config_builder = self.config_builder.with_ttl_config(config);
        self
    }

    /// 设置性能配置
    pub fn performance_config(mut self, config: crate::config::PerformanceConfig) -> Self {
        self.config_builder = self.config_builder.with_performance_config(config);
        self
    }

    /// 设置日志配置
    pub fn logging_config(mut self, config: crate::config::LoggingConfig) -> Self {
        self.config_builder = self.config_builder.with_logging_config(config);
        self
    }

    /// 使用开发环境预设
    pub fn development_preset(mut self) -> CacheResult<Self> {
        self.config_builder = CacheConfigBuilder::new();
        let config = CacheConfig::development()?;
        self.config_builder = CacheConfigBuilder::new()
            .with_l1_config(config.l1)
            .with_l2_config(config.l2)
            .with_compression_config(config.compression)
            .with_ttl_config(config.ttl)
            .with_performance_config(config.performance)
            .with_logging_config(config.logging);
        Ok(self)
    }

    /// 使用生产环境预设
    pub fn production_preset(mut self) -> CacheResult<Self> {
        self.config_builder = CacheConfigBuilder::new();
        let config = CacheConfig::production()?;
        self.config_builder = CacheConfigBuilder::new()
            .with_l1_config(config.l1)
            .with_l2_config(config.l2)
            .with_compression_config(config.compression)
            .with_ttl_config(config.ttl)
            .with_performance_config(config.performance)
            .with_logging_config(config.logging);
        Ok(self)
    }

    /// 构建缓存实例
    pub async fn build(self) -> CacheResult<RatMemCacheMelange> {
        let config = self.config_builder.build()?;
        RatMemCacheMelange::new(config).await
    }
}

impl Default for RatMemCacheMelangeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RatMemCacheMelange {
    /// 创建新的缓存实例
    pub async fn new(config: CacheConfig) -> CacheResult<Self> {
        let start_time = Instant::now();

        cache_log!(config.logging, debug, "RatMemCacheMelange::new 开始初始化");
        cache_log!(config.logging, debug, "配置: {:?}", config);

        // 验证数据库引擎
        if config.l2.enable_l2_cache && config.l2.database_engine != DatabaseEngine::MelangeDB {
            return Err(CacheError::config_error("此实现只支持 MelangeDB 引擎"));
        }

        // 初始化日志管理器
        cache_log!(config.logging, debug, "初始化日志管理器");
        let log_manager = Arc::new(LogManager::new(config.logging.clone()));

        // 初始化压缩器
        cache_log!(config.logging, debug, "初始化压缩器");
        let compressor = Arc::new(Compressor::new(config.compression.clone()));

        // 初始化 TTL 管理器
        cache_log!(config.logging, debug, "初始化 TTL 管理器");
        let ttl_manager = Arc::new(TtlManager::new(config.ttl.clone(), config.logging.clone()).await?);

        // 初始化指标收集器
        cache_log!(config.logging, debug, "初始化指标收集器");
        let metrics = Arc::new(MetricsCollector::new().await?);

        // 初始化 L1 缓存
        cache_log!(config.logging, debug, "初始化 L1 缓存");
        let l1_cache = Arc::new(
            L1Cache::new(
                config.l1.clone(),
                config.logging.clone(),
                compressor.as_ref().clone(),
                Arc::clone(&ttl_manager),
                Arc::clone(&metrics),
            ).await?
        );
        cache_log!(config.logging, debug, "L1 缓存初始化成功");

        // 初始化 L2 缓存（如果启用）
        cache_log!(config.logging, debug, "检查是否启用 L2 缓存: {}", config.l2.enable_l2_cache);
        cache_log!(config.logging, debug, "L2 缓存配置: {:?}", config.l2);
        let l2_cache = if config.l2.enable_l2_cache {
            cache_log!(config.logging, debug, "开始初始化 L2 缓存（MelangeDB）");
            cache_log!(config.logging, debug, "L2 缓存数据目录: {:?}", config.l2.data_dir);

            // 手动验证 L2 缓存目录是否可写
            if let Some(dir) = &config.l2.data_dir {
                cache_log!(config.logging, debug, "手动验证 L2 缓存目录是否可写: {:?}", dir);
                cache_log!(config.logging, debug, "目录是否存在: {}", dir.exists());

                if !dir.exists() {
                    cache_log!(config.logging, debug, "尝试创建目录: {:?}", dir);
                    match std::fs::create_dir_all(dir) {
                        Ok(_) => cache_log!(config.logging, debug, "目录创建成功"),
                        Err(e) => cache_log!(config.logging, debug, "创建目录失败: {}", e)
                    }
                }

                // 测试目录是否可写
                let test_file = dir.join(".cache_write_test");
                cache_log!(config.logging, debug, "尝试写入测试文件: {:?}", test_file);
                match std::fs::write(&test_file, b"test") {
                    Ok(_) => {
                        cache_log!(config.logging, debug, "测试文件写入成功");
                        match std::fs::remove_file(&test_file) {
                            Ok(_) => cache_log!(config.logging, debug, "测试文件删除成功"),
                            Err(e) => cache_log!(config.logging, debug, "测试文件删除失败: {}", e)
                        }
                    },
                    Err(e) => cache_log!(config.logging, debug, "测试文件写入失败: {}", e)
                }
            } else {
                cache_log!(config.logging, debug, "L2 缓存数据目录未设置");
            }

            cache_log!(config.logging, debug, "调用 L2CacheMelange::new");
            let l2_cache_result = L2CacheMelange::new(
                config.l2.clone(),
                config.logging.clone(),
                compressor.as_ref().clone(),
                Arc::clone(&ttl_manager),
                Arc::clone(&metrics),
            ).await;

            match &l2_cache_result {
                Ok(_) => cache_log!(config.logging, debug, "L2CacheMelange::new 调用成功"),
                Err(e) => cache_log!(config.logging, debug, "L2CacheMelange::new 调用失败: {}", e)
            }

            Some(Arc::new(l2_cache_result?))
        } else {
            cache_log!(config.logging, debug, "L2 缓存已禁用，不创建任何实例");
            None
        };

        cache_log!(config.logging, debug, "创建 RatMemCacheMelange 实例");
        let cache = Self {
            config: Arc::new(config.clone()),
            l1_cache,
            l2_cache,
            ttl_manager,
            metrics,
            log_manager,
            compressor,
            is_running: Arc::new(RwLock::new(true)),
        };

        // 启动后台任务
        cache_log!(config.logging, debug, "启动后台任务");
        cache.start_background_tasks().await;

        let elapsed = start_time.elapsed();
        cache_log!(config.logging, debug, "RatMemCacheMelange 初始化完成，耗时: {:.2}ms", elapsed.as_millis());

        Ok(cache)
    }

    /// 获取缓存值
    pub async fn get(&self, key: &str) -> CacheResult<Option<Bytes>> {
        self.get_with_options(key, &CacheOptionsMelange::default()).await
    }

    /// 获取缓存值（带选项）
    pub async fn get_with_options(&self, key: &str, options: &CacheOptionsMelange) -> CacheResult<Option<Bytes>> {
        let start_time = Instant::now();

        // 检查 TTL
        if self.ttl_manager.is_expired(key).await {
            self.delete_internal(key).await?;
            self.record_operation(CacheOperation::Get, start_time.elapsed()).await;
            return Ok(None);
        }

        // 尝试从 L1 获取（除非跳过）
        if !options.skip_l1 {
            if let Some(value) = self.l1_cache.get(key).await? {
                transfer_log!(debug, "L1 缓存命中: {}", key);
                self.record_operation(CacheOperation::Get, start_time.elapsed()).await;
                return Ok(Some(value));
            }
        }

        // 尝试从 L2 获取（如果启用且存在）
        #[cfg(feature = "melange-storage")]
        if let Some(l2_cache) = &self.l2_cache {
            if let Some(value) = l2_cache.get(key).await? {
                transfer_log!(debug, "L2 缓存命中: {}", key);

                // 将数据提升到 L1（除非跳过）
                if !options.skip_l1 && !options.force_l2 {
                    let ttl = self.ttl_manager.get_ttl(key).await;
                    if let Err(e) = self.l1_cache.set(key.to_string(), value.clone(), ttl).await {
                        cache_log!(self.config.logging, warn, "L1 缓存设置失败: {} - {}", key, e);
                    }
                }

                self.record_operation(CacheOperation::Get, start_time.elapsed()).await;
                return Ok(Some(value));
            }
        }

        // 缓存未命中
        self.metrics.record_cache_miss().await;
        cache_log!(self.config.logging, debug, "缓存未命中: {}", key);

        self.record_operation(CacheOperation::Get, start_time.elapsed()).await;
        Ok(None)
    }

    /// 设置缓存值
    pub async fn set(&self, key: String, value: Bytes) -> CacheResult<()> {
        self.set_with_options(key, value, &CacheOptionsMelange::default()).await
    }

    /// 设置缓存值（带 TTL）
    pub async fn set_with_ttl(&self, key: String, value: Bytes, ttl_seconds: u64) -> CacheResult<()> {
        let options = CacheOptionsMelange {
            ttl_seconds: Some(ttl_seconds),
            ..Default::default()
        };
        self.set_with_options(key, value, &options).await
    }

    /// 设置缓存值（带选项）
    pub async fn set_with_options(&self, key: String, value: Bytes, options: &CacheOptionsMelange) -> CacheResult<()> {
        let start_time = Instant::now();

        // 验证 TTL
        if let Some(ttl) = options.ttl_seconds {
            if ttl > self.config.ttl.max_ttl {
                return Err(CacheError::invalid_ttl(ttl as i64));
            }
        }

        // 设置到 L1（除非跳过或强制 L2）
        if !options.skip_l1 && !options.force_l2 {
            if let Err(e) = self.l1_cache.set(key.clone(), value.clone(), options.ttl_seconds).await {
                cache_log!(self.config.logging, warn, "L1 缓存设置失败: {} - {}", key, e);
            }
        }

        // 根据策略决定是否写入 L2（仅在存在时）
        #[cfg(feature = "melange-storage")]
        let should_write_l2 = if let Some(_l2_cache) = &self.l2_cache {
            options.force_l2 || self.should_write_to_l2(&key, &value, options).await
        } else {
            false
        };
        #[cfg(not(feature = "melange-storage"))]
        let should_write_l2 = false;

        if should_write_l2 {
            #[cfg(feature = "melange-storage")]
            if let Some(l2_cache) = &self.l2_cache {
                l2_cache.set(key.clone(), value, options.ttl_seconds).await?;
            }
        }

        cache_log!(self.config.logging, debug, "缓存设置完成: {} (L1: {}, L2: {})",
            key, !options.skip_l1 && !options.force_l2, should_write_l2);

        self.record_operation(CacheOperation::Set, start_time.elapsed()).await;
        Ok(())
    }

    /// 删除缓存值
    pub async fn delete(&self, key: &str) -> CacheResult<bool> {
        let start_time = Instant::now();
        let deleted = self.delete_internal(key).await?;
        self.record_operation(CacheOperation::Delete, start_time.elapsed()).await;
        Ok(deleted)
    }

    /// 清空缓存
    pub async fn clear(&self) -> CacheResult<()> {
        let start_time = Instant::now();

        // 清空 L1 和 L2（如果存在）
        self.l1_cache.clear().await?;
        #[cfg(feature = "melange-storage")]
        if let Some(l2_cache) = &self.l2_cache {
            l2_cache.clear().await?;
        }

        // TTL 管理器会自动清理

        cache_log!(self.config.logging, debug, "缓存已清空");

        self.record_operation(CacheOperation::Clear, start_time.elapsed()).await;
        Ok(())
    }

    /// 检查键是否存在
    pub async fn contains_key(&self, key: &str) -> CacheResult<bool> {
        // 检查 TTL
        if self.ttl_manager.is_expired(key).await {
            self.delete_internal(key).await?;
            return Ok(false);
        }

        // 检查 L1
        if self.l1_cache.contains_key(key) {
            return Ok(true);
        }

        // 检查 L2（如果存在）
        #[cfg(feature = "melange-storage")]
        if let Some(l2_cache) = &self.l2_cache {
            l2_cache.contains_key(key).await
        } else {
            Ok(false)
        }
        #[cfg(not(feature = "melange-storage"))]
        {
            Ok(false)
        }
    }

    /// 获取所有键
    pub async fn keys(&self) -> CacheResult<Vec<String>> {
        let mut keys = std::collections::HashSet::<String>::new();

        // 收集 L1 键
        for key in self.l1_cache.keys() {
            if !self.ttl_manager.is_expired(&key).await {
                keys.insert(key);
            }
        }

        // 收集 L2 键（如果存在）
        #[cfg(feature = "melange-storage")]
        if let Some(l2_cache) = &self.l2_cache {
            for key in l2_cache.keys().await? {
                if !self.ttl_manager.is_expired(&key).await {
                    keys.insert(key);
                }
            }
        }

        Ok(keys.into_iter().collect::<Vec<String>>())
    }

    /// 获取缓存大小
    pub async fn len(&self) -> CacheResult<usize> {
        let keys = self.keys().await?;
        Ok(keys.len())
    }

    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> CacheResult<bool> {
        let len = self.len().await?;
        Ok(len == 0)
    }

    /// 获取 L1 缓存统计
    pub async fn get_l1_stats(&self) -> L1CacheStats {
        self.l1_cache.get_stats().await
    }

    /// 获取 L2 缓存统计
    #[cfg(feature = "melange-storage")]
    pub async fn get_l2_stats(&self) -> L2CacheStats {
        if let Some(l2_cache) = &self.l2_cache {
            l2_cache.get_stats().await
        } else {
            L2CacheStats::default()
        }
    }

    /// 压缩 L2 缓存
    #[cfg(feature = "melange-storage")]
    pub async fn compact(&self) -> CacheResult<()> {
        if let Some(l2_cache) = &self.l2_cache {
            l2_cache.compact().await
        } else {
            Ok(())
        }
    }

    /// 设置 TTL
    pub async fn set_ttl(&self, key: &str, ttl_seconds: u64) -> CacheResult<()> {
        if ttl_seconds > self.config.ttl.max_ttl {
            return Err(CacheError::invalid_ttl(ttl_seconds as i64));
        }

        let _ = self.ttl_manager.add_key(key.to_string(), Some(ttl_seconds)).await;
        Ok(())
    }

    /// 移除 TTL
    pub async fn remove_ttl(&self, key: &str) -> CacheResult<()> {
        self.ttl_manager.remove_key(key).await;
        Ok(())
    }

    /// 关闭缓存
    pub async fn shutdown(&self) -> CacheResult<()> {
        cache_log!(self.config.logging, info, "开始关闭 RatMemCacheMelange...");

        // 设置停止标志
        {
            let mut running = self.is_running.write().await;
            *running = false;
        }

        // 等待后台任务完成
        tokio::time::sleep(Duration::from_millis(100)).await;

        // TTL 管理器会自动清理

        cache_log!(self.config.logging, info, "RatMemCacheMelange 已关闭");
        Ok(())
    }

    /// 内部删除方法
    async fn delete_internal(&self, key: &str) -> CacheResult<bool> {
        let mut deleted = false;

        // 从 L1 删除
        if self.l1_cache.delete(key).await? {
            deleted = true;
        }

        // 从 L2 删除（如果存在）
        #[cfg(feature = "melange-storage")]
        if let Some(l2_cache) = &self.l2_cache {
            if l2_cache.delete(key).await? {
                deleted = true;
            }
        }

        // 移除 TTL
        self.ttl_manager.remove_key(key).await;

        if deleted {
            cache_log!(self.config.logging, debug, "缓存删除: {}", key);
        }

        Ok(deleted)
    }

    /// 判断是否应该写入 L2
    async fn should_write_to_l2(&self, _key: &str, value: &Bytes, options: &CacheOptionsMelange) -> bool {
        // 如果强制 L2，直接返回 true
        if options.force_l2 {
            return true;
        }

        // 根据配置的写入策略决定
        match self.config.performance.l2_write_strategy.as_str() {
            "always" => true,
            "never" => false,
            "size_based" => value.len() >= self.config.performance.l2_write_threshold,
            "ttl_based" => options.ttl_seconds.unwrap_or(0) >= self.config.performance.l2_write_ttl_threshold,
            "adaptive" => {
                // 自适应策略：基于 L1 使用率和数据大小
                let l1_stats = self.l1_cache.get_stats().await;
                let l1_usage_ratio = l1_stats.memory_usage as f64 / self.config.l1.max_memory as f64;

                l1_usage_ratio > 0.8 || value.len() >= self.config.performance.l2_write_threshold
            },
            _ => false,
        }
    }

    /// 记录操作统计
    async fn record_operation(&self, operation: CacheOperation, duration: std::time::Duration) {
        // 使用指标系统记录操作
        self.metrics.record_cache_operation(operation).await;
        self.metrics.record_operation_latency(operation, duration).await;
    }

    /// 启动后台任务
    async fn start_background_tasks(&self) {
        // 启动统计更新任务
        if self.config.performance.enable_background_stats {
            let cache = Arc::new(self.clone());
            let stats_interval = cache.config.performance.stats_interval;
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(stats_interval));

                loop {
                    interval.tick().await;

                    let running = {
                        let running = cache.is_running.read().await;
                        *running
                    };

                    if !running {
                        break;
                    }

                    // 更新统计信息
                    if let Err(e) = cache.update_background_stats().await {
                        cache_log!(cache.config.logging, warn, "更新后台统计失败: {}", e);
                    }
                }
            });
        }

        // 启动性能监控任务
        if self.config.logging.enable_performance_logs {
            let cache = Arc::new(self.clone());
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(60)); // 每分钟记录一次

                loop {
                    interval.tick().await;

                    let running = {
                        let running = cache.is_running.read().await;
                        *running
                    };

                    if !running {
                        break;
                    }

                    // 记录性能日志
                    let l1_stats = cache.get_l1_stats().await;
                    #[cfg(feature = "melange-storage")]
                    let l2_stats = cache.get_l2_stats().await;

                    #[cfg(feature = "melange-storage")]
                    perf_log!(cache.config.logging, debug,
                        "缓存性能统计 (MelangeDB) - L1内存使用: {}MB, L2磁盘使用: {}MB, L2命中: {}, L2未命中: {}",
                        l1_stats.memory_usage / 1024 / 1024,
                        l2_stats.estimated_disk_usage / 1024 / 1024,
                        l2_stats.hits,
                        l2_stats.misses
                    );
                    #[cfg(not(feature = "melange-storage"))]
                    perf_log!(cache.config.logging, debug,
                        "缓存性能统计 (MelangeDB) - L1内存使用: {}MB",
                        l1_stats.memory_usage / 1024 / 1024
                    );
                }
            });
        }
    }

    /// 更新后台统计
    async fn update_background_stats(&self) -> CacheResult<()> {
        // 更新内存使用统计
        let l1_stats = self.l1_cache.get_stats().await;

        self.metrics.record_memory_usage(CacheLayer::Memory, l1_stats.memory_usage as u64).await;

        #[cfg(feature = "melange-storage")]
        {
            let l2_stats = if let Some(l2_cache) = &self.l2_cache {
                l2_cache.get_stats().await
            } else {
                L2CacheStats::default()
            };
            self.metrics.record_memory_usage(CacheLayer::Persistent, l2_stats.estimated_disk_usage as u64).await;
        }

        #[cfg(not(feature = "melange-storage"))]
        {
            self.metrics.record_memory_usage(CacheLayer::Persistent, 0).await;
        }

        Ok(())
    }
}

// 实现 Clone trait 以支持在异步任务中使用
impl Clone for RatMemCacheMelange {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            l1_cache: Arc::clone(&self.l1_cache),
            l2_cache: self.l2_cache.as_ref().map(|cache| Arc::clone(cache)),
            ttl_manager: Arc::clone(&self.ttl_manager),
            metrics: Arc::clone(&self.metrics),
            log_manager: Arc::clone(&self.log_manager),
            compressor: Arc::clone(&self.compressor),
            is_running: Arc::clone(&self.is_running),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CacheConfigBuilder, DatabaseEngine, MelangeSpecificConfig};
    use crate::melange_adapter::CompressionAlgorithm;
    use bytes::Bytes;
    use tempfile::TempDir;

    async fn create_test_cache() -> (RatMemCacheMelange, TempDir) {
        let temp_dir = TempDir::new().unwrap();

        let cache = RatMemCacheMelangeBuilder::new()
            .development_preset().unwrap()
            .l2_config(crate::config::L2Config {
                enable_l2_cache: true,
                data_dir: Some(temp_dir.path().to_path_buf()),
                max_disk_size: 10 * 1024 * 1024, // 10MB
                write_buffer_size: 1024 * 1024,  // 1MB
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,    // 512KB
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
                clear_on_startup: false,
                database_engine: DatabaseEngine::MelangeDB,
                melange_config: MelangeSpecificConfig {
                    compression_algorithm: CompressionAlgorithm::Lz4,
                    cache_size_mb: 256,
                    max_file_size_mb: 512,
                    enable_statistics: true,
                },
            })
            .ttl_config(crate::config::TtlConfig {
                default_ttl: Some(60),
                max_ttl: 3600,
                cleanup_interval: 60,
                max_cleanup_entries: 100,
                lazy_expiration: true,
                active_expiration: false, // 测试中禁用主动过期
            })
            .build()
            .await
            .unwrap();

        (cache, temp_dir)
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let (cache, _temp_dir) = create_test_cache().await;
        let is_empty = cache.is_empty().await.unwrap();
        assert!(is_empty);
    }

    #[tokio::test]
    async fn test_basic_operations() {
        let (cache, _temp_dir) = create_test_cache().await;

        let key = "test_key".to_string();
        let value = Bytes::from("test_value");

        // 设置
        cache.set(key.clone(), value.clone()).await.unwrap();

        // 获取
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);

        // 检查存在
        assert!(cache.contains_key(&key).await.unwrap());

        // 删除
        let deleted = cache.delete(&key).await.unwrap();
        assert!(deleted);
        assert!(!cache.contains_key(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_ttl_operations() {
        let (cache, _temp_dir) = create_test_cache().await;

        let key = "ttl_key".to_string();
        let value = Bytes::from("ttl_value");

        // 设置带 TTL
        cache.set_with_ttl(key.clone(), value.clone(), 2).await.unwrap();

        // 立即获取应该成功
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_some());

        // 检查 TTL
        let ttl = cache.ttl_manager.get_ttl(&key).await;
        assert!(ttl.is_some());

        // 等待过期
        tokio::time::sleep(Duration::from_millis(2100)).await;

        // 应该已过期
        let retrieved = cache.get(&key).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_cache_options() {
        let (cache, _temp_dir) = create_test_cache().await;

        let key = "options_key".to_string();
        let value = Bytes::from("options_value");

        // 强制写入 L2
        let options = CacheOptionsMelange {
            force_l2: true,
            ..Default::default()
        };

        cache.set_with_options(key.clone(), value.clone(), &options).await.unwrap();

        // 跳过 L1 获取
        let get_options = CacheOptionsMelange {
            skip_l1: true,
            ..Default::default()
        };

        let retrieved = cache.get_with_options(&key, &get_options).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
    }

    #[tokio::test]
    async fn test_clear_and_stats() {
        let (cache, _temp_dir) = create_test_cache().await;

        // 添加一些数据
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = Bytes::from(format!("value_{}", i));
            cache.set(key, value).await.unwrap();
        }

        let len_before = cache.len().await.unwrap();
        assert!(len_before > 0);

        // 进行一些读取操作来生成统计信息
        for i in 0..5 {
            let key = format!("key_{}", i);
            let _ = cache.get(&key).await.unwrap();
        }

        // 获取统计信息
        #[cfg(feature = "melange-storage")]
        let l2_stats = cache.get_l2_stats().await;

        // 清空
        cache.clear().await.unwrap();

        let is_empty = cache.is_empty().await.unwrap();
        assert!(is_empty);
    }

    #[tokio::test]
    async fn test_shutdown() {
        let (cache, _temp_dir) = create_test_cache().await;

        // 添加一些数据
        cache.set("test".to_string(), Bytes::from("value")).await.unwrap();

        // 关闭
        cache.shutdown().await.unwrap();

        // 验证状态
        let running = cache.is_running.read().await;
        assert!(!*running);
    }

    #[tokio::test]
    async fn test_compression_algorithms() {
        let temp_dir = TempDir::new().unwrap();

        for compression in [CompressionAlgorithm::None, CompressionAlgorithm::Lz4] {
            let l2_config = crate::config::L2Config {
                enable_l2_cache: true,
                data_dir: Some(temp_dir.path().to_path_buf()),
                max_disk_size: 10 * 1024 * 1024,
                write_buffer_size: 1024 * 1024,
                max_write_buffer_number: 3,
                block_cache_size: 512 * 1024,
                enable_compression: true,
                compression_level: 6,
                background_threads: 2,
                clear_on_startup: false,
                database_engine: DatabaseEngine::MelangeDB,
                melange_config: MelangeSpecificConfig {
                    compression_algorithm: compression,
                    cache_size_mb: 256,
                    max_file_size_mb: 512,
                    enable_statistics: true,
                },
            };

            let logging_config = crate::config::LoggingConfig {
                level: "debug".to_string(),
                enable_colors: false,
                show_timestamp: false,
                enable_performance_logs: true,
                enable_audit_logs: false,
                enable_cache_logs: true,
            };

            let config = CacheConfig {
                l1: crate::config::L1Config {
                    max_memory: 16 * 1024 * 1024,
                    max_entries: 1000,
                    eviction_strategy: crate::types::EvictionStrategy::Lru,
                },
                l2: l2_config,
                compression: crate::config::CompressionConfig {
                    enable_lz4: true,
                    compression_threshold: 1024,
                    compression_level: 4,
                    auto_compression: true,
                    min_compression_ratio: 0.8,
                },
                ttl: crate::config::TtlConfig {
                    default_ttl: Some(60),
                    max_ttl: 3600,
                    cleanup_interval: 60,
                    max_cleanup_entries: 100,
                    lazy_expiration: true,
                    active_expiration: false,
                },
                performance: crate::config::PerformanceConfig {
                    worker_threads: 4,
                    enable_concurrency: true,
                    read_write_separation: true,
                    batch_size: 100,
                    enable_warmup: false,
                    stats_interval: 30,
                    enable_background_stats: true,
                    l2_write_strategy: "adaptive".to_string(),
                    l2_write_threshold: 4096,
                    l2_write_ttl_threshold: 300,
                },
                logging: logging_config,
            };

            let cache = RatMemCacheMelange::new(config).await.unwrap();

            let key = "compression_test";
            let value = Bytes::from("this is a test value for compression");

            cache.set(key.to_string(), value.clone()).await.unwrap();
            let retrieved = cache.get(key).await.unwrap();
            assert_eq!(retrieved, Some(value));

            cache.shutdown().await.unwrap();
        }
    }
}