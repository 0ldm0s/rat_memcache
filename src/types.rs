//! 核心数据类型定义
//!
//! 定义缓存系统中使用的核心数据结构

use chrono;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 缓存值包装器，包含数据和元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheValue {
    /// 实际存储的数据
    pub data: Vec<u8>,
    /// 创建时间戳（Unix 时间戳，秒）
    pub created_at: u64,
    /// 最后访问时间戳（Unix 时间戳，秒）
    pub last_accessed: u64,
    /// 过期时间戳（Unix 时间戳，秒），None 表示永不过期
    pub expires_at: Option<u64>,
    /// 访问次数（用于 LFU 策略）
    pub access_count: u64,
    /// 数据是否已压缩
    pub is_compressed: bool,
    /// 原始数据大小（压缩前）
    pub original_size: usize,
    /// 压缩后大小
    pub compressed_size: usize,
}

impl CacheValue {
    /// 创建新的缓存值
    pub fn new(data: Vec<u8>, compressed: bool, original_size: usize) -> Self {
        let size = data.len();
        
        Self {
            data,
            created_at: current_timestamp(),
            last_accessed: current_timestamp(),
            expires_at: None,
            access_count: 1,
            is_compressed: compressed,
            original_size,
            compressed_size: size,
        }
    }

    /// 创建未压缩的缓存值
    pub fn new_uncompressed(data: Vec<u8>) -> Self {
        let size = data.len();
        Self::new(data, false, size)
    }

    /// 创建压缩的缓存值
    pub fn new_compressed(
        compressed_data: Vec<u8>,
        original_size: usize,
    ) -> Self {
        Self::new(compressed_data, true, original_size)
    }

    /// 检查值是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            current_timestamp() > expires_at
        } else {
            false
        }
    }

    /// 更新最后访问时间并增加访问计数
    pub fn touch(&mut self) {
        self.last_accessed = current_timestamp();
        self.access_count += 1;
    }

    /// 获取剩余 TTL（秒）
    pub fn remaining_ttl(&self) -> Option<u64> {
        self.expires_at.map(|expires_at| {
            let now = current_timestamp();
            if expires_at > now {
                expires_at - now
            } else {
                0
            }
        })
    }

    /// 获取年龄（从创建到现在的秒数）
    pub fn age(&self) -> u64 {
        current_timestamp() - self.created_at
    }

    /// 获取空闲时间（从最后访问到现在的秒数）
    pub fn idle_time(&self) -> u64 {
        current_timestamp() - self.last_accessed
    }

    /// 计算压缩比率
    pub fn compression_ratio(&self) -> f64 {
        if self.original_size == 0 {
            1.0
        } else {
            self.compressed_size as f64 / self.original_size as f64
        }
    }

    /// 获取数据大小
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// 获取实际占用的内存大小
    pub fn memory_size(&self) -> usize {
        self.data.len() + std::mem::size_of::<Self>()
    }
}

/// 缓存策略枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionStrategy {
    /// 最近最少使用（Least Recently Used）
    Lru,
    /// 最不经常使用（Least Frequently Used）
    Lfu,
    /// 先进先出（First In First Out）
    Fifo,
    /// 最近最少使用 + 最不经常使用混合策略
    LruLfu,
    /// 基于 TTL 的策略
    TtlBased,
}

/// 缓存层级枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLayer {
    /// L1 内存缓存
    Memory,
    /// L2 MelangeDB 持久化缓存
    Persistent,
}

/// 缓存操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOperation {
    /// 读取操作
    Get,
    /// 写入操作
    Set,
    /// 删除操作
    Delete,
    /// 清空操作
    Clear,
    /// 过期清理操作
    Expire,
}



/// 获取当前 Unix 时间戳（秒）
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 将 Duration 转换为秒数
pub fn duration_to_seconds(duration: Duration) -> u64 {
    duration.as_secs()
}

/// 从秒数创建 Duration
pub fn seconds_to_duration(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}