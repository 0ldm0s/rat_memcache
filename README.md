# RatMemCache

高性能 Memcached 协议兼容服务器，支持双层缓存和 **melange_db** 持久化存储

## 项目描述

RatMemCache 是一个基于 Rust 实现的高性能缓存系统，提供了以下两种使用模式：

1. **作为库使用**：提供高性能的缓存 API，支持内存和 **melange_db** 持久化双层缓存
2. **作为独立服务器使用**：100% 兼容 Memcached 协议的独立服务器

### 🪟 Windows 平台原生支持

**RatMemCache 是目前少数能在 Windows 平台原生运行的高性能 Memcached 兼容服务器！**

- ✅ **原生 Windows 支持**：无需 WSL 或虚拟机，直接在 Windows 上运行
- ✅ **100% 协议兼容**：完全兼容 Memcached 协议，可直接替换原版 memcached
- ✅ **跨平台一致性**：Windows、Linux、macX 功能完全一致
- ✅ **解决 Windows 痛点**：原版 memcached 在 Windows 上部署复杂，RatMemCache 开箱即用

采用 LGPL-v3 许可证，支持自由使用和修改。

## 主要特性

- 🚀 **高性能**: 基于异步运行时，支持高并发访问
- 📦 **双层缓存架构**: 内存 L1 缓存 + MelangeDB L2 持久化缓存
- 🔌 **100% Memcached 协议兼容**: 可直接替换标准的 memcached 服务器
- 🪟 **Windows 原生支持**: 无需 WSL，直接在 Windows 平台运行
- 🧠 **智能驱逐策略**: 支持 LRU、LFU、FIFO、混合策略等
- ⏰ **TTL 支持**: 灵活的过期时间管理
- 🗜️ **数据压缩**: LZ4 压缩算法，节省存储空间
- 📊 **高性能指标**: 读写分离指标系统
- 🎨 **结构化日志**: 基于 rat_logger 的高性能日志系统
- 🔧 **灵活配置**: 支持多种预设配置和自定义配置

## 许可证

本项目采用 **LGPL-v3** 许可证。这意味着：

- ✅ 可以自由使用、修改和分发
- ✅ 可以在商业项目中使用
- ✅ 可以作为库链接到你的项目中
- ⚠️ 修改后的库源代码需要以 LGPL 许可证开源
- ⚠️ 链接到你的应用程序时，应用程序可以保持闭源

详见 [LICENSE](LICENSE) 文件。

## 快速开始

### 作为库使用

RatMemCache 可以作为 Rust 库集成到你的项目中，提供高性能的双层缓存功能。

#### 基本集成

```toml
[dependencies]
rat_memcache = { version = "0.2.0", features = ["cache-lib"] }
tokio = { version = "1.0", features = ["full"] }
```

#### 快速开始

```rust
use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建缓存实例 - 使用默认配置
    let cache = RatMemCacheBuilder::new()
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

    // 设置带 TTL 的缓存（60秒过期）
    cache.set_with_ttl("temp_key".to_string(), Bytes::from("temp_value"), 60).await?;

    // 检查缓存是否存在
    let exists = cache.contains_key("temp_key").await?;
    println!("Key exists: {}", exists);

    // 获取缓存键列表
    let keys = cache.keys().await?;
    println!("Cache keys: {:?}", keys);

    // 条件删除
    let deleted = cache.delete("temp_key").await?;
    println!("Key deleted: {}", deleted);

    // 优雅关闭
    cache.shutdown().await?;

    Ok(())
}
```

#### 高级配置

```rust
use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{L1Config, L2Config, TtlConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 自定义 L1 配置（2GB 内存限制）
    let l1_config = L1Config {
        max_memory: 2 * 1024 * 1024 * 1024,  // 2GB in bytes
        max_entries: 1_000_000,             // 100万条记录
        eviction_strategy: EvictionStrategy::Lru,
    };

    // 自定义 L2 配置（10GB 磁盘空间）
    let l2_config = L2Config {
        enable_l2_cache: true,
        data_dir: Some(PathBuf::from("./cache_data")),
        clear_on_startup: false,
        max_disk_size: 10 * 1024 * 1024 * 1024,  // 10GB in bytes
        write_buffer_size: 64 * 1024 * 1024,     // 64MB
        max_write_buffer_number: 3,
        block_cache_size: 32 * 1024 * 1024,      // 32MB
        enable_compression: true,
        compression_level: 6,
        background_threads: 2,
        database_engine: Default::default(),
        melange_config: Default::default(),
    };

    // TTL 配置
    let ttl_config = TtlConfig {
        default_ttl: Some(3600),     // 默认1小时
        max_ttl: 86400,              // 最大24小时
        cleanup_interval: 300,       // 5分钟清理一次
        ..Default::default()
    };

    let cache = RatMemCacheBuilder::new()
        .l1_config(l1_config)
        .l2_config(l2_config)
        .ttl_config(ttl_config)
        .build()
        .await?;

    // 使用缓存...

    Ok(())
}
```

