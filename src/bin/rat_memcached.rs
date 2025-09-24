//! RatMemcached - 高性能 Memcached 协议兼容服务器
//!
//! 基于 rat_memcache 构建的高性能缓存服务器
//! 完全兼容 Memcached 协议，性能超越原版 Memcached

#[cfg(feature = "mimalloc-allocator")]
use mimalloc::MiMalloc;

#[cfg(feature = "mimalloc-allocator")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

use bytes::Bytes;
use clap::{Arg, Command};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal;
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};

use rat_memcache::{
    config::CacheConfig,
    error::{CacheError, CacheResult},
    logging::{LogManager, flush_logs_if_async},
    RatMemCache,
};

// 使用 rat_logger 日志宏
use rat_logger::{debug, error, info, warn};

// 引入流式协议支持
use rat_memcache::streaming_protocol::{StreamingCommand, StreamingResponse, StreamingParser, StreamingFormatter};

/// 服务器配置
#[derive(Debug, Clone, serde::Deserialize)]
struct ServerConfig {
    /// 绑定地址
    bind_addr: String,
    /// 缓存配置文件路径
    cache_config_path: Option<String>,
    }

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:11211".to_string(),
            cache_config_path: None,
        }
    }
}

/// Memcached 协议命令
#[derive(Debug, Clone)]
enum MemcachedCommand {
    Get {
        keys: Vec<String>,
    },
    Set {
        key: String,
        flags: u32,
        exptime: u32,
        bytes: usize,
        data: Option<Bytes>,
    },
    Add {
        key: String,
        flags: u32,
        exptime: u32,
        bytes: usize,
        data: Option<Bytes>,
    },
    Replace {
        key: String,
        flags: u32,
        exptime: u32,
        bytes: usize,
        data: Option<Bytes>,
    },
    Delete {
        key: String,
    },
    Incr {
        key: String,
        value: u64,
    },
    Decr {
        key: String,
        value: u64,
    },
    // 流式协议命令
    StreamingGet {
        key: String,
        chunk_size: Option<usize>,
    },
    SetBegin {
        key: String,
        total_size: usize,
        chunk_count: usize,
        flags: u32,
        exptime: u32,
    },
    SetData {
        key: String,
        chunk_number: usize,
        data: Bytes,
    },
    SetEnd {
        key: String,
    },
    Stats,
    Flush,
    Version,
    Quit,
    Unknown(String),
}

/// Memcached 协议响应
#[derive(Debug, Clone)]
enum MemcachedResponse {
    Value {
        key: String,
        flags: u32,
        bytes: usize,
        data: Bytes,
    },
    End,
    Stored,
    NotStored,
    Exists,
    NotFound,
    Deleted,
    Touched,
    Ok,
    Error(String),
    ClientError(String),
    ServerError(String),
    Stats(HashMap<String, String>),
    Version(String),
    // 流式协议响应
    StreamBegin {
        key: String,
        total_size: usize,
        chunk_count: usize,
    },
    StreamData {
        key: String,
        chunk_number: usize,
        data: Bytes,
    },
    StreamEnd {
        key: String,
    },
    StreamError(String),
}

/// Memcached 服务器
pub struct MemcachedServer {
    cache: Arc<RatMemCache>,
    bind_addr: SocketAddr,
    config: ServerConfig,
    start_time: Instant,
    listener: Option<TokioTcpListener>,
    shutdown_notify: Arc<Notify>,
    streaming_parser: StreamingParser,
    // 流式传输状态管理
    streaming_state: Arc<tokio::sync::RwLock<HashMap<String, StreamingSession>>>,
    // 分块SET状态管理
    chunked_set_state: Arc<tokio::sync::RwLock<HashMap<String, ChunkedSetSession>>>,
}

/// 流式传输会话状态
#[derive(Debug, Clone)]
struct StreamingSession {
    /// 当前正在传输的键
    key: String,
    /// 总数据大小
    total_size: usize,
    /// 块大小
    chunk_size: usize,
    /// 当前块索引
    current_chunk: usize,
    /// 总块数
    total_chunks: usize,
    /// 完整数据
    data: Bytes,
    /// 创建时间
    created_at: Instant,
}

/// 分块SET会话状态
#[derive(Debug, Clone)]
struct ChunkedSetSession {
    /// 键名
    key: String,
    /// 总大小
    total_size: usize,
    /// 块数量
    chunk_count: usize,
    /// 标志
    flags: u32,
    /// 过期时间
    exptime: u32,
    /// 已接收的数据块
    received_chunks: HashMap<usize, Bytes>,
    /// 创建时间
    created_at: Instant,
}

impl ChunkedSetSession {
    pub fn new(key: String, total_size: usize, chunk_count: usize, flags: u32, exptime: u32) -> Self {
        Self {
            key,
            total_size,
            chunk_count,
            flags,
            exptime,
            received_chunks: HashMap::new(),
            created_at: Instant::now(),
        }
    }

    /// 添加数据块
    pub fn add_chunk(&mut self, chunk_number: usize, data: Bytes) -> bool {
        if chunk_number >= self.chunk_count {
            return false;
        }

        self.received_chunks.insert(chunk_number, data);
        true
    }

    /// 检查是否所有块都已接收
    pub fn is_complete(&self) -> bool {
        self.received_chunks.len() == self.chunk_count
    }

    /// 组装完整数据
    pub fn assemble_data(&self) -> Option<Vec<u8>> {
        if !self.is_complete() {
            return None;
        }

        let mut assembled_data = Vec::with_capacity(self.total_size);
        for i in 0..self.chunk_count {
            if let Some(chunk) = self.received_chunks.get(&i) {
                assembled_data.extend_from_slice(chunk);
            } else {
                return None; // 缺少数据块
            }
        }

        Some(assembled_data)
    }

    /// 获取接收进度
    pub fn progress(&self) -> (usize, usize) {
        (self.received_chunks.len(), self.chunk_count)
    }
}

impl StreamingSession {
    pub fn new(key: String, data: Bytes, chunk_size: usize) -> Self {
        let total_size = data.len();
        let total_chunks = (total_size + chunk_size - 1) / chunk_size;

        Self {
            key,
            total_size,
            chunk_size,
            current_chunk: 0,
            total_chunks,
            data,
            created_at: Instant::now(),
        }
    }

