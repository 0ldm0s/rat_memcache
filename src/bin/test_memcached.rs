//! RatMemcached 功能测试工具
//!
//! 通过 TCP 连接测试 RatMemcached 服务器的所有功能
//! 正常退出表示成功，异常退出返回错误信息和非零退出代码

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process;
use std::time::{Duration, Instant};

/// 测试配置
struct TestConfig {
    server_addr: String,
    timeout: Duration,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:11211".to_string(),
            timeout: Duration::from_secs(5), // 较短的超时限制
        }
    }
}

/// 测试结果
#[derive(Debug)]
struct TestResult {
    test_name: String,
    success: bool,
    error: Option<String>,
    duration: Duration,
}

impl TestResult {
    fn success(test_name: String, duration: Duration) -> Self {
        Self {
            test_name,
            success: true,
            error: None,
            duration,
        }
    }
    
    fn failure(test_name: String, error: String, duration: Duration) -> Self {
        Self {
            test_name,
            success: false,
            error: Some(error),
            duration,
        }
    }
}

/// Memcached 测试客户端
struct MemcachedTestClient {
    stream: TcpStream,
    config: TestConfig,
}

impl MemcachedTestClient {
    /// 连接到服务器
    fn connect(config: TestConfig) -> Result<Self, String> {
        println!("🔗 连接到服务器: {}", config.server_addr);
        
        let stream = TcpStream::connect_timeout(
            &config.server_addr.parse().map_err(|e| format!("解析地址失败: {}", e))?,
            config.timeout,
        ).map_err(|e| format!("连接失败: {}", e))?;
        
        // 设置读写超时
        stream.set_read_timeout(Some(config.timeout))
            .map_err(|e| format!("设置读超时失败: {}", e))?;
        stream.set_write_timeout(Some(config.timeout))
            .map_err(|e| format!("设置写超时失败: {}", e))?;
        
        println!("✅ 连接成功");
        
        Ok(Self { stream, config })
    }
    
    /// 发送命令并接收响应
    fn send_command(&mut self, command: &str) -> Result<String, String> {
        // 发送命令
        self.stream.write_all(command.as_bytes())
            .map_err(|e| format!("发送命令失败: {}", e))?;
        self.stream.flush()
            .map_err(|e| format!("刷新缓冲区失败: {}", e))?;
        
        // 接收响应
        let mut buffer = [0u8; 4096];
        let bytes_read = self.stream.read(&mut buffer)
            .map_err(|e| format!("读取响应失败: {}", e))?;
        
        if bytes_read == 0 {
            return Err("服务器关闭连接".to_string());
        }
        
        let response = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
        Ok(response)
    }
    
    /// 发送存储命令（需要数据）
    fn send_storage_command(&mut self, command: &str, data: &str) -> Result<String, String> {
        // 发送命令行
        self.stream.write_all(command.as_bytes())
            .map_err(|e| format!("发送命令失败: {}", e))?;
        
        // 发送数据
        self.stream.write_all(data.as_bytes())
            .map_err(|e| format!("发送数据失败: {}", e))?;
        self.stream.write_all(b"\r\n")
            .map_err(|e| format!("发送结束符失败: {}", e))?;
        
        self.stream.flush()
            .map_err(|e| format!("刷新缓冲区失败: {}", e))?;
        
        // 接收响应
        let mut buffer = [0u8; 4096];
        let bytes_read = self.stream.read(&mut buffer)
            .map_err(|e| format!("读取响应失败: {}", e))?;
        
        if bytes_read == 0 {
            return Err("服务器关闭连接".to_string());
        }
        
        let response = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
        Ok(response)
    }
    