#### 生产环境最佳实践

```rust
use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{L1Config, L2Config, PerformanceConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 生产环境配置 - 默认关闭统计以获得最佳性能
    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 4 * 1024 * 1024 * 1024,  // 4GB
            max_entries: 2_000_000,
            eviction_strategy: EvictionStrategy::Lru,
        })
        .l2_config(L2Config {
            enable_l2_cache: true,
            max_disk_size: 50 * 1024 * 1024 * 1024,  // 50GB
            enable_compression: true,
            background_threads: 4,
            ..Default::default()
        })
        .performance_config(PerformanceConfig {
            // 根据需要启用统计（会影响性能）
            enable_background_stats: false,  // 生产环境建议关闭
            stats_interval: 60,               // 统计收集间隔
            ..Default::default()
        })
        .build()
        .await?;

    // 应用程序主逻辑...

    // 在需要时获取统计信息（例如：监控端点、定期报告、调试等）
    if let Some(stats) = get_cache_stats_for_monitoring(&cache).await {
        println!("缓存状态: {} MB 使用, 命中率: {:.1}%",
                 stats.total_memory_usage / 1024 / 1024,
                 stats.hit_rate.unwrap_or(0.0));
    }

    Ok(())
}

/// 示例：监控函数 - 可以集成到你的监控系统中
async fn get_cache_stats_for_monitoring(cache: &rat_memcache::RatMemCache) -> Option<CacheStats> {
    // 仅在需要时获取统计信息，避免频繁调用影响性能
    let stats = cache.get_cache_stats().await;
    let hit_rate = cache.get_hit_rate().await;

    // 返回统计信息用于监控系统
    Some(CacheStats {
        l1_stats: stats.l1_stats,
        l2_stats: stats.l2_stats,
        total_memory_usage: stats.total_memory_usage,
        total_entries: stats.total_entries,
        hit_rate,
    })
}

/// 用于 Prometheus/Grafana 等监控系统的指标导出示例
async fn export_metrics(cache: &rat_memcache::RatMemCache) -> String {
    let stats = cache.get_cache_stats().await;

    format!(
        "rat_memcache_memory_usage_bytes {}\n\
         rat_memcache_total_entries {}\n\
         rat_memcache_l1_entries {}\n\
         rat_memcache_l2_entries {}\n",
        stats.total_memory_usage,
        stats.total_entries,
        stats.l1_stats.entry_count,
        stats.l2_stats.entry_count
    )
}
```

### 作为独立服务器使用

```bash
# 克隆项目
git clone https://github.com/0ldm0s/rat_memcache.git
cd rat_memcache

# 编译（启用服务器功能）
cargo build --release

# 使用默认配置启动服务器
cargo run --bin rat_memcached

# 指定绑定地址
cargo run --bin rat_memcached -- --bind 0.0.0.0:11211

# 使用配置文件
cargo run --bin rat_memcached -- --config custom_config.toml

# 作为守护进程运行
cargo run --bin rat_memcached -- --daemon --pid-file /var/run/rat_memcached.pid
```

#### Windows 平台特别说明

在 Windows 平台上，RatMemCache 提供了与 Linux/macX 完全一致的功能：

```powershell
# Windows 编译
cargo build --release

# Windows 启动服务器
cargo run --bin rat_memcached

# Windows 指定端口
cargo run --bin rat_memcached -- --bind 127.0.0.1:11211

# Windows 后台运行（使用 PowerShell Start-Process）
Start-Process cargo -ArgumentList "run --bin rat_memcached -- --bind 0.0.0.0:11211" -NoNewWindow
```

**Windows 优势**：
- 无需安装 WSL 或虚拟机
- 原生性能，无虚拟化开销
- 与 Windows 服务完美集成
- 支持 Windows 原生路径和权限管理

### 协议兼容性

RatMemCache 完全兼容 Memcached 协议，支持以下命令：

- `get` / `gets` - 获取数据
- `set` / `add` / `replace` / `append` / `prepend` / `cas` - 设置数据
- `delete` - 删除数据
- `incr` / `decr` - 增减数值
- `stats` - 获取统计信息
- `flush_all` - 清空所有数据
- `version` - 获取版本信息