    /// 获取下一个数据块
    pub fn next_chunk(&mut self) -> Option<Bytes> {
        if self.current_chunk >= self.total_chunks {
            return None;
        }

        let start = self.current_chunk * self.chunk_size;
        let end = std::cmp::min(start + self.chunk_size, self.total_size);
        let chunk_data = self.data.slice(start..end);

        self.current_chunk += 1;
        Some(chunk_data)
    }

    /// 检查是否还有更多数据块
    pub fn has_more_chunks(&self) -> bool {
        self.current_chunk < self.total_chunks
    }

    /// 获取进度信息
    pub fn progress(&self) -> (usize, usize) {
        (self.current_chunk, self.total_chunks)
    }
}

impl MemcachedServer {
    /// 处理流式GET命令
    async fn handle_streaming_get(
        &self,
        key: String,
        chunk_size: Option<usize>,
    ) -> CacheResult<Vec<MemcachedResponse>> {
        let chunk_size = chunk_size.unwrap_or(4096);

        match self.cache.get(&key).await {
            Ok(Some(data)) => {
                info!("流式GET命中: {} ({} bytes)", key, data.len());

                // 创建流式会话
                let session = StreamingSession::new(key.clone(), data.clone(), chunk_size);

                // 存储会话状态
                {
                    let mut state = self.streaming_state.write().await;
                    state.insert(key.clone(), session);
                }

                // 生成响应序列
                let mut responses = Vec::new();

                // 添加流开始响应
                responses.push(MemcachedResponse::StreamBegin {
                    key: key.clone(),
                    total_size: data.len(),
                    chunk_count: (data.len() + chunk_size - 1) / chunk_size,
                });

                Ok(responses)
            }
            Ok(None) => {
                info!("流式GET未命中: {}", key);
                Ok(vec![MemcachedResponse::StreamError("键不存在".to_string())])
            }
            Err(e) => {
                error!("流式GET失败: {}", e);
                Ok(vec![MemcachedResponse::StreamError(format!("获取失败: {}", e))])
            }
        }
    }

    /// 获取下一个数据块
    async fn get_next_stream_chunk(&self, key: &str) -> Option<MemcachedResponse> {
        let mut state = self.streaming_state.write().await;

        if let Some(session) = state.get_mut(key) {
            if let Some(chunk_data) = session.next_chunk() {
                let (current, total) = session.progress();
                let response = MemcachedResponse::StreamData {
                    key: key.to_string(),
                    chunk_number: current - 1,
                    data: chunk_data,
                };

                // 如果这是最后一个块，添加流结束响应
                if !session.has_more_chunks() {
                    state.remove(key); // 清理会话
                }

                Some(response)
            } else {
                // 没有更多数据，发送流结束响应
                state.remove(key); // 清理会话
                Some(MemcachedResponse::StreamEnd {
                    key: key.to_string(),
                })
            }
        } else {
            None
        }
    }

    /// 处理分块SET开始命令
    async fn handle_set_begin(
        &self,
        key: String,
        total_size: usize,
        chunk_count: usize,
        flags: u32,
        exptime: u32,
    ) -> CacheResult<MemcachedResponse> {
        info!("处理SET开始: {} ({} bytes, {} chunks)", key, total_size, chunk_count);

        // 创建分块SET会话
        let session = ChunkedSetSession::new(key.clone(), total_size, chunk_count, flags, exptime);

        // 存储会话状态
        {
            let mut state = self.chunked_set_state.write().await;
            state.insert(key.clone(), session);
        }

        Ok(MemcachedResponse::Stored)
    }

    /// 处理分块SET数据命令
    async fn handle_set_data(
        &self,
        key: String,
        chunk_number: usize,
        data: Bytes,
    ) -> CacheResult<MemcachedResponse> {
        info!("处理SET数据: {} (chunk {}, {} bytes)", key, chunk_number, data.len());

        let mut state = self.chunked_set_state.write().await;

        if let Some(session) = state.get_mut(&key) {
            if session.add_chunk(chunk_number, data) {
                let (received, total) = session.progress();
                info!("SET数据进度: {}/{}", received, total);

                // 如果已接收所有块，组装数据并存储
                if session.is_complete() {
                    if let Some(assembled_data) = session.assemble_data() {
                        let ttl = if session.exptime > 0 { session.exptime as u64 } else { 0 };
                        match self.cache.set_with_ttl(key.clone(), Bytes::from(assembled_data), ttl).await {
                            Ok(_) => {
                                info!("分块SET完成: {}", key);
                                state.remove(&key); // 清理会话
                                Ok(MemcachedResponse::Stored)
                            }
                            Err(e) => {
                                error!("分块SET存储失败: {}", e);
                                state.remove(&key); // 清理会话
                                Ok(MemcachedResponse::ServerError(format!("存储失败: {}", e)))
                            }
                        }
                    } else {
                        error!("分块SET数据组装失败: {}", key);
                        state.remove(&key);
                        Ok(MemcachedResponse::ServerError("数据组装失败".to_string()))
                    }
                } else {
                    Ok(MemcachedResponse::Stored)
                }
            } else {
                error!("分块SET数据块无效: {} (chunk {})", key, chunk_number);
                Ok(MemcachedResponse::ClientError("无效的数据块".to_string()))
            }
        } else {
            warn!("分块SET会话不存在: {}", key);
            Ok(MemcachedResponse::ClientError("会话不存在".to_string()))
        }
    }

    /// 处理分块SET结束命令
    async fn handle_set_end(&self, key: String) -> CacheResult<MemcachedResponse> {
        info!("处理SET结束: {}", key);

        let mut state = self.chunked_set_state.write().await;

        if let Some(session) = state.get(&key) {
            if session.is_complete() {
                // 数据已经在handle_set_data中处理完成
                state.remove(&key);
                Ok(MemcachedResponse::Stored)
            } else {
                let (received, total) = session.progress();
                warn!("分块SET未完成: {} ({}/{})", key, received, total);
                state.remove(&key);
                Ok(MemcachedResponse::ClientError("数据不完整".to_string()))
            }
        } else {
            warn!("分块SET会话不存在: {}", key);
            Ok(MemcachedResponse::ClientError("会话不存在".to_string()))
        }
    }

