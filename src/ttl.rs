//! TTL (Time To Live) 管理模块
//!
//! 提供过期时间管理、惰性过期和主动过期清理功能

use crate::config::TtlConfig;
use crate::error::{CacheError, CacheResult};
use crate::types::current_timestamp;
use crate::config::LoggingConfig;
use crate::{ttl_log, perf_log};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use tokio::time::interval;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// TTL 管理器
#[derive(Debug)]
pub struct TtlManager {
    config: Arc<TtlConfig>,
    logging_config: Arc<LoggingConfig>,
    /// 按过期时间排序的键索引 (expire_time -> keys)
    expiry_index: Arc<RwLock<BTreeMap<u64, HashSet<String>>>>,
    /// 键到过期时间的映射 (key -> expire_time)
    key_expiry: Arc<RwLock<std::collections::HashMap<String, u64>>>,
    /// 清理任务通道
    cleanup_sender: UnboundedSender<CleanupCommand>,
    /// 统计信息
    stats: Arc<Mutex<TtlStats>>,
}

/// 清理命令
#[derive(Debug, Clone)]
enum CleanupCommand {
    /// 添加键的过期时间
    AddKey { key: String, expire_time: u64 },
    /// 移除键
    RemoveKey { key: String },
    /// 更新键的过期时间
    UpdateKey { key: String, expire_time: u64 },
    /// 强制清理过期键
    ForceCleanup,
    /// 停止清理任务
    Stop,
}

/// TTL 统计信息
#[derive(Debug, Clone, Default)]
pub struct TtlStats {
    /// 总过期键数
    pub total_expired: u64,
    /// 惰性过期数
    pub lazy_expired: u64,
    /// 主动过期数
    pub active_expired: u64,
    /// 清理任务执行次数
    pub cleanup_runs: u64,
    /// 平均清理时间（毫秒）
    pub avg_cleanup_time_ms: f64,
    /// 当前管理的键数量
    pub managed_keys: u64,
}

impl TtlManager {
    /// 创建新的 TTL 管理器
    pub async fn new(config: TtlConfig, logging_config: LoggingConfig) -> CacheResult<Self> {
        let (cleanup_sender, cleanup_receiver) = unbounded_channel();
        
        let manager = Self {
            config: Arc::new(config),
            logging_config: Arc::new(logging_config),
            expiry_index: Arc::new(RwLock::new(BTreeMap::new())),
            key_expiry: Arc::new(RwLock::new(std::collections::HashMap::new())),
            cleanup_sender,
            stats: Arc::new(Mutex::new(TtlStats::default())),
        };

        // 启动清理任务
        if manager.config.active_expiration {
            manager.start_cleanup_task(cleanup_receiver).await;
        }

        ttl_log!(&manager.logging_config, info, "TTL 管理器已启动");
        Ok(manager)
    }

    /// 添加键的过期时间
    pub async fn add_key(&self, key: String, _ttl_seconds: Option<u64>) -> CacheResult<u64> {
        let expire_time = if let Some(expire) = self.config.expire_seconds {
            // 使用配置中设置的过期时间
            if expire == 0 {
                // 配置为0表示永不过期
                return Ok(0);
            }
            current_timestamp() + expire
        } else {
            // 配置中没有设置过期时间，永不过期
            return Ok(0);
        };

        // 更新索引
        self.update_key_expiry(key.clone(), expire_time).await;
        
        // 发送清理命令
        if let Err(e) = self.cleanup_sender.send(CleanupCommand::AddKey {
            key: key.clone(),
            expire_time,
        }) {
            ttl_log!(&self.logging_config, warn, "发送清理命令失败: {}", e);
        }

        ttl_log!(&self.logging_config, debug, "添加键 {} 的过期时间: {}", key, expire_time);
        Ok(expire_time)
    }

    /// 移除键的过期时间
    pub async fn remove_key(&self, key: &str) {
        self.remove_key_expiry(key).await;
        
        if let Err(e) = self.cleanup_sender.send(CleanupCommand::RemoveKey {
            key: key.to_string(),
        }) {
            ttl_log!(&self.logging_config, warn, "发送移除命令失败: {}", e);
        }

        ttl_log!(&self.logging_config, debug, "移除键 {} 的过期时间", key);
    }

    /// 更新键的过期时间
    pub async fn update_key(&self, key: String, ttl_seconds: Option<u64>) -> CacheResult<u64> {
        // 先移除旧的过期时间
        self.remove_key_expiry(&key).await;
        
        // 添加新的过期时间
        self.add_key(key, ttl_seconds).await
    }

