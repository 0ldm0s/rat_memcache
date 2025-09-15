//! 压缩模块
//!
//! 提供基于 LZ4 的高性能数据压缩和解压缩功能

use crate::error::{CacheError, CacheResult};
use crate::config::CompressionConfig;
use bytes::Bytes;
use lz4::{Decoder, EncoderBuilder};
use std::io::{Read, Write};
use std::sync::Arc;

/// 压缩器
#[derive(Debug, Clone)]
pub struct Compressor {
    config: Arc<CompressionConfig>,
}

/// 压缩结果
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// 压缩后的数据
    pub compressed_data: Bytes,
    /// 原始数据大小
    pub original_size: usize,
    /// 压缩后大小
    pub compressed_size: usize,
    /// 压缩比率 (compressed_size / original_size)
    pub compression_ratio: f64,
    /// 是否实际进行了压缩
    pub is_compressed: bool,
}

/// 解压缩结果
#[derive(Debug, Clone)]
pub struct DecompressionResult {
    /// 解压缩后的数据
    pub data: Bytes,
    /// 解压缩后的大小
    pub size: usize,
}

impl Compressor {
    /// 创建新的压缩器
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// 压缩数据
    pub fn compress(&self, data: &[u8]) -> CacheResult<CompressionResult> {
        let original_size = data.len();
        
        // 检查是否需要压缩
        if !self.should_compress(data) {
            return Ok(CompressionResult {
                compressed_data: Bytes::copy_from_slice(data),
                original_size,
                compressed_size: original_size,
                compression_ratio: 1.0,
                is_compressed: false,
            });
        }

        // 执行 LZ4 压缩
        let compressed_data = self.compress_lz4(data)?;
        let compressed_size = compressed_data.len();
        let compression_ratio = compressed_size as f64 / original_size as f64;

        // 检查压缩效果
        if compression_ratio >= self.config.min_compression_ratio {
            // 压缩效果不佳，返回原始数据
            Ok(CompressionResult {
                compressed_data: Bytes::copy_from_slice(data),
                original_size,
                compressed_size: original_size,
                compression_ratio: 1.0,
                is_compressed: false,
            })
        } else {
            // 压缩效果良好，返回压缩数据
            Ok(CompressionResult {
                compressed_data: Bytes::from(compressed_data),
                original_size,
                compressed_size,
                compression_ratio,
                is_compressed: true,
            })
        }
    }

    /// 解压缩数据
    pub fn decompress(&self, compressed_data: &[u8], is_compressed: bool) -> CacheResult<DecompressionResult> {
        if !is_compressed {
            // 数据未压缩，直接返回
            return Ok(DecompressionResult {
                data: Bytes::copy_from_slice(compressed_data),
                size: compressed_data.len(),
            });
        }

        // 执行 LZ4 解压缩
        let decompressed_data = self.decompress_lz4(compressed_data)?;
        let size = decompressed_data.len();

        Ok(DecompressionResult {
            data: Bytes::from(decompressed_data),
            size,
        })
    }

    /// 检查是否应该压缩数据
    fn should_compress(&self, data: &[u8]) -> bool {
        if !self.config.enable_lz4 {
            return false;
        }

        // 检查数据大小阈值
        if data.len() < self.config.compression_threshold {
            return false;
        }

        // 如果启用自动压缩检测，进行简单的熵检测
        if self.config.auto_compression {
            self.estimate_compressibility(data)
        } else {
            true
        }
    }

    /// 估算数据的可压缩性
    /// 使用简单的字节频率分析来估算数据是否值得压缩
    fn estimate_compressibility(&self, data: &[u8]) -> bool {
        if data.len() < 64 {
            return false;
        }

        // 采样前 256 字节进行快速分析
        let sample_size = std::cmp::min(256, data.len());
        let sample = &data[..sample_size];
        
        // 计算字节频率
        let mut freq = [0u32; 256];
        for &byte in sample {
            freq[byte as usize] += 1;
        }

        // 计算唯一字节数
        let unique_bytes = freq.iter().filter(|&&count| count > 0).count();
        
        // 如果唯一字节数太少，可能是重复数据，值得压缩
        // 如果唯一字节数接近 256，可能是随机数据，不值得压缩
        let uniqueness_ratio = unique_bytes as f64 / 256.0;
        
        // 经验值：唯一性比率在 0.1-0.8 之间的数据通常值得压缩
        uniqueness_ratio >= 0.1 && uniqueness_ratio <= 0.8
    }

    /// 执行 LZ4 压缩
    fn compress_lz4(&self, data: &[u8]) -> CacheResult<Vec<u8>> {
        let mut encoder = EncoderBuilder::new()
            .level(self.config.compression_level as u32)
            .build(Vec::new())
            .map_err(|e| CacheError::compression_error(&format!("创建 LZ4 编码器失败: {}", e)))?;

        encoder.write_all(data)
            .map_err(|e| CacheError::compression_error(&format!("LZ4 压缩写入失败: {}", e)))?;

        let (compressed_data, result) = encoder.finish();
        result.map_err(|e| CacheError::compression_error(&format!("LZ4 压缩完成失败: {}", e)))?;

        Ok(compressed_data)
    }

    /// 执行 LZ4 解压缩
    fn decompress_lz4(&self, compressed_data: &[u8]) -> CacheResult<Vec<u8>> {
        let mut decoder = Decoder::new(compressed_data)
            .map_err(|e| CacheError::compression_error(&format!("创建 LZ4 解码器失败: {}", e)))?;

        let mut decompressed_data = Vec::new();
        decoder.read_to_end(&mut decompressed_data)
            .map_err(|e| CacheError::compression_error(&format!("LZ4 解压缩失败: {}", e)))?;

        Ok(decompressed_data)
    }