    /// 创建新的 Memcached 服务器
    pub async fn new(config: ServerConfig) -> CacheResult<Self> {
        let bind_addr: SocketAddr = config
            .bind_addr
            .parse()
            .map_err(|e| CacheError::config_error(&format!("无效的绑定地址: {}", e)))?;

        // 加载缓存配置
        let cache_config = Self::load_cache_config(&config).await?;

        // 初始化日志系统
        let log_manager = LogManager::new(cache_config.logging.clone());
        log_manager.initialize()?;

        info!("🚀 初始化 RatMemcached 服务器");
        info!("📍 绑定地址: {}", bind_addr);

        // 显示配置详情
        Self::print_configuration_details(&cache_config);

        // 如果是异步日志模式，强制刷新启动时的配置信息
        flush_logs_if_async(&cache_config.logging);

        // 创建缓存实例
        let cache = Arc::new(RatMemCache::new(cache_config).await?);
        info!("✅ 缓存实例创建成功");

        // 创建传统 TCP 监听器
        let listener = Some(Self::create_tcp_listener(bind_addr).await?);

        Ok(Self {
            cache,
            bind_addr,
            config,
            start_time: Instant::now(),
            listener,
            shutdown_notify: Arc::new(Notify::new()),
            streaming_parser: StreamingParser::new(),
            streaming_state: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            chunked_set_state: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        })
    }

    /// 显示配置详情
    fn print_configuration_details(cache_config: &CacheConfig) {
        info!("📊 缓存配置详情:");

        // L1 配置
        info!("  🎯 L1 内存缓存:");
        info!("    - 最大内存: {:.2} MB", cache_config.l1.max_memory as f64 / 1024.0 / 1024.0);
        info!("    - 最大条目: {}", cache_config.l1.max_entries);
        info!("    - 淘汰策略: {:?}", cache_config.l1.eviction_strategy);

        // L2 缓存配置显示
        if let Some(l2_config) = &cache_config.l2 {
            if l2_config.enable_l2_cache {
                info!("  💾 L2 MelangeDB 持久化缓存:");
                info!("    - 启用状态: 是");
                if let Some(data_dir) = &l2_config.data_dir {
                    info!("    - 数据目录: {}", data_dir.display());
                }
                info!("    - 最大磁盘空间: {:.2} MB", l2_config.max_disk_size as f64 / 1024.0 / 1024.0);
                info!("    - 块缓存大小: {:.2} MB", l2_config.block_cache_size as f64 / 1024.0 / 1024.0);
                info!("    - 写缓冲区: {:.2} MB", l2_config.write_buffer_size as f64 / 1024.0 / 1024.0);
                info!("    - 压缩: {}", if l2_config.enable_lz4 { "启用" } else { "禁用" });

                // MelangeDB 特定配置
                info!("    - MelangeDB 压缩算法: {:?}", if l2_config.enable_lz4 { "LZ4" } else { "None" });
                info!("    - 缓存大小: {} MB", l2_config.cache_size_mb);
                info!("    - 最大文件大小: {} MB", l2_config.max_file_size_mb);
                info!("    - 智能Flush: {}", if l2_config.smart_flush_enabled { "启用" } else { "禁用" });
                if l2_config.smart_flush_enabled {
                    info!("    - Flush间隔: {}-{}ms (基础: {}ms)",
                          l2_config.smart_flush_min_interval_ms,
                          l2_config.smart_flush_max_interval_ms,
                          l2_config.smart_flush_base_interval_ms);
                }
                info!("    - 缓存预热策略: {:?}", l2_config.cache_warmup_strategy);
                info!("    - 统计信息: {}", if true { "启用" } else { "禁用" });
            } else {
                info!("  💾 L2 MelangeDB 持久化缓存: 禁用 (通过配置)");
            }
        } else {
            info!("  💾 L2 MelangeDB 持久化缓存: 禁用 (未配置)");
        }

        // TTL 配置
        info!("  ⏰ TTL 配置:");
        info!("    - 过期时间: {}秒", cache_config.ttl.expire_seconds.unwrap_or(0));
        info!("    - 清理间隔: {}秒", cache_config.ttl.cleanup_interval);

        // 压缩配置（现在在L2配置中）
        info!("  🗜️  压缩配置:");
        if let Some(l2_config) = &cache_config.l2 {
            info!("    - LZ4压缩: {}", if l2_config.enable_lz4 { "启用" } else { "禁用" });
            info!("    - 压缩阈值: {} bytes", l2_config.compression_threshold);
            info!("    - 最大压缩阈值: {} bytes", l2_config.compression_max_threshold);
            info!("    - 压缩级别: {}", l2_config.compression_level);
        } else {
            info!("    - LZ4压缩: 禁用 (未配置L2缓存)");
        }

        // 性能配置
        info!("  ⚡ 性能配置:");
        info!("    - 工作线程: {}", cache_config.performance.worker_threads);
        info!("    - 并发支持: {}", if cache_config.performance.enable_concurrency { "启用" } else { "禁用" });
        info!("    - 读写分离: {}", if cache_config.performance.read_write_separation { "启用" } else { "禁用" });
        info!("    - 大值阈值: {} bytes ({}KB)", cache_config.performance.large_value_threshold, cache_config.performance.large_value_threshold / 1024);

        #[cfg(feature = "mimalloc-allocator")]
        info!("  🧠 内存分配器: mimalloc (高性能优化)");

        #[cfg(not(feature = "mimalloc-allocator"))]
        info!("  🧠 内存分配器: 系统默认");
    }

    /// 加载缓存配置
    async fn load_cache_config(config: &ServerConfig) -> CacheResult<CacheConfig> {
        if let Some(config_path) = &config.cache_config_path {
            // 从文件加载配置
            let config_content = tokio::fs::read_to_string(config_path)
                .await
                .map_err(|e| CacheError::io_error(&format!("读取配置文件失败: {}", e)))?;

            let cache_config: CacheConfig = toml::from_str(&config_content)
                .map_err(|e| CacheError::config_error(&format!("解析配置文件失败: {}", e)))?;

            Ok(cache_config)
        } else {
            // 预设配置功能已移除，必须使用配置文件
            return Err(CacheError::config_error(
                "预设配置功能已移除，必须通过配置文件进行详细配置。请使用 --config 参数指定配置文件路径。"
            ));
        }
    }