    /// 检查键是否过期（惰性过期）
    pub async fn is_expired(&self, key: &str) -> bool {
        if !self.config.lazy_expiration {
            return false;
        }

        let key_expiry = self.key_expiry.read().await;
        if let Some(&expire_time) = key_expiry.get(key) {
            if expire_time > 0 && current_timestamp() >= expire_time {
                drop(key_expiry);
                
                // 记录惰性过期
                let mut stats = self.stats.lock().await;
                stats.lazy_expired += 1;
                stats.total_expired += 1;
                drop(stats);
                
                ttl_log!(&self.logging_config, debug, "键 {} 已过期（惰性检查）", key);
                return true;
            }
        }
        false
    }

    /// 获取键的剩余 TTL（秒）
    pub async fn get_ttl(&self, key: &str) -> Option<u64> {
        let key_expiry = self.key_expiry.read().await;
        if let Some(&expire_time) = key_expiry.get(key) {
            if expire_time == 0 {
                // 永不过期
                return None;
            }
            
            let current = current_timestamp();
            if current >= expire_time {
                // 已过期
                return Some(0);
            }
            
            return Some(expire_time - current);
        }
        None
    }

    /// 获取所有过期的键
    pub async fn get_expired_keys(&self, limit: usize) -> Vec<String> {
        let current_time = current_timestamp();
        let expiry_index = self.expiry_index.read().await;
        
        let mut expired_keys = Vec::new();
        
        for (&expire_time, keys) in expiry_index.iter() {
            if expire_time > current_time {
                break; // BTreeMap 是有序的，后面的都没过期
            }
            
            for key in keys {
                if expired_keys.len() >= limit {
                    return expired_keys;
                }
                expired_keys.push(key.clone());
            }
        }
        
        expired_keys
    }

    /// 强制清理过期键
    pub async fn force_cleanup(&self) {
        if let Err(e) = self.cleanup_sender.send(CleanupCommand::ForceCleanup) {
            ttl_log!(&self.logging_config, warn, "发送强制清理命令失败: {}", e);
        }
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> TtlStats {
        let mut stats = self.stats.lock().await;
        
        // 更新当前管理的键数量
        let key_expiry = self.key_expiry.read().await;
        stats.managed_keys = key_expiry.len() as u64;
        drop(key_expiry);
        
        stats.clone()
    }

    /// 重置统计信息
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.lock().await;
        *stats = TtlStats::default();
        ttl_log!(&self.logging_config, info, "TTL 统计信息已重置");
    }

    /// 停止 TTL 管理器
    pub async fn stop(&self) {
        if let Err(e) = self.cleanup_sender.send(CleanupCommand::Stop) {
            ttl_log!(&self.logging_config, warn, "发送停止命令失败: {}", e);
        }
        ttl_log!(&self.logging_config, info, "TTL 管理器已停止");
    }

    /// 更新键的过期时间索引
    async fn update_key_expiry(&self, key: String, expire_time: u64) {
        // 移除旧的索引
        self.remove_key_expiry(&key).await;
        
        // 添加新的索引
        let mut expiry_index = self.expiry_index.write().await;
        let mut key_expiry = self.key_expiry.write().await;
        
        expiry_index.entry(expire_time)
            .or_insert_with(HashSet::new)
            .insert(key.clone());
        
        key_expiry.insert(key, expire_time);
    }

    /// 移除键的过期时间索引
    async fn remove_key_expiry(&self, key: &str) {
        let mut key_expiry = self.key_expiry.write().await;
        
        if let Some(old_expire_time) = key_expiry.remove(key) {
            drop(key_expiry);
            
            let mut expiry_index = self.expiry_index.write().await;
            if let Some(keys) = expiry_index.get_mut(&old_expire_time) {
                keys.remove(key);
                if keys.is_empty() {
                    expiry_index.remove(&old_expire_time);
                }
            }
        }
    }

    /// 启动清理任务
    async fn start_cleanup_task(&self, mut cleanup_receiver: UnboundedReceiver<CleanupCommand>) {
        let config = Arc::clone(&self.config);
        let logging_config = Arc::clone(&self.logging_config);
        let expiry_index = Arc::clone(&self.expiry_index);
        let key_expiry = Arc::clone(&self.key_expiry);
        let stats = Arc::clone(&self.stats);
        
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(config.cleanup_interval));
            
            ttl_log!(logging_config, info, "TTL 清理任务已启动，间隔: {}秒", config.cleanup_interval);
            
