# RatMemCache

高性能双层缓存系统，基于内存缓存和 MelangeDB 持久化存储

## 项目描述

RatMemCache 是一个高性能的 Rust 语言实现的缓存系统，提供内存 L1 缓存和 MelangeDB L2 持久化缓存的双层架构。支持 Memcached 协议兼容的服务器模式。

## 主要特性

- **双层缓存架构**: 内存 L1 缓存 + MelangeDB L2 持久化缓存
- **Memcached 协议兼容**: 完全兼容 Memcached 协议的高性能服务器
- **多种驱逐策略**: 支持 LRU、LFU、FIFO、混合策略等
- **TTL 支持**: 灵活的过期时间管理
- **数据压缩**: LZ4 压缩算法，节省存储空间
- **高性能指标**: 读写分离指标系统
- **结构化日志**: 基于 zerg_creep 的高性能日志系统
- **异步设计**: 全异步 API，支持高并发

## 项目结构

```
src/
├── bin/
│   ├── rat_memcached.rs    # Memcached 协议服务器
│   └── test_memcached.rs   # 测试服务器
├── cache.rs                # 缓存核心实现
├── config.rs               # 配置管理
├── error.rs                # 错误处理
├── lib.rs                  # 库主文件
├── logging.rs              # 日志系统
├── metrics.rs              # 性能指标
├── types.rs                # 类型定义
├── compression.rs          # 压缩模块
├── l1_cache.rs             # L1 内存缓存
├── l2_cache.rs             # L2 持久化缓存
└── ttl.rs                  # TTL 管理
```

## 快速开始

### 作为库使用

```rust
use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建缓存实例
    let cache = RatMemCacheBuilder::new()
        .development_preset()
        .build()
        .await?;

    // 基本操作
    let key = "my_key".to_string();
    let value = Bytes::from("my_value");

    // 设置缓存
    cache.set(key.clone(), value.clone()).await?;

    // 获取缓存
    if let Some(retrieved) = cache.get(&key).await? {
        println!("Retrieved: {:?}", retrieved);
    }

    // 关闭缓存
    cache.shutdown().await?;

    Ok(())
}
```

### 启动服务器

```bash
# 使用默认配置启动服务器
cargo run --bin rat_memcached

# 指定绑定地址
cargo run --bin rat_memcached -- --bind 0.0.0.0:11211
```

## 配置说明

项目使用 TOML 格式配置文件，支持以下配置选项：

- **L1 配置**: 内存缓存大小、驱逐策略、最大条目数
- **L2 配置**: MelangeDB 存储路径、磁盘大小、压缩设置
- **压缩配置**: LZ4 压缩阈值、压缩级别
- **TTL 配置**: 默认过期时间、清理间隔
- **性能配置**: 并发操作数、超时设置
- **日志配置**: 日志级别、输出文件

## 构建和测试

```bash
# 构建项目
cargo build

# 运行测试
cargo test

# 运行基准测试
cargo bench
```

## 依赖项

主要依赖：
- tokio: 异步运行时
- rocksdb: 持久化存储
- dashmap: 并发哈希表
- lz4: 数据压缩
- zerg_creep: 日志系统
- clap: 命令行参数解析

## 开发状态

项目处于活跃开发阶段，主要功能已实现：
- 基本缓存操作 (get/set/delete)
- TTL 过期管理
- 数据压缩
- Memcached 协议支持
- 性能指标收集

## 注意事项

- 需要安装 MelangeDB（作为子模块包含）
- 建议使用 Rust 1.70+ 版本
- 生产环境使用时需要调整默认配置参数