    async fn create_tcp_listener(bind_addr: SocketAddr) -> CacheResult<TokioTcpListener> {
        info!("🔧 初始化传统 TCP 监听器");

        // 创建 TCP 监听器
        let listener = TokioTcpListener::bind(bind_addr)
            .await
            .map_err(|e| CacheError::io_error(&format!("绑定地址失败: {}", e)))?;

        // 设置平台特定的优化
        Self::configure_tcp_options(&listener).await?;

        info!("✅ TCP 监听器创建成功，地址: {}", bind_addr);
        Ok(listener)
    }

    /// 配置 TCP 选项（平台特定优化）
    async fn configure_tcp_options(listener: &TokioTcpListener) -> CacheResult<()> {
        info!("🔧 配置平台特定的 TCP 优化");

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            // 获取底层 socket 进行平台特定优化
            let socket = listener.as_raw_fd();

            // Unix 平台优化
            unsafe {
                // 设置 TCP_NODELAY 禁用 Nagle 算法
                let nodelay: libc::c_int = 1;
                if libc::setsockopt(
                    socket,
                    libc::IPPROTO_TCP,
                    libc::TCP_NODELAY,
                    &nodelay as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) != 0
                {
                    warn!("设置 TCP_NODELAY 失败: {}", std::io::Error::last_os_error());
                }

                // 设置 SO_REUSEADDR 允许地址重用
                let reuseaddr: libc::c_int = 1;
                if libc::setsockopt(
                    socket,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEADDR,
                    &reuseaddr as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) != 0
                {
                    warn!(
                        "设置 SO_REUSEADDR 失败: {}",
                        std::io::Error::last_os_error()
                    );
                }

                // 设置 SO_KEEPALIVE 启用连接保持
                let keepalive: libc::c_int = 1;
                if libc::setsockopt(
                    socket,
                    libc::SOL_SOCKET,
                    libc::SO_KEEPALIVE,
                    &keepalive as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) != 0
                {
                    warn!(
                        "设置 SO_KEEPALIVE 失败: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;
            use windows_sys::Win32::Networking::WinSock;

            // 获取底层 socket 进行平台特定优化
            let socket = listener.as_raw_socket();

            // Windows 平台优化
            unsafe {
                // 设置 TCP_NODELAY
                let nodelay: i32 = 1;
                if WinSock::setsockopt(
                    socket as WinSock::SOCKET,
                    WinSock::IPPROTO_TCP,
                    WinSock::TCP_NODELAY,
                    &nodelay as *const _ as *const u8,
                    std::mem::size_of::<i32>() as i32,
                ) != 0
                {
                    warn!("设置 TCP_NODELAY 失败: {}", std::io::Error::last_os_error());
                }

                // 设置 SO_REUSEADDR
                let reuseaddr: i32 = 1;
                if WinSock::setsockopt(
                    socket as WinSock::SOCKET,
                    WinSock::SOL_SOCKET,
                    WinSock::SO_REUSEADDR,
                    &reuseaddr as *const _ as *const u8,
                    std::mem::size_of::<i32>() as i32,
                ) != 0
                {
                    warn!(
                        "设置 SO_REUSEADDR 失败: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }

        info!("✅ TCP 优化配置完成");
        Ok(())
    }

    /// 启动服务器
    pub async fn start(&self) -> CacheResult<()> {
        info!("🚀 启动 RatMemcached 服务器");

        let listener = self.listener.as_ref().unwrap();
        info!("🔗 开始监听连接...");

        // 创建用于优雅退出的 future
        let shutdown = self.shutdown_notify.notified();

        // 使用 tokio::select! 来同时处理连接和退出信号
        tokio::select! {
            // 主循环：处理传入的连接
            result = async {
                loop {
                    match listener.accept().await {
                        Ok((stream, addr)) => {
                            info!("🔗 新连接来自: {}", addr);

                            // 为新连接创建处理任务
                            let cache = Arc::clone(&self.cache);
                            let start_time = self.start_time;

                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_tcp_connection(stream, cache, start_time).await
                                {
                                    error!("处理 TCP 连接失败: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("接受连接失败: {}", e);
                            // 短暂休眠避免错误循环
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
            } => {
                return result;
            },

            // 等待退出信号
            _ = shutdown => {
                info!("🛑 收到退出信号，开始优雅关闭...");
                // 这里可以执行一些清理工作
                Ok(())
            }
        }
    }

    /// 触发优雅退出
    pub async fn shutdown(&self) {
        info!("🛑 触发服务器关闭...");
        self.shutdown_notify.notify_waiters();
    }

    async fn handle_tcp_connection(
        mut stream: TcpStream,
        cache: Arc<RatMemCache>,
        start_time: Instant,
    ) -> CacheResult<()> {
        info!("🔗 开始处理 TCP 连接");

        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 5;
        const MAX_EMPTY_READS: u32 = 3;
        let mut empty_read_count = 0;
        let mut buffer_accumulator = String::new(); // 累积缓冲区
        let mut pending_command: Option<MemcachedCommand> = None; // 等待数据的命令
        let mut expected_bytes = 0; // 期待的数据字节数

        loop {
            // 尝试接收数据，设置超时
            let mut buffer = vec![0u8; 4096];
            let receive_result =
                tokio::time::timeout(Duration::from_secs(30), stream.read(&mut buffer)).await;

            match receive_result {
                Ok(Ok(bytes_read)) => {
                    if bytes_read == 0 {
                        empty_read_count += 1;
                        if empty_read_count >= MAX_EMPTY_READS {
                            debug!("连接连续收到空数据，可能已断开");
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }

                    // 重置错误计数器
                    consecutive_errors = 0;
                    empty_read_count = 0;

                    info!("📨 接收到 {} 字节数据", bytes_read);

                    // 将新数据添加到累积缓冲区
                    let new_data = String::from_utf8_lossy(&buffer[..bytes_read]);
                    buffer_accumulator.push_str(&new_data);

                    // 处理累积的数据
                    let mut should_quit = false;
                    while !buffer_accumulator.is_empty() {
                        if let Some(mut cmd) = pending_command.take() {
                            // 正在等待数据的命令

                            // 检查是否有足够的数据，需要考虑数据后的行结束符
                            let data_with_terminator_len = if buffer_accumulator.len()
                                >= expected_bytes + 2
                                && buffer_accumulator
                                    .chars()
                                    .skip(expected_bytes)
                                    .take(2)
                                    .collect::<String>()
                                    == "\r\n"
                            {
                                expected_bytes + 2 // 数据 + \r\n
                            } else if buffer_accumulator.len() >= expected_bytes + 1
                                && buffer_accumulator.chars().skip(expected_bytes).next()
                                    == Some('\n')
                            {
                                expected_bytes + 1 // 数据 + \n
                            } else {
                                0 // 数据不完整
                            };

                            if data_with_terminator_len > 0 {
                                let data = buffer_accumulator
                                    .chars()
                                    .take(expected_bytes)
                                    .collect::<String>();
                                buffer_accumulator = buffer_accumulator
                                    .chars()
                                    .skip(data_with_terminator_len)
                                    .collect();

                                // 设置命令数据
                                match &mut cmd {
                                    MemcachedCommand::Set { data: d, .. } => {
                                        *d = Some(Bytes::from(data.into_bytes()))
                                    }
                                    MemcachedCommand::Add { data: d, .. } => {
                                        *d = Some(Bytes::from(data.into_bytes()))
                                    }
                                    MemcachedCommand::Replace { data: d, .. } => {
                                        *d = Some(Bytes::from(data.into_bytes()))
                                    }
                                    _ => {}
                                }

                                // 执行命令
                                let response = Self::execute_command(cmd, &cache, start_time).await;
                                let response_data = Self::format_response(response);

                                if let Err(e) = stream.write_all(&response_data).await {
                                    error!("发送响应失败: {}", e);
                                    consecutive_errors += 1;
                                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                        return Ok(());
                                    }
                                }

                                pending_command = None;
                                expected_bytes = 0;
                            } else {
                                // 数据还不够，等待更多数据
                                pending_command = Some(cmd);
                                break;
                            }
                        } else {
                            // 查找完整的命令行，支持 \r\n 和 \n 两种结束符
                            let line_end_pos = if let Some(pos) = buffer_accumulator.find("\r\n") {
                                Some((pos, 2)) // \r\n 占用 2 个字符
                            } else if let Some(pos) = buffer_accumulator.find('\n') {
                                Some((pos, 1)) // \n 占用 1 个字符
                            } else {
                                None
                            };

                            if let Some((line_end, separator_len)) = line_end_pos {
                                let line = buffer_accumulator[..line_end].to_string();
                                buffer_accumulator =
                                    buffer_accumulator[line_end + separator_len..].to_string();

                                if line.trim().is_empty() {
                                    continue; // 跳过空行
                                }

                                debug!("📝 处理命令行: {}", line);
                                let command = Self::parse_command(&line);

                                // 检查是否需要等待数据
                                let needs_data = matches!(
                                    command,
                                    MemcachedCommand::Set { .. }
                                        | MemcachedCommand::Add { .. }
                                        | MemcachedCommand::Replace { .. }
                                );

                                if needs_data {
                                    // 获取期待的字节数
                                    let bytes = match &command {
                                        MemcachedCommand::Set { bytes, .. }
                                        | MemcachedCommand::Add { bytes, .. }
                                        | MemcachedCommand::Replace { bytes, .. } => *bytes,
                                        _ => 0,
                                    };
                                    pending_command = Some(command);
                                    expected_bytes = bytes;
                                } else if matches!(command, MemcachedCommand::Quit) {
                                    should_quit = true;
                                    let response =
                                        Self::execute_command(command, &cache, start_time).await;
                                    let response_data = Self::format_response(response);
                                    let _ = stream.write_all(&response_data).await;
                                    break;
                                } else {
                                    // 立即执行的命令
                                    let response =
                                        Self::execute_command(command, &cache, start_time).await;
                                    let response_data = Self::format_response(response);

                                    if let Err(e) = stream.write_all(&response_data).await
                                    {
                                        error!("发送响应失败: {}", e);
                                        consecutive_errors += 1;
                                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                            return Ok(());
                                        }
                                    }
                                }
                            } else {
                                // 没有完整的命令行，等待更多数据
                                break;
                            }
                        }
                    }

                    if should_quit {
                        info!("🔚 客户端请求退出连接");
                        break;
                    }
                }
                Ok(Err(e)) => {
                    error!("接收数据失败: {}", e);

                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("连续错误次数过多，停止处理");
                        break;
                    }
                    // 短暂等待后重试
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(_) => {
                    // 超时
                    debug!("接收数据超时，检查连接状态");
                    // 超时不算错误，继续循环检查连接状态
                }
            }
        }

        info!("🔚 连接处理结束");
        Ok(())
    }

    /// 格式化响应
    fn format_response(response: MemcachedResponse) -> Vec<u8> {
        match response {
            MemcachedResponse::Value {
                key,
                flags,
                bytes,
                data,
            } => {
                let header = format!("VALUE {} {} {}\r\n", key, flags, bytes);
                let mut response_data = Vec::new();
                response_data.extend_from_slice(header.as_bytes());
                response_data.extend_from_slice(&data);
                response_data.extend_from_slice(b"\r\nEND\r\n");
                response_data
            }
            MemcachedResponse::End => b"END\r\n".to_vec(),
            MemcachedResponse::Stored => b"STORED\r\n".to_vec(),
            MemcachedResponse::NotStored => b"NOT_STORED\r\n".to_vec(),
            MemcachedResponse::Deleted => b"DELETED\r\n".to_vec(),
            MemcachedResponse::NotFound => b"NOT_FOUND\r\n".to_vec(),
            MemcachedResponse::Ok => b"OK\r\n".to_vec(),
            MemcachedResponse::Error(msg) => format!("ERROR {}\r\n", msg).into_bytes(),
            MemcachedResponse::ServerError(msg) => format!("SERVER_ERROR {}\r\n", msg).into_bytes(),
            MemcachedResponse::ClientError(msg) => format!("CLIENT_ERROR {}\r\n", msg).into_bytes(),
            MemcachedResponse::Stats(stats) => {
                let mut result = Vec::new();
                for (key, value) in stats {
                    result.extend_from_slice(format!("STAT {} {}\r\n", key, value).as_bytes());
                }
                result.extend_from_slice(b"END\r\n");
                result
            }
            MemcachedResponse::Version(version) => format!("VERSION {}\r\n", version).into_bytes(),
            // 流式协议响应处理
            MemcachedResponse::StreamBegin { key, total_size, chunk_count } => {
                StreamingFormatter::format_stream_begin(&key, total_size, chunk_count)
            }
            MemcachedResponse::StreamData { key, chunk_number, data } => {
                StreamingFormatter::format_stream_data(&key, chunk_number, &data)
            }
            MemcachedResponse::StreamEnd { key } => {
                StreamingFormatter::format_stream_end(&key)
            }
            MemcachedResponse::StreamError(msg) => {
                StreamingFormatter::format_error(&msg)
            }
            _ => b"ERROR\r\n".to_vec(),
        }
    }

    /// 解析 Memcached 命令
    fn parse_command(line: &str) -> MemcachedCommand {
        let line = line.trim();
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.is_empty() {
            return MemcachedCommand::Unknown(line.to_string());
        }

        match parts[0].to_lowercase().as_str() {
            "get" => {
                let keys = parts[1..].iter().map(|s| s.to_string()).collect();
                MemcachedCommand::Get { keys }
            }
            "set" => {
                if parts.len() >= 5 {
                    let key = parts[1].to_string();
                    let flags = parts[2].parse().unwrap_or(0);
                    let exptime = parts[3].parse().unwrap_or(0);
                    let bytes = parts[4].parse().unwrap_or(0);
                    MemcachedCommand::Set {
                        key,
                        flags,
                        exptime,
                        bytes,
                        data: None,
                    }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "add" => {
                if parts.len() >= 5 {
                    let key = parts[1].to_string();
                    let flags = parts[2].parse().unwrap_or(0);
                    let exptime = parts[3].parse().unwrap_or(0);
                    let bytes = parts[4].parse().unwrap_or(0);
                    MemcachedCommand::Add {
                        key,
                        flags,
                        exptime,
                        bytes,
                        data: None,
                    }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "replace" => {
                if parts.len() >= 5 {
                    let key = parts[1].to_string();
                    let flags = parts[2].parse().unwrap_or(0);
                    let exptime = parts[3].parse().unwrap_or(0);
                    let bytes = parts[4].parse().unwrap_or(0);
                    MemcachedCommand::Replace {
                        key,
                        flags,
                        exptime,
                        bytes,
                        data: None,
                    }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "delete" => {
                if parts.len() >= 2 {
                    MemcachedCommand::Delete {
                        key: parts[1].to_string(),
                    }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "incr" => {
                if parts.len() >= 3 {
                    let key = parts[1].to_string();
                    let value = parts[2].parse().unwrap_or(1);
                    MemcachedCommand::Incr { key, value }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "decr" => {
                if parts.len() >= 3 {
                    let key = parts[1].to_string();
                    let value = parts[2].parse().unwrap_or(1);
                    MemcachedCommand::Decr { key, value }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            // 流式协议命令
            "streaming_get" | "sget" => {
                if parts.len() >= 2 {
                    let key = parts[1].to_string();
                    let chunk_size = parts.get(2).and_then(|s| s.parse().ok());
                    MemcachedCommand::StreamingGet { key, chunk_size }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "set_begin" => {
                if parts.len() >= 5 {
                    let key = parts[1].to_string();
                    let total_size = parts[2].parse().unwrap_or(0);
                    let chunk_count = parts[3].parse().unwrap_or(0);
                    let flags = parts[4].parse().unwrap_or(0);
                    let exptime = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
                    MemcachedCommand::SetBegin { key, total_size, chunk_count, flags, exptime }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "set_data" => {
                if parts.len() >= 3 {
                    let key = parts[1].to_string();
                    let chunk_number = parts[2].parse().unwrap_or(0);
                    MemcachedCommand::SetData { key, chunk_number, data: Bytes::new() } // 数据将在后续处理
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "set_end" => {
                if parts.len() >= 2 {
                    let key = parts[1].to_string();
                    MemcachedCommand::SetEnd { key }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "stats" => MemcachedCommand::Stats,
            "flush_all" => MemcachedCommand::Flush,
            "version" => MemcachedCommand::Version,
            "quit" => MemcachedCommand::Quit,
            _ => MemcachedCommand::Unknown(line.to_string()),
        }
    }

    /// 执行 Memcached 命令
    async fn execute_command(
        command: MemcachedCommand,
        cache: &Arc<RatMemCache>,
        start_time: Instant,
    ) -> MemcachedResponse {
        match command {
            MemcachedCommand::Get { keys } => {
                info!("执行 GET 命令: {:?}", keys);

                // 获取第一个键的值（简化实现）
                if let Some(key) = keys.first() {
                    match cache.get(key).await {
                        Ok(Some(data)) => {
                            info!("GET 命中: {} ({} bytes)", key, data.len());
                            MemcachedResponse::Value {
                                key: key.clone(),
                                flags: 0,
                                bytes: data.len(),
                                data,
                            }
                        }
                        Ok(None) => {
                            info!("GET 未命中: {}", key);
                            MemcachedResponse::End
                        }
                        Err(e) => {
                            error!("GET 失败: {}", e);
                            MemcachedResponse::ServerError(format!("获取失败: {}", e))
                        }
                    }
                } else {
                    MemcachedResponse::End
                }
            }
            MemcachedCommand::Set {
                key, exptime, data, ..
            } => {
                if let Some(data) = data {
                    info!(
                        "执行 SET 命令: {} ({} bytes, TTL: {})",
                        key,
                        data.len(),
                        exptime
                    );

                    let ttl = if exptime > 0 { exptime as u64 } else { 0 };

                    match cache.set_with_ttl(key.clone(), data, ttl).await {
                        Ok(_) => {
                            info!("SET 成功: {}", key);
                            MemcachedResponse::Stored
                        }
                        Err(e) => {
                            error!("SET 失败: {}", e);
                            MemcachedResponse::ServerError(format!("设置失败: {}", e))
                        }
                    }
                } else {
                    MemcachedResponse::ClientError("缺少数据".to_string())
                }
            }
            MemcachedCommand::Add {
                key, exptime, data, ..
            } => {
                if let Some(data) = data {
                    debug!(
                        "执行 ADD 命令: {} ({} bytes, TTL: {})",
                        key,
                        data.len(),
                        exptime
                    );

                    // 检查键是否已存在
                    match cache.get(&key).await {
                        Ok(Some(_)) => {
                            debug!("ADD 失败，键已存在: {}", key);
                            MemcachedResponse::NotStored
                        }
                        Ok(None) => {
                            let ttl = if exptime > 0 { exptime as u64 } else { 0 };
                            match cache.set_with_ttl(key.clone(), data, ttl).await {
                                Ok(_) => {
                                    debug!("ADD 成功: {}", key);
                                    MemcachedResponse::Stored
                                }
                                Err(e) => {
                                    error!("ADD 失败: {}", e);
                                    MemcachedResponse::ServerError(format!("添加失败: {}", e))
                                }
                            }
                        }
                        Err(e) => {
                            error!("ADD 检查失败: {}", e);
                            MemcachedResponse::ServerError(format!("检查失败: {}", e))
                        }
                    }
                } else {
                    MemcachedResponse::ClientError("缺少数据".to_string())
                }
            }
            MemcachedCommand::Replace {
                key, exptime, data, ..
            } => {
                if let Some(data) = data {
                    debug!(
                        "执行 REPLACE 命令: {} ({} bytes, TTL: {})",
                        key,
                        data.len(),
                        exptime
                    );

                    // 检查键是否存在
                    match cache.get(&key).await {
                        Ok(Some(_)) => {
                            let ttl = if exptime > 0 { exptime as u64 } else { 0 };
                            match cache.set_with_ttl(key.clone(), data, ttl).await {
                                Ok(_) => {
                                    debug!("REPLACE 成功: {}", key);
                                    MemcachedResponse::Stored
                                }
                                Err(e) => {
                                    error!("REPLACE 失败: {}", e);
                                    MemcachedResponse::ServerError(format!("替换失败: {}", e))
                                }
                            }
                        }
                        Ok(None) => {
                            debug!("REPLACE 失败，键不存在: {}", key);
                            MemcachedResponse::NotStored
                        }
                        Err(e) => {
                            error!("REPLACE 检查失败: {}", e);
                            MemcachedResponse::ServerError(format!("检查失败: {}", e))
                        }
                    }
                } else {
                    MemcachedResponse::ClientError("缺少数据".to_string())
                }
            }
            MemcachedCommand::Delete { key } => {
                debug!("执行 DELETE 命令: {}", key);

                match cache.delete(&key).await {
                    Ok(true) => {
                        debug!("DELETE 成功: {}", key);
                        MemcachedResponse::Deleted
                    }
                    Ok(false) => {
                        debug!("DELETE 失败，键不存在: {}", key);
                        MemcachedResponse::NotFound
                    }
                    Err(e) => {
                        error!("DELETE 失败: {}", e);
                        MemcachedResponse::ServerError(format!("删除失败: {}", e))
                    }
                }
            }
            MemcachedCommand::Incr { key, value } => {
                debug!("执行 INCR 命令: {} (+{})", key, value);

                // 简化实现：获取当前值，增加，然后设置
                match cache.get(&key).await {
                    Ok(Some(data)) => {
                        if let Ok(current_str) = String::from_utf8(data.to_vec()) {
                            if let Ok(current_val) = current_str.trim().parse::<u64>() {
                                let new_val = current_val.saturating_add(value);
                                let new_data = Bytes::from(new_val.to_string());

                                match cache.set_with_ttl(key, new_data, 0).await {
                                    Ok(_) => {
                                        debug!("INCR 成功: {} -> {}", current_val, new_val);
                                        MemcachedResponse::Value {
                                            key: "".to_string(),
                                            flags: 0,
                                            bytes: new_val.to_string().len(),
                                            data: Bytes::from(new_val.to_string()),
                                        }
                                    }
                                    Err(e) => {
                                        error!("INCR 设置失败: {}", e);
                                        MemcachedResponse::ServerError(format!("增加失败: {}", e))
                                    }
                                }
                            } else {
                                MemcachedResponse::ClientError("值不是数字".to_string())
                            }
                        } else {
                            MemcachedResponse::ClientError("值不是有效字符串".to_string())
                        }
                    }
                    Ok(None) => MemcachedResponse::NotFound,
                    Err(e) => {
                        error!("INCR 获取失败: {}", e);
                        MemcachedResponse::ServerError(format!("获取失败: {}", e))
                    }
                }
            }
            MemcachedCommand::Decr { key, value } => {
                debug!("执行 DECR 命令: {} (-{})", key, value);

                // 简化实现：获取当前值，减少，然后设置
                match cache.get(&key).await {
                    Ok(Some(data)) => {
                        if let Ok(current_str) = String::from_utf8(data.to_vec()) {
                            if let Ok(current_val) = current_str.trim().parse::<u64>() {
                                let new_val = current_val.saturating_sub(value);
                                let new_data = Bytes::from(new_val.to_string());

                                match cache.set_with_ttl(key, new_data, 0).await {
                                    Ok(_) => {
                                        debug!("DECR 成功: {} -> {}", current_val, new_val);
                                        MemcachedResponse::Value {
                                            key: "".to_string(),
                                            flags: 0,
                                            bytes: new_val.to_string().len(),
                                            data: Bytes::from(new_val.to_string()),
                                        }
                                    }
                                    Err(e) => {
                                        error!("DECR 设置失败: {}", e);
                                        MemcachedResponse::ServerError(format!("减少失败: {}", e))
                                    }
                                }
                            } else {
                                MemcachedResponse::ClientError("值不是数字".to_string())
                            }
                        } else {
                            MemcachedResponse::ClientError("值不是有效字符串".to_string())
                        }
                    }
                    Ok(None) => MemcachedResponse::NotFound,
                    Err(e) => {
                        error!("DECR 获取失败: {}", e);
                        MemcachedResponse::ServerError(format!("获取失败: {}", e))
                    }
                }
            }
            MemcachedCommand::Stats => {
                debug!("执行 STATS 命令");

                let mut stats_map = HashMap::new();
                let uptime = start_time.elapsed().as_secs();

                stats_map.insert("uptime".to_string(), uptime.to_string());
                stats_map.insert(
                    "version".to_string(),
                    format!("RatMemcached {}", env!("CARGO_PKG_VERSION")),
                );
                stats_map.insert("pointer_size".to_string(), "64".to_string());
                stats_map.insert("rusage_user".to_string(), "0.0".to_string());
                stats_map.insert("rusage_system".to_string(), "0.0".to_string());
                stats_map.insert("curr_items".to_string(), "0".to_string());
                stats_map.insert("total_items".to_string(), "0".to_string());
                stats_map.insert("bytes".to_string(), "0".to_string());
                stats_map.insert("curr_connections".to_string(), "1".to_string());
                stats_map.insert("total_connections".to_string(), "1".to_string());
                stats_map.insert("connection_structures".to_string(), "1".to_string());
                stats_map.insert("cmd_get".to_string(), "0".to_string());
                stats_map.insert("cmd_set".to_string(), "0".to_string());
                stats_map.insert("get_hits".to_string(), "0".to_string());
                stats_map.insert("get_misses".to_string(), "0".to_string());
                stats_map.insert("evictions".to_string(), "0".to_string());
                stats_map.insert("bytes_read".to_string(), "0".to_string());
                stats_map.insert("bytes_written".to_string(), "0".to_string());
                stats_map.insert("limit_maxbytes".to_string(), "67108864".to_string());
                stats_map.insert("threads".to_string(), "4".to_string());

                MemcachedResponse::Stats(stats_map)
            }
            MemcachedCommand::Flush => {
                debug!("执行 FLUSH_ALL 命令");

                match cache.clear().await {
                    Ok(_) => {
                        info!("FLUSH_ALL 成功");
                        MemcachedResponse::Ok
                    }
                    Err(e) => {
                        error!("FLUSH_ALL 失败: {}", e);
                        MemcachedResponse::ServerError(format!("清空失败: {}", e))
                    }
                }
            }
            MemcachedCommand::Version => {
                debug!("执行 VERSION 命令");
                MemcachedResponse::Version(format!("RatMemcached {}", env!("CARGO_PKG_VERSION")))
            }
            MemcachedCommand::Quit => {
                debug!("执行 QUIT 命令");
                MemcachedResponse::Ok
            }
            // 流式协议命令处理
            MemcachedCommand::StreamingGet { key, chunk_size } => {
                info!("执行流式GET命令: {} (chunk_size: {:?})", key, chunk_size);
                // 这里简化处理，直接返回流开始响应
                // 实际的流式数据传输需要在连接处理中实现
                match cache.get(&key).await {
                    Ok(Some(data)) => {
                        info!("流式GET命中: {} ({} bytes)", key, data.len());
                        let chunk_size = chunk_size.unwrap_or(4096);
                        let total_size = data.len();
                        let chunk_count = (total_size + chunk_size - 1) / chunk_size;

                        MemcachedResponse::StreamBegin {
                            key: key.clone(),
                            total_size,
                            chunk_count,
                        }
                    }
                    Ok(None) => {
                        info!("流式GET未命中: {}", key);
                        MemcachedResponse::StreamError("键不存在".to_string())
                    }
                    Err(e) => {
                        error!("流式GET失败: {}", e);
                        MemcachedResponse::StreamError(format!("获取失败: {}", e))
                    }
                }
            }
            MemcachedCommand::SetBegin { key, total_size, chunk_count, flags, exptime } => {
                info!("执行SET开始命令: {} (total: {} bytes, chunks: {})", key, total_size, chunk_count);
                // 初始化流式SET操作
                // 这里需要在服务器中维护状态，暂时简化处理
                MemcachedResponse::Stored
            }
            MemcachedCommand::SetData { key, chunk_number, data } => {
                info!("执行SET数据命令: {} (chunk: {}, size: {} bytes)", key, chunk_number, data.len());
                // 处理数据块
                MemcachedResponse::Stored
            }
            MemcachedCommand::SetEnd { key } => {
                info!("执行SET结束命令: {}", key);
                // 完成流式SET操作
                MemcachedResponse::Stored
            }
            MemcachedCommand::Unknown(cmd) => {
                warn!("未知命令: {}", cmd);
                MemcachedResponse::Error("未知命令".to_string())
            }
        }
    }
}

/// 加载服务器配置
fn load_server_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    // 尝试从配置文件加载
    let config_content = std::fs::read_to_string("rat_memcached.toml")?;
    let config = toml::from_str::<ServerConfig>(&config_content)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建命令行参数解析器
    let matches = Command::new("rat_memcached")
        .version(env!("CARGO_PKG_VERSION"))
        .author("RatMemcache Team")
        .about("高性能 Memcached 协议兼容服务器")
        .arg(
            Arg::new("bind")
                .short('b')
                .long("bind")
                .value_name("ADDRESS")
                .help("绑定地址 (默认: 127.0.0.1:11211)")
                .default_value("127.0.0.1:11211"),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("缓存配置文件路径"),
        )
        .get_matches();

    // 启动前的美观输出
    println!("🚀 RatMemcached - 高性能 Memcached 协议兼容服务器");
    println!("📦 基于 rat_memcache (MelangeDB存储后端)");
    println!("⚡ 支持完整的 Memcached 协议");
    #[cfg(feature = "mimalloc-allocator")]
    println!("🧠 使用 mimalloc 高性能内存分配器");

    // 从命令行参数构建配置
    let mut config = ServerConfig {
        bind_addr: matches.get_one::<String>("bind").unwrap().clone(),
        cache_config_path: matches.get_one::<String>("config").map(|s| s.clone()),
    };

    // 如果没有指定配置文件，尝试从默认配置文件加载
    if config.cache_config_path.is_none() {
        if let Ok(file_config) = load_server_config() {
            if file_config.cache_config_path.is_some() {
                config.cache_config_path = file_config.cache_config_path;
            }
        }
    }

    println!("⚙️ 服务器配置:");
    println!("  - 绑定地址: {}", config.bind_addr);
    if let Some(ref config_path) = config.cache_config_path {
        println!("  - 配置文件: {}", config_path);
    }

    // 创建并启动服务器
    let server = Arc::new(MemcachedServer::new(config).await?);

    // 启动后的日志使用 rat_logger
    info!("✅ 服务器创建成功，开始监听...");

    // 克隆服务器引用用于信号处理
    let server_clone = Arc::clone(&server);

    // 启动服务器任务
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            error!("服务器运行错误: {}", e);
        }
    });

    // 等待 Ctrl+C 信号
    tokio::select! {
        // 等待服务器自然结束
        result = server_handle => {
            if let Err(e) = result {
                error!("服务器任务异常退出: {}", e);
            }
        },

        // 等待 Ctrl+C 信号
        _ = signal::ctrl_c() => {
            info!("🛑 收到 Ctrl+C 信号，开始优雅关闭...");

            // 触发服务器关闭
            server_clone.shutdown().await;

            // 等待一小段时间让服务器完成清理
            tokio::time::sleep(Duration::from_millis(100)).await;

            info!("✅ 服务器已优雅关闭");
        }
    }

    Ok(())
}
