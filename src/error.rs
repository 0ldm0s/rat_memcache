//! 错误类型定义
//!
//! 为 rat_memcache 提供统一的错误处理机制

use thiserror::Error;

/// 缓存操作错误类型
#[derive(Error, Debug)]
pub enum CacheError {
    /// 键不存在
    #[error("键 '{key}' 不存在")]
    KeyNotFound { key: String },

    /// 键已过期
    #[error("键 '{key}' 已过期")]
    KeyExpired { key: String },

    /// 序列化错误
    #[error("序列化失败: {message}")]
    SerializationError { message: String },

    /// 压缩错误
    #[error("LZ4 压缩/解压缩失败: {message}")]
    CompressionError { message: String },

  
    /// MelangeDB 错误
    #[cfg(feature = "melange-storage")]
    #[error("MelangeDB 操作失败: {message}")]
    MelangeDbError { message: String },


    /// 配置错误
    #[error("配置错误: {message}")]
    ConfigError { message: String },

    /// 内存不足
    #[error("内存不足，无法分配 {requested_size} 字节")]
    OutOfMemory { requested_size: usize },

    /// 缓存容量已满
    #[error("缓存容量已满，当前大小: {current_size}, 最大容量: {max_capacity}")]
    CacheFull {
        current_size: usize,
        max_capacity: usize,
    },

    /// 无效的 TTL 值
    #[error("无效的 TTL 值: {ttl_seconds} 秒")]
    InvalidTtl { ttl_seconds: i64 },

    /// 并发访问冲突
    #[error("并发访问冲突，键: '{key}'")]
    ConcurrencyConflict { key: String },

    /// IO 错误
    #[error("IO 操作失败: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },

    /// 其他错误
    #[error("未知错误: {message}")]
    Other { message: String },
}

/// 缓存操作结果类型
pub type CacheResult<T> = Result<T, CacheError>;

/// 从字符串创建压缩错误的便捷函数
impl CacheError {
    /// 创建压缩错误
    pub fn compression_error(message: impl Into<String>) -> Self {
        Self::CompressionError {
            message: message.into(),
        }
    }

    /// 创建配置错误
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError {
            message: message.into(),
        }
    }

    /// 创建其他错误
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
    }

    /// 创建键不存在错误
    pub fn key_not_found(key: impl Into<String>) -> Self {
        Self::KeyNotFound {
            key: key.into(),
        }
    }

    /// 创建键过期错误
    pub fn key_expired(key: impl Into<String>) -> Self {
        Self::KeyExpired {
            key: key.into(),
        }
    }

    /// 创建序列化错误
    pub fn serialization_error(message: impl Into<String>) -> Self {
        Self::Other {
            message: format!("序列化错误: {}", message.into()),
        }
    }

    
    /// 创建 MelangeDB 错误
    #[cfg(feature = "melange-storage")]
    pub fn melange_db_error(message: impl Into<String>) -> Self {
        Self::MelangeDbError {
            message: message.into(),
        }
    }

    /// 创建数据库错误
    pub fn database_error(message: impl Into<String>) -> Self {
        Self::Other {
            message: format!("数据库错误: {}", message.into()),
        }
    }


    /// 创建内存不足错误
    pub fn out_of_memory(requested_size: usize) -> Self {
        Self::OutOfMemory {
            requested_size,
        }
    }

    /// 创建缓存已满错误
    pub fn cache_full(current_size: usize, max_capacity: usize) -> Self {
        Self::CacheFull {
            current_size,
            max_capacity,
        }
    }

    /// 创建无效 TTL 错误
    pub fn invalid_ttl(ttl_seconds: i64) -> Self {
        Self::InvalidTtl {
            ttl_seconds,
        }
    }

    /// 创建并发冲突错误
    pub fn concurrency_conflict(key: impl Into<String>) -> Self {
        Self::ConcurrencyConflict {
            key: key.into(),
        }
    }

    /// 创建 IO 错误
    pub fn io_error(message: impl Into<String>) -> Self {
        Self::Other {
            message: format!("IO错误: {}", message.into()),
        }
    }

    /// 检查是否为键不存在错误
    pub fn is_key_not_found(&self) -> bool {
        matches!(self, CacheError::KeyNotFound { .. })
    }

    /// 检查是否为键过期错误
    pub fn is_key_expired(&self) -> bool {
        matches!(self, CacheError::KeyExpired { .. })
    }

    /// 检查是否为缓存已满错误
    pub fn is_cache_full(&self) -> bool {
        matches!(self, CacheError::CacheFull { .. })
    }
}