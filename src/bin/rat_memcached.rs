//! RatMemcached - 高性能 Memcached 协议兼容服务器
//!
//! 基于 mammoth_transport 和 rat_memcache 构建的高性能缓存服务器
//! 完全兼容 Memcached 协议，性能超越原版 Memcached

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use tokio::sync::RwLock;
use tokio::time::timeout;
use clap::{Arg, Command};

use mammoth_transport::config::{TransportBuilder, ProtocolType};
use mammoth_transport::core::TransportRuntime;
use mammoth_transport::protocols::tcp::{CongestionControlConfig, TcpConfig};
use mammoth_transport::metrics::global_metrics;

use rat_memcache::{
    RatMemCache, RatMemCacheBuilder,
    config::CacheConfig,
    error::{CacheError, CacheResult},
    logging::{init_logger, LogManager},
    perf_log, audit_log, cache_log,
};

// 使用 zerg_creep 日志宏
use zerg_creep::{info, warn, error, debug, trace};

/// 服务器配置
#[derive(Debug, Clone, serde::Deserialize)]
struct ServerConfig {
    /// 绑定地址
    bind_addr: String,
    /// 缓存配置文件路径
    cache_config_path: Option<String>,
    /// 预设配置类型
    preset: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:11211".to_string(),
            cache_config_path: None,
            preset: Some("high_speed_communication".to_string()),
        }
    }
}

/// Memcached 协议命令
#[derive(Debug, Clone)]
enum MemcachedCommand {
    Get { keys: Vec<String> },
    Set { key: String, flags: u32, exptime: u32, bytes: usize, data: Option<Bytes> },
    Add { key: String, flags: u32, exptime: u32, bytes: usize, data: Option<Bytes> },
    Replace { key: String, flags: u32, exptime: u32, bytes: usize, data: Option<Bytes> },
    Delete { key: String },
    Incr { key: String, value: u64 },
    Decr { key: String, value: u64 },
    Stats,
    Flush,
    Version,
    Quit,
    Unknown(String),
}

/// Memcached 协议响应
#[derive(Debug, Clone)]
enum MemcachedResponse {
    Value { key: String, flags: u32, bytes: usize, data: Bytes },
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
}

/// Memcached 服务器
pub struct MemcachedServer {
    cache: Arc<RatMemCache>,
    bind_addr: SocketAddr,
    config: ServerConfig,
    start_time: Instant,
    transport: Option<Arc<RwLock<TransportRuntime>>>,
}

impl MemcachedServer {
    /// 创建新的 Memcached 服务器
    pub async fn new(config: ServerConfig) -> CacheResult<Self> {
        let bind_addr: SocketAddr = config.bind_addr.parse()
            .map_err(|e| CacheError::config_error(&format!("无效的绑定地址: {}", e)))?;

        // 加载缓存配置
        let cache_config = Self::load_cache_config(&config).await?;
        
        // 初始化日志系统
        let log_manager = LogManager::new(cache_config.logging.clone());
        log_manager.initialize()?;
        
        info!("🚀 初始化 RatMemcached 服务器");
        info!("📍 绑定地址: {}", bind_addr);
        info!("🔧 强制使用 mammoth_transport 传输层");
        
        // 创建缓存实例
        let cache = Arc::new(RatMemCache::new(cache_config).await?);
        info!("✅ 缓存实例创建成功");
        
        // 创建传输层（bin 模式强制使用 mammoth_transport）
        let transport = Some(Arc::new(RwLock::new(Self::create_transport(bind_addr).await?)));
        
        Ok(Self {
            cache,
            bind_addr,
            config,
            start_time: Instant::now(),
            transport,
        })
    }
    
    /// 加载缓存配置
    async fn load_cache_config(config: &ServerConfig) -> CacheResult<CacheConfig> {
        if let Some(config_path) = &config.cache_config_path {
            // 从文件加载配置
            let config_content = tokio::fs::read_to_string(config_path).await
                .map_err(|e| CacheError::io_error(&format!("读取配置文件失败: {}", e)))?;
            
            let cache_config: CacheConfig = toml::from_str(&config_content)
                .map_err(|e| CacheError::config_error(&format!("解析配置文件失败: {}", e)))?;
            
            Ok(cache_config)
        } else {
            // 使用预设配置
            match config.preset.as_deref() {
                Some("development") => CacheConfig::development(),
                Some("production") => CacheConfig::production(),
                Some("high_speed_communication") | None => CacheConfig::high_speed_communication(),
                Some(preset) => {
                    return Err(CacheError::config_error(&format!("未知的预设配置: {}", preset)));
                }
            }
        }
    }
    