    /// 测试版本命令
    fn test_version(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "VERSION".to_string();
        
        match self.send_command("version\r\n") {
            Ok(response) => {
                if response.starts_with("VERSION") {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("版本响应格式错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试统计命令
    fn test_stats(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "STATS".to_string();
        
        match self.send_command("stats\r\n") {
            Ok(response) => {
                if response.contains("STAT") && response.ends_with("END\r\n") {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("统计响应格式错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 SET 命令
    fn test_set(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "SET".to_string();
        
        match self.send_storage_command("set test_key 0 0 10\r\n", "test_value") {
            Ok(response) => {
                if response.trim() == "STORED" {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("SET 响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 GET 命令
    fn test_get(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "GET".to_string();
        
        match self.send_command("get test_key\r\n") {
            Ok(response) => {
                if response.contains("VALUE test_key") && response.contains("test_value") && response.ends_with("END\r\n") {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("GET 响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 ADD 命令
    fn test_add(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "ADD".to_string();
        
        // 测试添加新键
        match self.send_storage_command("add new_key 0 0 9\r\n", "new_value") {
            Ok(response) => {
                if response.trim() == "STORED" {
                    // 测试添加已存在的键
                    match self.send_storage_command("add new_key 0 0 9\r\n", "new_value") {
                        Ok(response2) => {
                            if response2.trim() == "NOT_STORED" {
                                TestResult::success(test_name, start.elapsed())
                            } else {
                                TestResult::failure(
                                    test_name,
                                    format!("ADD 重复键响应错误: {}", response2.trim()),
                                    start.elapsed(),
                                )
                            }
                        }
                        Err(e) => TestResult::failure(test_name, e, start.elapsed()),
                    }
                } else {
                    TestResult::failure(
                        test_name,
                        format!("ADD 新键响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 REPLACE 命令
    fn test_replace(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "REPLACE".to_string();
        
        // 测试替换存在的键
        match self.send_storage_command("replace test_key 0 0 13\r\n", "replaced_value") {
            Ok(response) => {
                if response.trim() == "STORED" {
                    // 测试替换不存在的键
                    match self.send_storage_command("replace nonexistent 0 0 5\r\n", "value") {
                        Ok(response2) => {
                            if response2.trim() == "NOT_STORED" {
                                TestResult::success(test_name, start.elapsed())
                            } else {
                                TestResult::failure(
                                    test_name,
                                    format!("REPLACE 不存在键响应错误: {}", response2.trim()),
                                    start.elapsed(),
                                )
                            }
                        }
                        Err(e) => TestResult::failure(test_name, e, start.elapsed()),
                    }
                } else {
                    TestResult::failure(
                        test_name,
                        format!("REPLACE 存在键响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 INCR/DECR 命令
    fn test_incr_decr(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "INCR/DECR".to_string();
        
        // 设置一个数字值
        match self.send_storage_command("set counter 0 0 2\r\n", "10") {
            Ok(response) => {
                if response.trim() != "STORED" {
                    return TestResult::failure(
                        test_name,
                        format!("设置计数器失败: {}", response.trim()),
                        start.elapsed(),
                    );
                }
            }
            Err(e) => return TestResult::failure(test_name, e, start.elapsed()),
        }
        
        // 测试 INCR
        match self.send_command("incr counter 5\r\n") {
            Ok(response) => {
                if !response.contains("15") {
                    return TestResult::failure(
                        test_name,
                        format!("INCR 响应错误: {}", response.trim()),
                        start.elapsed(),
                    );
                }
            }
            Err(e) => return TestResult::failure(test_name, e, start.elapsed()),
        }
        
        // 测试 DECR
        match self.send_command("decr counter 3\r\n") {
            Ok(response) => {
                if response.contains("12") {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("DECR 响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 DELETE 命令
    fn test_delete(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "DELETE".to_string();
        
        // 删除存在的键
        match self.send_command("delete test_key\r\n") {
            Ok(response) => {
                if response.trim() == "DELETED" {
                    // 删除不存在的键
                    match self.send_command("delete nonexistent\r\n") {
                        Ok(response2) => {
                            if response2.trim() == "NOT_FOUND" {
                                TestResult::success(test_name, start.elapsed())
                            } else {
                                TestResult::failure(
                                    test_name,
                                    format!("DELETE 不存在键响应错误: {}", response2.trim()),
                                    start.elapsed(),
                                )
                            }
                        }
                        Err(e) => TestResult::failure(test_name, e, start.elapsed()),
                    }
                } else {
                    TestResult::failure(
                        test_name,
                        format!("DELETE 存在键响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试 FLUSH_ALL 命令
    fn test_flush(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "FLUSH_ALL".to_string();
        
        match self.send_command("flush_all\r\n") {
            Ok(response) => {
                if response.trim() == "OK" {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("FLUSH_ALL 响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 测试未知命令
    fn test_unknown_command(&mut self) -> TestResult {
        let start = Instant::now();
        let test_name = "UNKNOWN_COMMAND".to_string();
        
        match self.send_command("unknown_command\r\n") {
            Ok(response) => {
                if response.starts_with("ERROR") {
                    TestResult::success(test_name, start.elapsed())
                } else {
                    TestResult::failure(
                        test_name,
                        format!("未知命令响应错误: {}", response.trim()),
                        start.elapsed(),
                    )
                }
            }
            Err(e) => TestResult::failure(test_name, e, start.elapsed()),
        }
    }
    
    /// 运行所有测试
    fn run_all_tests(&mut self) -> Vec<TestResult> {
        println!("🧪 开始运行 Memcached 协议测试...");
        
        let mut results = Vec::new();
        
        // 基础命令测试
        results.push(self.test_version());
        results.push(self.test_stats());
        
        // 存储和检索测试
        results.push(self.test_set());
        results.push(self.test_get());
        results.push(self.test_add());
        results.push(self.test_replace());
        
        // 数值操作测试
        results.push(self.test_incr_decr());
        
        // 删除和清空测试
        results.push(self.test_delete());
        results.push(self.test_flush());
        
        // 错误处理测试
        results.push(self.test_unknown_command());
        
        results
    }
}

/// 打印测试结果
fn print_results(results: &[TestResult]) {
    println!("\n📊 测试结果:");
    println!("{:-<60}", "");
    
    let mut total_tests = 0;
    let mut passed_tests = 0;
    let mut total_duration = Duration::new(0, 0);
    
    for result in results {
        total_tests += 1;
        total_duration += result.duration;
        
        let status = if result.success {
            passed_tests += 1;
            "✅ PASS"
        } else {
            "❌ FAIL"
        };
        
        println!(
            "{:<20} {} ({:.2}ms)",
            result.test_name,
            status,
            result.duration.as_secs_f64() * 1000.0
        );
        
        if let Some(error) = &result.error {
            println!("    错误: {}", error);
        }
    }
    
    println!("{:-<60}", "");
    println!(
        "总计: {}/{} 通过, 总耗时: {:.2}ms",
        passed_tests,
        total_tests,
        total_duration.as_secs_f64() * 1000.0
    );
    
    if passed_tests == total_tests {
        println!("🎉 所有测试通过！");
    } else {
        println!("💥 有 {} 个测试失败", total_tests - passed_tests);
    }
}

fn main() {
    println!("🚀 RatMemcached 功能测试工具");
    println!("📋 测试 Memcached 协议的完整功能");
    
    // 解析命令行参数
    let mut config = TestConfig::default();
    if let Some(addr) = std::env::args().nth(1) {
        config.server_addr = addr;
    }
    
    println!("⚙️ 测试配置:");
    println!("  - 服务器地址: {}", config.server_addr);
    println!("  - 超时时间: {:?}", config.timeout);
    
    // 连接到服务器
    let mut client = match MemcachedTestClient::connect(config) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("❌ 连接服务器失败: {}", e);
            process::exit(1);
        }
    };
    
    // 运行所有测试
    let results = client.run_all_tests();
    
    // 打印结果
    print_results(&results);
    
    // 检查是否有失败的测试
    let failed_tests: Vec<_> = results.iter().filter(|r| !r.success).collect();
    
    if !failed_tests.is_empty() {
        eprintln!("\n❌ 测试失败，退出代码: 1");
        for failed in failed_tests {
            eprintln!("  - {}: {}", failed.test_name, failed.error.as_ref().unwrap_or(&"未知错误".to_string()));
        }
        process::exit(1);
    }
    
    println!("\n✅ 所有测试通过，服务器功能正常！");
    process::exit(0);
}