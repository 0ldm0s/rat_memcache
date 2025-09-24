/*!
 * 流式协议支持
 *
 * 扩展memcached协议，支持大值数据的流式传输
 */

use bytes::Bytes;
use std::collections::HashMap;

/// 流式命令类型
#[derive(Debug, Clone, PartialEq)]
pub enum StreamingCommand {
    /// 流式GET请求
    StreamingGet { key: String, chunk_size: Option<usize> },
    /// 分块SET开始
    SetBegin { key: String, total_size: usize, chunk_count: usize, flags: u32, exptime: u32 },
    /// 分块SET数据
    SetData { key: String, chunk_number: usize, data: Bytes },
    /// 分块SET结束
    SetEnd { key: String },
}

/// 流式响应类型
#[derive(Debug, Clone)]
pub enum StreamingResponse {
    /// 流开始
    StreamBegin { key: String, total_size: usize, chunk_count: usize },
    /// 数据块
    StreamData { key: String, chunk_number: usize, data: Bytes },
    /// 流结束
    StreamEnd { key: String },
    /// 错误响应
    Error(String),
}

/// 流式协议解析器
pub struct StreamingParser {
    /// 正在进行的分块SET操作
    pending_sets: HashMap<String, PendingSetOperation>,
}

/// 待完成的SET操作
#[derive(Debug)]
struct PendingSetOperation {
    total_size: usize,
    chunk_count: usize,
    flags: u32,
    exptime: u32,
    received_chunks: HashMap<usize, Bytes>,
}

impl StreamingParser {
    pub fn new() -> Self {
        Self {
            pending_sets: HashMap::new(),
        }
    }

    /// 解析流式命令
    pub fn parse_command(&mut self, line: &str, data: Option<Bytes>) -> Option<StreamingCommand> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        match parts[0].to_lowercase().as_str() {
            "streaming_get" | "sget" => {
                if parts.len() >= 2 {
                    let key = parts[1].to_string();
                    let chunk_size = parts.get(2).and_then(|s| s.parse().ok());
                    Some(StreamingCommand::StreamingGet { key, chunk_size })
                } else {
                    None
                }
            }
            "set_begin" => {
                if parts.len() >= 5 {
                    let key = parts[1].to_string();
                    let total_size = parts[2].parse().unwrap_or(0);
                    let chunk_count = parts[3].parse().unwrap_or(0);
                    let flags = parts[4].parse().unwrap_or(0);
                    let exptime = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

                    let pending_op = PendingSetOperation {
                        total_size,
                        chunk_count,
                        flags,
                        exptime,
                        received_chunks: HashMap::new(),
                    };
                    self.pending_sets.insert(key.clone(), pending_op);

                    Some(StreamingCommand::SetBegin { key, total_size, chunk_count, flags, exptime })
                } else {
                    None
                }
            }
            "set_data" => {
                if parts.len() >= 3 && data.is_some() {
                    let key = parts[1].to_string();
                    let chunk_number = parts[2].parse().unwrap_or(0);
                    Some(StreamingCommand::SetData { key, chunk_number, data: data.unwrap() })
                } else {
                    None
                }
            }
            "set_end" => {
                if parts.len() >= 2 {
                    let key = parts[1].to_string();
                    Some(StreamingCommand::SetEnd { key })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// 检查SET操作是否完成
    pub fn check_set_complete(&mut self, key: &str) -> Option<(PendingSetOperation, Vec<u8>)> {
        if let Some(pending_op) = self.pending_sets.get(key) {
            if pending_op.received_chunks.len() == pending_op.chunk_count {
                // 按顺序组装数据
                let mut assembled_data = Vec::new();
                for i in 0..pending_op.chunk_count {
                    if let Some(chunk) = pending_op.received_chunks.get(&i) {
                        assembled_data.extend_from_slice(chunk);
                    } else {
                        return None; // 缺少数据块
                    }
                }

                let op = self.pending_sets.remove(key);
                return Some((op.unwrap(), assembled_data));
            }
        }
        None
    }

    /// 添加数据块
    pub fn add_chunk(&mut self, key: String, chunk_number: usize, data: Bytes) -> bool {
        if let Some(pending_op) = self.pending_sets.get_mut(&key) {
            pending_op.received_chunks.insert(chunk_number, data);
            true
        } else {
            false
        }
    }
}

impl Default for StreamingParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 流式响应格式化器
pub struct StreamingFormatter;

impl StreamingFormatter {
    /// 格式化流开始响应
    pub fn format_stream_begin(key: &str, total_size: usize, chunk_count: usize) -> Vec<u8> {
        format!("STREAM_BEGIN {} {} {}\r\n", key, total_size, chunk_count).into_bytes()
    }

    /// 格式化数据块响应
    pub fn format_stream_data(key: &str, chunk_number: usize, data: &[u8]) -> Vec<u8> {
        format!("STREAM_DATA {} {} {}\r\n", key, chunk_number, data.len()).into_bytes()
    }

    /// 格式化流结束响应
    pub fn format_stream_end(key: &str) -> Vec<u8> {
        format!("STREAM_END {}\r\n", key).into_bytes()
    }

    /// 格式化错误响应
    pub fn format_error(msg: &str) -> Vec<u8> {
        format!("STREAM_ERROR {}\r\n", msg).into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_streaming_get() {
        let mut parser = StreamingParser::new();

        let cmd = parser.parse_command("sget my_key 4096", None);
        assert_eq!(cmd, Some(StreamingCommand::StreamingGet {
            key: "my_key".to_string(),
            chunk_size: Some(4096)
        }));
    }

    #[test]
    fn test_parse_set_begin() {
        let mut parser = StreamingParser::new();

        let cmd = parser.parse_command("set_begin my_key 50000 13 0 300", None);
        assert_eq!(cmd, Some(StreamingCommand::SetBegin {
            key: "my_key".to_string(),
            total_size: 50000,
            chunk_count: 13,
            flags: 0,
            exptime: 300
        }));
    }

    #[test]
    fn test_formatter() {
        let begin = StreamingFormatter::format_stream_begin("test_key", 50000, 13);
        assert_eq!(String::from_utf8_lossy(&begin), "STREAM_BEGIN test_key 50000 13\r\n");

        let data = StreamingFormatter::format_stream_data("test_key", 0, b"test_data");
        assert_eq!(String::from_utf8_lossy(&data), "STREAM_DATA test_key 0 9\r\n");

        let end = StreamingFormatter::format_stream_end("test_key");
        assert_eq!(String::from_utf8_lossy(&end), "STREAM_END test_key\r\n");
    }
}