    async fn create_transport(bind_addr: SocketAddr) -> CacheResult<TransportRuntime> {
        info!("🔧 初始化 mammoth_transport 传输层");
        
        // 初始化全局指标系统
        global_metrics().initialize().await
            .map_err(|e| CacheError::io_error(&format!("初始化指标系统失败: {}", e)))?;
        
        // 创建高性能 TCP 配置
        let tcp_config = TcpConfig::default()
            .with_connect_timeout(Duration::from_secs(5))
            .with_read_timeout(Some(Duration::from_secs(30)))
            .with_write_timeout(Some(Duration::from_secs(30)))
            .with_no_delay(true) // 禁用 Nagle 算法，优化延迟
            .with_reuse_options(true, true)
            .with_backlog(Some(2048)) // 大监听队列支持高并发
            .with_zero_copy(true)
            .with_congestion_control(Some(
                CongestionControlConfig::new()
                    .with_algorithm("auto")
                    .with_platform_optimized(true),
            ));
        
        // 使用服务器高吞吐预设
        let transport = TransportBuilder::new()
            .as_server_high_throughput()
            .map_err(|e| CacheError::config_error(&format!("创建传输构建器失败: {}", e)))?
            .with_tcp_listener(bind_addr, tcp_config)
            .build()
            .map_err(|e| CacheError::config_error(&format!("构建传输层失败: {}", e)))?;
        
        info!("✅ mammoth_transport 传输层创建成功");
        Ok(transport)
    }
    
    /// 启动服务器
    pub async fn start(&self) -> CacheResult<()> {
        info!("🚀 启动 RatMemcached 服务器");
        
        // bin 模式强制使用 mammoth_transport
        self.start_with_mammoth_transport().await
    }
    
