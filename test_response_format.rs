#[cfg(test)]
mod tests {
    use rat_memcache::bin::rat_memcached::MemcachedResponse;

    #[test]
    fn test_format_large_binary_response() {
        // 创建50KB的二进制数据（包含各种字节值）
        let large_data: Vec<u8> = (0..51200).map(|i| i as u8).collect();

        let response = MemcachedResponse::Value {
            key: "test_key".to_string(),
            flags: 0,
            bytes: large_data.len(),
            data: large_data.clone(),
        };

        // 测试格式化 - 这可能会panic或产生问题
        let formatted = rat_memcache::bin::rat_memcached::RatMemcached::format_response(response);

        println!("Formatted response length: {}", formatted.len());
        assert!(formatted.contains("VALUE test_key 0 51200"));
    }
}