            loop {
                tokio::select! {
                    // 定时清理
                    _ = cleanup_interval.tick() => {
                        Self::perform_cleanup(
                            &config,
                            &logging_config,
                            &expiry_index,
                            &key_expiry,
                            &stats,
                        ).await;
                    }
                    
                    // 处理清理命令
                    command = cleanup_receiver.recv() => {
                        match command {
                            Some(CleanupCommand::ForceCleanup) => {
                                Self::perform_cleanup(
                                    &config,
                                    &logging_config,
                                    &expiry_index,
                                    &key_expiry,
                                    &stats,
                                ).await;
                            }
                            Some(CleanupCommand::Stop) => {
                                ttl_log!(logging_config, info, "TTL 清理任务已停止");
                                break;
                            }
                            Some(_) => {
                                // 其他命令暂时忽略，因为索引更新在主线程中处理
                            }
                            None => {
                                ttl_log!(logging_config, warn, "清理命令通道已关闭");
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    /// 执行清理操作
    async fn perform_cleanup(
        config: &TtlConfig,
        logging_config: &LoggingConfig,
        expiry_index: &Arc<RwLock<BTreeMap<u64, HashSet<String>>>>,
        key_expiry: &Arc<RwLock<std::collections::HashMap<String, u64>>>,
        stats: &Arc<Mutex<TtlStats>>,
    ) {
        let start_time = Instant::now();
        let current_time = current_timestamp();
        
        ttl_log!(logging_config, debug, "开始 TTL 清理任务");
        
        let mut expired_keys = Vec::new();
        
        // 收集过期的键
        {
            let expiry_index_guard = expiry_index.read().await;
            
            for (&expire_time, keys) in expiry_index_guard.iter() {
                if expire_time > current_time {
                    break; // 后面的都没过期
                }
                
                for key in keys {
                    if expired_keys.len() >= config.max_cleanup_entries {
                        break;
                    }
                    expired_keys.push(key.clone());
                }
                
                if expired_keys.len() >= config.max_cleanup_entries {
                    break;
                }
            }
        }
        
        // 清理过期的键
        if !expired_keys.is_empty() {
            let mut expiry_index_guard = expiry_index.write().await;
            let mut key_expiry_guard = key_expiry.write().await;
            
            for key in &expired_keys {
                if let Some(expire_time) = key_expiry_guard.remove(key) {
                    if let Some(keys) = expiry_index_guard.get_mut(&expire_time) {
                        keys.remove(key);
                        if keys.is_empty() {
                            expiry_index_guard.remove(&expire_time);
                        }
                    }
                }
            }
        }
        
        // 更新统计信息
        let cleanup_duration = start_time.elapsed();
        let mut stats_guard = stats.lock().await;
        stats_guard.cleanup_runs += 1;
        stats_guard.active_expired += expired_keys.len() as u64;
        stats_guard.total_expired += expired_keys.len() as u64;
        
        // 更新平均清理时间
        let new_time_ms = cleanup_duration.as_millis() as f64;
        if stats_guard.avg_cleanup_time_ms == 0.0 {
            stats_guard.avg_cleanup_time_ms = new_time_ms;
        } else {
            stats_guard.avg_cleanup_time_ms = 
                (stats_guard.avg_cleanup_time_ms * 0.9) + (new_time_ms * 0.1);
        }
        
        drop(stats_guard);
        
        if !expired_keys.is_empty() {
            ttl_log!(logging_config, info, 
                "TTL 清理完成: 清理了 {} 个过期键，耗时 {:.2}ms",
                expired_keys.len(), cleanup_duration.as_millis()
            );
        } else {
            ttl_log!(logging_config, debug, 
                "TTL 清理完成: 无过期键，耗时 {:.2}ms",
                cleanup_duration.as_millis()
            );
        }
        
        perf_log!(logging_config, debug, 
            "TTL cleanup performance: {} keys cleaned in {:.2}ms",
            expired_keys.len(), cleanup_duration.as_millis()
        );
    }
}

impl Drop for TtlManager {
    fn drop(&mut self) {
        // 在销毁时尝试停止清理任务
        if self.config.active_expiration {
            // 忽略发送错误，因为清理任务可能已经停止
            let _ = self.cleanup_sender.send(CleanupCommand::Stop);
        }
    }
}

/// TTL 辅助函数
pub mod utils {
    use super::*;

    /// 计算 TTL 到期时间
    pub fn calculate_expire_time(ttl_seconds: u64) -> u64 {
        current_timestamp() + ttl_seconds
    }

    /// 检查过期时间是否有效
    pub fn is_valid_expire_time(expire_time: u64) -> bool {
        expire_time == 0 || expire_time > current_timestamp()
    }

    /// 格式化剩余时间
    pub fn format_remaining_time(ttl_seconds: u64) -> String {
        if ttl_seconds == 0 {
            return "已过期".to_string();
        }
        
        let days = ttl_seconds / 86400;
        let hours = (ttl_seconds % 86400) / 3600;
        let minutes = (ttl_seconds % 3600) / 60;
        let seconds = ttl_seconds % 60;
        
        if days > 0 {
            format!("{}天{}小时{}分钟{}秒", days, hours, minutes, seconds)
        } else if hours > 0 {
            format!("{}小时{}分钟{}秒", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}分钟{}秒", minutes, seconds)
        } else {
            format!("{}秒", seconds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TtlConfig, LoggingConfig};
    use tokio::time::{sleep, Duration};

    fn create_test_config() -> (TtlConfig, LoggingConfig) {
        let ttl_config = TtlConfig {
            expire_seconds: Some(60),
            cleanup_interval: 1,
            max_cleanup_entries: 100,
            lazy_expiration: true,
            active_expiration: true,
        };

        let logging_config = LoggingConfig {
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
        };

        (ttl_config, logging_config)
    }

    #[tokio::test]
    async fn test_ttl_manager_creation() {
        let (ttl_config, logging_config) = create_test_config();
        let manager = TtlManager::new(ttl_config, logging_config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_add_and_get_ttl() {
        let (ttl_config, logging_config) = create_test_config();
        let manager = TtlManager::new(ttl_config, logging_config).await.unwrap();
        
        let expire_time = manager.add_key("test_key".to_string(), Some(30)).await.unwrap();
        assert!(expire_time > current_timestamp());
        
        let ttl = manager.get_ttl("test_key").await;
        assert!(ttl.is_some());
        assert!(ttl.unwrap() <= 30);
    }

    #[tokio::test]
    async fn test_key_expiration() {
        let (mut ttl_config, logging_config) = create_test_config();
        ttl_config.cleanup_interval = 1; // 1秒清理间隔
        
        let manager = TtlManager::new(ttl_config, logging_config).await.unwrap();
        
        // 添加一个很短的 TTL
        manager.add_key("short_ttl_key".to_string(), Some(1)).await.unwrap();

        // 等待过期 - 增加等待时间确保过期
        sleep(Duration::from_millis(2500)).await; // 等待2.5秒

        // 现在应该过期了
        let ttl = manager.get_ttl("short_ttl_key").await;
        let is_expired = manager.is_expired("short_ttl_key").await;

        // 如果键已被清理（get_ttl返回None），那么它确实已经过期了
        // 如果键仍在TTL管理器中，那么is_expired应该返回true
        let actually_expired = ttl.is_none() || is_expired;

        assert!(actually_expired, "键应该在2.5秒后过期，TTL: {:?}, is_expired: {}", ttl, is_expired);
    }

    #[tokio::test]
    async fn test_remove_key() {
        let (ttl_config, logging_config) = create_test_config();
        let manager = TtlManager::new(ttl_config, logging_config).await.unwrap();
        
        manager.add_key("test_key".to_string(), Some(60)).await.unwrap();
        assert!(manager.get_ttl("test_key").await.is_some());
        
        manager.remove_key("test_key").await;
        assert!(manager.get_ttl("test_key").await.is_none());
    }

    #[tokio::test]
    async fn test_update_key() {
        let (ttl_config, logging_config) = create_test_config();
        let manager = TtlManager::new(ttl_config, logging_config).await.unwrap();
        
        manager.add_key("test_key".to_string(), Some(60)).await.unwrap();
        let old_ttl = manager.get_ttl("test_key").await.unwrap();
        
        manager.update_key("test_key".to_string(), Some(120)).await.unwrap();
        let new_ttl = manager.get_ttl("test_key").await.unwrap();
        
        assert!(new_ttl > old_ttl);
    }

    #[test]
    fn test_format_remaining_time() {
        assert_eq!(utils::format_remaining_time(0), "已过期");
        assert_eq!(utils::format_remaining_time(30), "30秒");
        assert_eq!(utils::format_remaining_time(90), "1分钟30秒");
        assert_eq!(utils::format_remaining_time(3661), "1小时1分钟1秒");
        assert_eq!(utils::format_remaining_time(90061), "1天1小时1分钟1秒");
    }
}