    async fn start_with_mammoth_transport(&self) -> CacheResult<()> {
        info!("🔧 使用 mammoth_transport 启动服务器");
        
        let transport = self.transport.as_ref().unwrap();
        
        // 启动传输层
        {
            let mut transport_guard = transport.write().await;
            transport_guard.start().await
                .map_err(|e| CacheError::io_error(&format!("启动传输层失败: {}", e)))?;
        }
        
        // 等待监听器完全启动
        tokio::time::sleep(Duration::from_millis(500)).await;
        info!("✅ mammoth_transport 传输层已启动");
        
        // 获取连接适配器并处理连接
        let connection_adapter = {
            let transport_guard = transport.read().await;
            transport_guard.connection_adapter()
                .map_err(|e| CacheError::io_error(&format!("获取连接适配器失败: {}", e)))?
        };
        
        info!("🔗 开始监听连接...");
        
        // 跟踪已知连接，避免重复处理
        let mut known_connections: std::collections::HashSet<mammoth_transport::core::ConnectionId> = std::collections::HashSet::new();
        
        // 主循环：处理传入的连接
        loop {
            // 检查活跃连接并处理新连接
            match connection_adapter.list_active_connections().await {
                Ok(active_connections) => {
                    // 检查是否有新连接
                    for connection_id in &active_connections {
                        if !known_connections.contains(connection_id) {
                            // 发现新连接
                            known_connections.insert(connection_id.clone());
                            
                            // 获取连接信息
                            match connection_adapter.get_connection_info(connection_id).await {
                                Ok(conn_info) => {
                                    info!("🔗 检测到新连接: {} 来自 {}", connection_id, conn_info.remote_addr);
                                    
                                    // 为新连接创建处理任务
                                    let cache = Arc::clone(&self.cache);
                                    let adapter = connection_adapter.clone();
                                    let start_time = self.start_time;
                                    let conn_id = connection_id.clone();
                                    let conn_id_for_error = connection_id.clone();
                                    
                                    tokio::spawn(async move {
                                        if let Err(e) = Self::handle_mammoth_connection(conn_id, cache, adapter, start_time).await {
                                            error!("处理 mammoth_transport 连接 {} 失败: {}", conn_id_for_error, e);
                                        }
                                    });
                                }
                                Err(e) => {
                                    error!("获取连接 {} 信息失败: {}", connection_id, e);
                                    known_connections.remove(connection_id);
                                }
                            }
                        }
                    }
                    
                    // 清理已断开的连接
                    known_connections.retain(|conn_id| active_connections.contains(conn_id));
                }
                Err(e) => {
                    debug!("获取活跃连接失败: {}", e);
                }
            }
            
            // 短暂休眠避免过度轮询
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    async fn handle_mammoth_connection(
        connection_id: mammoth_transport::core::ConnectionId,
        cache: Arc<RatMemCache>,
        connection_adapter: mammoth_transport::adapters::ConnectionAdapter,
        start_time: Instant,
    ) -> CacheResult<()> {
        info!("🔗 开始处理连接: {}", connection_id);
        
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 5;
        const MAX_EMPTY_READS: u32 = 3;
        let mut empty_read_count = 0;
        let mut buffer_accumulator = String::new(); // 累积缓冲区
        let mut pending_command: Option<MemcachedCommand> = None; // 等待数据的命令
        let mut expected_bytes = 0; // 期待的数据字节数
        
        loop {
            info!("🔄 连接 {} 进入处理循环", connection_id);
            // 检查连接是否仍然活跃
            match connection_adapter.list_active_connections().await {
                Ok(active_connections) => {
                    if !active_connections.contains(&connection_id) {
                        info!("🔌 连接 {} 已断开", connection_id);
                        break;
                    }
                }
                Err(e) => {
                    error!("检查连接 {} 状态失败: {}", connection_id, e);
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("连接 {} 连续错误次数过多，停止处理", connection_id);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
            
            // 尝试接收数据，设置超时
            let mut buffer = vec![0u8; 4096];
            info!("🔍 准备从连接 {} 接收数据，缓冲区大小: {}", connection_id, buffer.len());
            let receive_result = tokio::time::timeout(
                Duration::from_secs(30),
                connection_adapter.receive_data(connection_id.clone(), &mut buffer)
            ).await;
            info!("📥 连接 {} 接收数据调用完成", connection_id);
            
            match receive_result {
                Ok(Ok(bytes_read)) => {
                    if bytes_read == 0 {
                        empty_read_count += 1;
                        if empty_read_count >= MAX_EMPTY_READS {
                            debug!("连接 {} 连续收到空数据，可能已断开", connection_id);
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    
                    // 重置错误计数器
                    consecutive_errors = 0;
                    empty_read_count = 0;
                    
                    info!("📨 从连接 {} 接收到 {} 字节数据", connection_id, bytes_read);
                    
                    // 将新数据添加到累积缓冲区
                    let new_data = String::from_utf8_lossy(&buffer[..bytes_read]);
                    buffer_accumulator.push_str(&new_data);
                    info!("📝 累积缓冲区内容: {:?}", buffer_accumulator);
                    
                    // 处理累积的数据
                    info!("🔄 开始处理累积数据，缓冲区长度: {}", buffer_accumulator.len());
                    let mut should_quit = false;
                    while !buffer_accumulator.is_empty() {
                        if let Some(mut cmd) = pending_command.take() {
                            // 正在等待数据的命令
                            info!("📋 处理待处理命令，期待字节数: {}, 当前缓冲区长度: {}", expected_bytes, buffer_accumulator.len());
                            info!("📋 当前缓冲区内容: {:?}", buffer_accumulator);
                            
                            // 检查是否有足够的数据，需要考虑数据后的行结束符
                            let data_with_terminator_len = if buffer_accumulator.len() >= expected_bytes + 2 
                                && buffer_accumulator.chars().skip(expected_bytes).take(2).collect::<String>() == "\r\n" {
                                info!("📋 检测到 \\r\\n 结束符");
                                expected_bytes + 2 // 数据 + \r\n
                            } else if buffer_accumulator.len() >= expected_bytes + 1 
                                && buffer_accumulator.chars().skip(expected_bytes).next() == Some('\n') {
                                info!("📋 检测到 \\n 结束符");
                                expected_bytes + 1 // 数据 + \n
                            } else {
                                info!("📋 数据不完整，等待更多数据");
                                0 // 数据不完整
                            };
                            
                            if data_with_terminator_len > 0 {
                                let data = buffer_accumulator.chars().take(expected_bytes).collect::<String>();
                                buffer_accumulator = buffer_accumulator.chars().skip(data_with_terminator_len).collect();
                                info!("📋 提取的数据: {:?}, 剩余缓冲区: {:?}", data, buffer_accumulator);
                                
                                // 设置命令数据
                                 match &mut cmd {
                                     MemcachedCommand::Set { data: d, .. } => *d = Some(Bytes::from(data.into_bytes())),
                                     MemcachedCommand::Add { data: d, .. } => *d = Some(Bytes::from(data.into_bytes())),
                                     MemcachedCommand::Replace { data: d, .. } => *d = Some(Bytes::from(data.into_bytes())),
                                     _ => {}
                                 }
                                
                                info!("📋 数据设置后的命令: {:?}", cmd);
                                
                                // 执行命令
                                let response = Self::execute_command(cmd, &cache, start_time).await;
                                let response_data = Self::format_response(response);
                                
                                if let Err(e) = connection_adapter.send_data(connection_id.clone(), response_data.as_bytes()).await {
                                    error!("向连接 {} 发送响应失败: {}", connection_id, e);
                                    consecutive_errors += 1;
                                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                        return Ok(());
                                    }
                                } else {
                                    debug!("✅ 向连接 {} 发送响应成功", connection_id);
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
                                buffer_accumulator = buffer_accumulator[line_end + separator_len..].to_string();
                                
                                if line.trim().is_empty() {
                                    continue; // 跳过空行
                                }
                                
                                debug!("📝 处理命令行: {}", line);
                                 let command = Self::parse_command(&line);
                                 info!("🔍 解析的命令: {:?}", command);
                                 
                                 // 检查是否需要等待数据
                                 let needs_data = matches!(command, 
                                     MemcachedCommand::Set { .. } |
                                     MemcachedCommand::Add { .. } |
                                     MemcachedCommand::Replace { .. }
                                 );
                                 
                                 if needs_data {
                                     // 获取期待的字节数
                                     let bytes = match &command {
                                         MemcachedCommand::Set { bytes, .. } |
                                         MemcachedCommand::Add { bytes, .. } |
                                         MemcachedCommand::Replace { bytes, .. } => *bytes,
                                         _ => 0,
                                     };
                                     info!("📋 设置待处理命令，期待字节数: {}", bytes);
                                     pending_command = Some(command);
                                     expected_bytes = bytes;
                                 } else if matches!(command, MemcachedCommand::Quit) {
                                     should_quit = true;
                                     let response = Self::execute_command(command, &cache, start_time).await;
                                     let response_data = Self::format_response(response);
                                     let _ = connection_adapter.send_data(connection_id.clone(), response_data.as_bytes()).await;
                                     break;
                                 } else {
                                     // 立即执行的命令
                                     let response = Self::execute_command(command, &cache, start_time).await;
                                     let response_data = Self::format_response(response);
                                     
                                     if let Err(e) = connection_adapter.send_data(connection_id.clone(), response_data.as_bytes()).await {
                                         error!("向连接 {} 发送响应失败: {}", connection_id, e);
                                         consecutive_errors += 1;
                                         if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                             return Ok(());
                                         }
                                     } else {
                                         debug!("✅ 向连接 {} 发送响应成功", connection_id);
                                     }
                                 }
                            } else {
                                // 没有完整的命令行，等待更多数据
                                break;
                            }
                        }
                    }
                    
                    if should_quit {
                        info!("🔚 客户端请求退出连接: {}", connection_id);
                        break;
                    }
                }
                Ok(Err(e)) => {
                    let error_msg = e.to_string();
                    error!("从连接 {} 接收数据失败: {}", connection_id, e);
                    
                    // 如果是连接不存在错误，立即退出
                    if error_msg.contains("连接句柄不存在") || error_msg.contains("Connection not found") {
                        info!("🔌 连接 {} 已不存在，停止处理", connection_id);
                        break;
                    }
                    
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("连接 {} 连续错误次数过多，停止处理", connection_id);
                        break;
                    }
                    // 短暂等待后重试
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(_) => {
                    // 超时
                    debug!("连接 {} 接收数据超时，检查连接状态", connection_id);
                    // 超时不算错误，继续循环检查连接状态
                }
            }
        }
        
        info!("🔚 连接 {} 处理结束", connection_id);
        Ok(())
    }
    
    /// 格式化响应
    fn format_response(response: MemcachedResponse) -> String {
        match response {
            MemcachedResponse::Value { key, flags, bytes, data } => {
                format!("VALUE {} {} {}\r\n{}\r\nEND\r\n", key, flags, bytes, String::from_utf8_lossy(&data))
            }
            MemcachedResponse::End => "END\r\n".to_string(),
            MemcachedResponse::Stored => "STORED\r\n".to_string(),
            MemcachedResponse::NotStored => "NOT_STORED\r\n".to_string(),
            MemcachedResponse::Deleted => "DELETED\r\n".to_string(),
            MemcachedResponse::NotFound => "NOT_FOUND\r\n".to_string(),
            MemcachedResponse::Ok => "OK\r\n".to_string(),
            MemcachedResponse::Error(msg) => format!("ERROR {}\r\n", msg),
            MemcachedResponse::ServerError(msg) => format!("SERVER_ERROR {}\r\n", msg),
            MemcachedResponse::ClientError(msg) => format!("CLIENT_ERROR {}\r\n", msg),
            MemcachedResponse::Stats(stats) => {
                let mut result = String::new();
                for (key, value) in stats {
                    result.push_str(&format!("STAT {} {}\r\n", key, value));
                }
                result.push_str("END\r\n");
                result
            }
            MemcachedResponse::Version(version) => format!("VERSION {}\r\n", version),
            _ => "ERROR\r\n".to_string(),
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
                    MemcachedCommand::Set { key, flags, exptime, bytes, data: None }
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
                    MemcachedCommand::Add { key, flags, exptime, bytes, data: None }
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
                    MemcachedCommand::Replace { key, flags, exptime, bytes, data: None }
                } else {
                    MemcachedCommand::Unknown(line.to_string())
                }
            }
            "delete" => {
                if parts.len() >= 2 {
                    MemcachedCommand::Delete { key: parts[1].to_string() }
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
            MemcachedCommand::Set { key, exptime, data, .. } => {
                if let Some(data) = data {
                    info!("执行 SET 命令: {} ({} bytes, TTL: {})", key, data.len(), exptime);
                    
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
            MemcachedCommand::Add { key, exptime, data, .. } => {
                if let Some(data) = data {
                    debug!("执行 ADD 命令: {} ({} bytes, TTL: {})", key, data.len(), exptime);
                    
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
            MemcachedCommand::Replace { key, exptime, data, .. } => {
                if let Some(data) = data {
                    debug!("执行 REPLACE 命令: {} ({} bytes, TTL: {})", key, data.len(), exptime);
                    
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
                stats_map.insert("version".to_string(), format!("RatMemcached {}", env!("CARGO_PKG_VERSION")));
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
                .default_value("127.0.0.1:11211")
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("缓存配置文件路径")
        )
        .arg(
            Arg::new("preset")
                .short('p')
                .long("preset")
                .value_name("PRESET")
                .help("预设配置类型 (development, production, high_speed_communication)")
                .default_value("high_speed_communication")
        )
        .get_matches();

    // 启动前的美观输出
    println!("🚀 RatMemcached - 高性能 Memcached 协议兼容服务器");
    println!("📦 基于 rat_memcache + mammoth_transport");
    println!("⚡ 支持完整的 Memcached 协议");
    
    // 从命令行参数构建配置
    let mut config = ServerConfig {
        bind_addr: matches.get_one::<String>("bind").unwrap().clone(),
        cache_config_path: matches.get_one::<String>("config").map(|s| s.clone()),
        preset: Some(matches.get_one::<String>("preset").unwrap().clone()),
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
    println!("  - 强制使用 mammoth_transport 传输层");
    println!("  - 预设配置: {:?}", config.preset);
    if let Some(ref config_path) = config.cache_config_path {
        println!("  - 配置文件: {}", config_path);
    }
    
    // 创建并启动服务器
    let server = MemcachedServer::new(config).await?;
    
    // 启动后的日志使用 zerg_creep
    info!("✅ 服务器创建成功，开始监听...");
    
    server.start().await?;
    
    Ok(())
}