你可以使用任何标准的 Memcached 客户端连接到 RatMemCache 服务器：

```bash
# 使用 telnet 测试
telnet 127.0.0.1 11211

# 使用 memcached-cli
memcached-cli --server 127.0.0.1:11211
```

## 配置说明

项目使用 TOML 格式配置文件，支持灵活的配置选项：

```toml
[cache]
max_memory_mb = 1024
max_entries = 100000
eviction_strategy = "lru"

[melange_db]
data_dir = "./rat_memcache_data"
max_disk_size_mb = 1024
enable_compression = true

[ttl]
default_ttl_seconds = 3600
cleanup_interval_seconds = 300

[performance]
worker_threads = 4
enable_async = true
```

## 构建和测试

```bash
# 构建项目
cargo build

# 构建发布版本
cargo build --release

# 运行测试
cargo test

# 运行基准测试
cargo bench

# 检查代码格式
cargo fmt

# 检查代码质量
cargo clippy
```

## 功能特性

### 缓存功能
- ✅ 基本缓存操作 (get/set/delete)
- ✅ TTL 过期管理
- ✅ 批量操作支持
- ✅ 条件操作 (cas)
- ✅ 数据压缩

### 协议支持
- ✅ 完整的 Memcached 协议实现
- ✅ 二进制协议支持
- ✅ ASCII 协议支持
- ✅ 多连接处理
- ✅ 并发访问控制

### 性能特性
- ✅ 异步 I/O
- ✅ 读写分离
- ✅ 内存池管理
- ✅ 智能缓存预热
- ✅ 性能指标收集

### 可靠性
- ✅ 数据持久化
- ✅ 优雅关闭
- ✅ 错误恢复
- ✅ 内存保护

## 架构设计

```
┌─────────────────────────────────────────────────────────┐
│                    RatMemCache                          │
├─────────────────┬───────────────────────────────────────┤
│   服务器层      │            库接口层                   │
│  (Memcached    │         (Rust API)                   │
│   Protocol)    │                                       │
├─────────────────┴───────────────────────────────────────┤
│                     核心层                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   L1缓存    │  │   TTL管理   │  │  指标收集    │    │
│  │   (内存)    │  │            │  │             │    │
│  └─────────────┘  └─────────────┘  └─────────────┘    │
├─────────────────────────────────────────────────────────┤
│                   存储层                                │
│  ┌─────────────────────────────────────────────────┐  │
│  │               MelangeDB L2 缓存                   │  │
│  │            (持久化存储)                          │  │
│  └─────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## 性能基准

在标准测试环境下（4核CPU，8GB内存）：

- **QPS**: 50,000+ (简单get操作)
- **内存使用**: < 50MB 基础占用
- **并发连接**: 10,000+
- **延迟**: < 1ms (99th percentile)

## 依赖项

主要依赖：
- **tokio**: 异步运行时
- **melange_db**: 持久化存储 (可选) - 高性能嵌入式数据库
- **dashmap**: 并发哈希表
- **lz4**: 数据压缩
- **rat_logger**: 日志系统
- **clap**: 命令行参数解析
- **mimalloc**: 高性能内存分配器

## 版本兼容性

- **Rust**: 1.70+ (edition 2021)
- **操作系统**: Linux, macOS, Windows (完全原生支持)
- **Memcached 协议**: 1.4.0+
- **Windows 特性**: 原生支持，无需 WSL 或虚拟机

## 贡献指南

欢迎贡献代码！请遵循以下步骤：

1. Fork 本项目
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 创建 Pull Request

## 维护者

- [@0ldm0s](https://github.com/0ldm0s) - 主要开发者

## 致谢

感谢以下开源项目：
- [Tokio](https://tokio.rs/) - 异步运行时
- [melange_db](https://github.com/melange-db/melange_db) - 高性能嵌入式持久化存储
- [Rust](https://www.rust-lang.org/) - 编程语言

## 路线图

- [ ] 增强集群支持
- [ ] 添加更多驱逐策略
- [ ] 支持 Redis 协议
- [ ] Web 管理界面
- [ ] 更多监控指标

## 许可证细节

本项目采用 **GNU Lesser General Public License v3.0 or later (LGPL-3.0-or-later)** 许可证。

这意味着：
- 你可以将本库链接到任何类型的软件中（包括闭源软件）
- 修改本库源代码时，必须以相同的许可证发布修改后的版本
- 使用本库的应用程序可以保持自己的许可证

详见 [LICENSE](LICENSE) 文件。