    /// 获取压缩配置
    pub fn config(&self) -> &CompressionConfig {
        &self.config
    }

    /// 估算压缩后的大小（用于内存预分配）
    pub fn estimate_compressed_size(&self, original_size: usize) -> usize {
        if original_size < self.config.compression_threshold {
            return original_size;
        }

        // 根据压缩级别估算压缩比率
        let estimated_ratio = match self.config.compression_level {
            1..=3 => 0.7,   // 快速压缩
            4..=6 => 0.6,   // 平衡压缩
            7..=9 => 0.5,   // 高压缩
            10..=12 => 0.4, // 最高压缩
            _ => 0.6,       // 默认
        };

        (original_size as f64 * estimated_ratio) as usize
    }
}

/// 压缩统计信息
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// 总压缩次数
    pub total_compressions: u64,
    /// 总解压缩次数
    pub total_decompressions: u64,
    /// 压缩的总原始字节数
    pub total_original_bytes: u64,
    /// 压缩的总压缩字节数
    pub total_compressed_bytes: u64,
    /// 跳过压缩的次数
    pub skipped_compressions: u64,
    /// 压缩失败次数
    pub compression_failures: u64,
    /// 解压缩失败次数
    pub decompression_failures: u64,
}

impl CompressionStats {
    /// 创建新的统计信息
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录压缩操作
    pub fn record_compression(&mut self, result: &CompressionResult) {
        self.total_compressions += 1;
        self.total_original_bytes += result.original_size as u64;
        
        if result.is_compressed {
            self.total_compressed_bytes += result.compressed_size as u64;
        } else {
            self.skipped_compressions += 1;
            self.total_compressed_bytes += result.original_size as u64;
        }
    }

    /// 记录解压缩操作
    pub fn record_decompression(&mut self, _size: usize) {
        self.total_decompressions += 1;
    }

    /// 记录压缩失败
    pub fn record_compression_failure(&mut self) {
        self.compression_failures += 1;
    }

    /// 记录解压缩失败
    pub fn record_decompression_failure(&mut self) {
        self.decompression_failures += 1;
    }

    /// 计算总体压缩比率
    pub fn overall_compression_ratio(&self) -> f64 {
        if self.total_original_bytes == 0 {
            return 1.0;
        }
        self.total_compressed_bytes as f64 / self.total_original_bytes as f64
    }

    /// 计算压缩节省的字节数
    pub fn bytes_saved(&self) -> u64 {
        if self.total_compressed_bytes <= self.total_original_bytes {
            self.total_original_bytes - self.total_compressed_bytes
        } else {
            0
        }
    }

    /// 计算压缩成功率
    pub fn compression_success_rate(&self) -> f64 {
        if self.total_compressions == 0 {
            return 0.0;
        }
        let successful = self.total_compressions - self.compression_failures;
        successful as f64 / self.total_compressions as f64
    }

    /// 计算解压缩成功率
    pub fn decompression_success_rate(&self) -> f64 {
        if self.total_decompressions == 0 {
            return 0.0;
        }
        let successful = self.total_decompressions - self.decompression_failures;
        successful as f64 / self.total_decompressions as f64
    }

    /// 重置统计信息
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CompressionConfig;

    fn create_test_compressor() -> Compressor {
        let config = CompressionConfig {
            enable_lz4: true,
            compression_threshold: 100,
            compression_level: 4,
            auto_compression: true,
            min_compression_ratio: 0.8,
        };
        Compressor::new(config)
    }

    #[test]
    fn test_compress_small_data() {
        let compressor = create_test_compressor();
        let data = b"small";
        
        let result = compressor.compress(data).unwrap();
        assert!(!result.is_compressed);
        assert_eq!(result.compressed_data.as_ref(), data);
    }

    #[test]
    fn test_compress_large_data() {
        let mut config = CompressionConfig {
            enable_lz4: true,
            compression_threshold: 100,
            compression_level: 4,
            auto_compression: false, // 禁用自动检测，强制压缩
            min_compression_ratio: 0.8,
        };
        let compressor = Compressor::new(config);
        let data = b"Hello, World! This is a test string that should be compressed.".repeat(20);
        
        let result = compressor.compress(&data).unwrap();
        assert!(result.is_compressed);
        assert!(result.compressed_size < result.original_size);
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let compressor = create_test_compressor();
        let original_data = b"Hello, World! This is a test string that should be compressed.".repeat(10);
        
        let compress_result = compressor.compress(&original_data).unwrap();
        let decompress_result = compressor.decompress(
            &compress_result.compressed_data,
            compress_result.is_compressed
        ).unwrap();
        
        assert_eq!(decompress_result.data.as_ref(), original_data.as_slice());
    }

    #[test]
    fn test_compression_stats() {
        let mut stats = CompressionStats::new();
        
        let result = CompressionResult {
            compressed_data: Bytes::from(vec![1, 2, 3]),
            original_size: 100,
            compressed_size: 50,
            compression_ratio: 0.5,
            is_compressed: true,
        };
        
        stats.record_compression(&result);
        
        assert_eq!(stats.total_compressions, 1);
        assert_eq!(stats.total_original_bytes, 100);
        assert_eq!(stats.total_compressed_bytes, 50);
        assert_eq!(stats.overall_compression_ratio(), 0.5);
        assert_eq!(stats.bytes_saved(), 50);
    }
}