# RatMemCache

High-performance Memcached protocol-compatible server with dual-layer cache and **melange_db** persistent storage

[![Crates.io](https://img.shields.io/crates/v/rat_memcache.svg)](https://crates.io/crates/rat_memcache)
[![Documentation](https://docs.rs/rat_memcache/badge.svg)](https://docs.rs/rat_memcache)
[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)
[![Coverage](https://img.shields.io/codecov/c/github/0ldm0s/rat_memcache)](https://codecov.io/gh/0ldm0s/rat_memcache)
[![Downloads](https://img.shields.io/crates/d/rat_memcache.svg)](https://crates.io/crates/rat_memcache)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://rust-lang.org)

---

🇨🇳 [中文](README.md) | 🇺🇸 [English](README_EN.md) | 🇯🇵 [日本語](README_JA.md)

## Project Description

RatMemCache is a high-performance caching system based on Rust implementation, providing two usage modes:

1. **As a library**: Provides high-performance caching API with memory and **melange_db** persistent dual-layer cache
2. **As a standalone server**: 100% Memcached protocol-compatible standalone server

### 🪟 Native Windows Platform Support

**RatMemCache is one of the few high-performance Memcached-compatible servers that can run natively on Windows!**

- ✅ **Native Windows Support**: No WSL or virtual machine required, runs directly on Windows
- ✅ **100% Protocol Compatibility**: Fully compatible with Memcached protocol, direct replacement for original memcached
- ✅ **Cross-platform Consistency**: Windows, Linux, macOS functionality is completely identical
- ✅ **Solves Windows Pain Points**: Original memcached is complex to deploy on Windows, RatMemCache is ready to use

Licensed under LGPL-v3, supporting free usage and modification.

## Key Features

- 🚀 **High Performance**: Based on async runtime, supports high concurrency
- 📦 **Dual-Layer Cache Architecture**: Memory L1 cache + MelangeDB L2 persistent cache
- 🔌 **100% Memcached Protocol Compatible**: Can directly replace standard memcached server
- 🪟 **Windows Native Support**: No WSL required, runs directly on Windows platform
- 🧠 **Intelligent Eviction Strategies**: Supports LRU, LFU, FIFO, hybrid strategies, etc.
- ⏰ **TTL Support**: Flexible expiration time management
- 🐘 **Large Value Optimization**: Large values exceeding threshold are automatically sent to L2 storage, avoiding memory exhaustion
- 🗜️ **Data Compression**: LZ4 compression algorithm, saves storage space
- 🎨 **Structured Logging**: High-performance logging system based on rat_logger
- 🔧 **Flexible Configuration**: Supports multiple preset configurations and custom configurations

## License

This project is licensed under **LGPL-v3**. This means:

- ✅ Free to use, modify and distribute
- ✅ Can be used in commercial projects
- ✅ Can be linked to your projects
- ⚠️ Modified library source code must be open-sourced under LGPL license
- ⚠️ When linked to your application, the application can remain closed-source

See [LICENSE](LICENSE) file for details.

## Quick Start

### Usage Scenario Selection

RatMemCache provides flexible feature selection to meet different scenario needs:

#### 1. Pure Memory Cache (Default)
```toml
[dependencies]
rat_memcache = "0.2.1"
```
- ✅ Basic memory cache functionality
- ✅ TTL support
- ❌ Persistent storage
- ❌ Performance metrics
- Suitable for: Simple cache scenarios

#### 2. Dual-Layer Cache (Memory + Persistent)
```toml
[dependencies]
rat_memcache = { version = "0.2.1", features = ["full-features"] }
```
- ✅ All library features
- ✅ MelangeDB persistent storage
- ✅ LZ4 compression
- ✅ Performance metrics
- ✅ mimalloc memory allocator
- Suitable for: Production environments requiring persistence

#### 3. Complete Server
```toml
[dependencies]
rat_memcache = { version = "0.2.1", features = ["server"] }
```
- ✅ Includes all library features
- ✅ rat_memcached binary
- Suitable for: Use as standalone memcached server

#### 4. Custom Combination
```toml
[dependencies]
rat_memcache = { version = "0.2.1", features = ["cache-lib", "ttl-support", "metrics"] }
```
- Select specific features as needed
- Minimize dependencies and compilation time

### Using as a Library

RatMemCache can be integrated into your project as a Rust library, providing high-performance dual-layer cache functionality.

#### Basic Integration

```toml
[dependencies]
rat_memcache = "0.2.1"
tokio = { version = "1.0", features = ["full"] }
```

#### Quick Start

```rust
use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create cache instance - use default configuration
    let cache = RatMemCacheBuilder::new()
        .build()
        .await?;

    // Basic operations
    let key = "my_key".to_string();
    let value = Bytes::from("my_value");

    // Set cache
    cache.set(key.clone(), value.clone()).await?;

    // Get cache
    if let Some(retrieved) = cache.get(&key).await? {
        println!("Retrieved: {:?}", retrieved);
    }

    // Set cache with TTL (expires in 60 seconds)
    cache.set_with_ttl("temp_key".to_string(), Bytes::from("temp_value"), 60).await?;

    // Check if cache exists
    let exists = cache.contains_key("temp_key").await?;
    println!("Key exists: {}", exists);

    // Get cache key list
    let keys = cache.keys().await?;
    println!("Cache keys: {:?}", keys);

    // Conditional deletion
    let deleted = cache.delete("temp_key").await?;
    println!("Key deleted: {}", deleted);

    // Graceful shutdown
    cache.shutdown().await?;

    Ok(())
}
```

#### Advanced Configuration

```rust
use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{L1Config, L2Config, TtlConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Custom L1 configuration (2GB memory limit)
    let l1_config = L1Config {
        max_memory: 2 * 1024 * 1024 * 1024,  // 2GB in bytes
        max_entries: 1_000_000,             // 1 million entries
        eviction_strategy: EvictionStrategy::Lru,
    };

    // Custom L2 configuration (10GB disk space)
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

    // TTL configuration
    let ttl_config = TtlConfig {
        default_ttl: Some(3600),     // Default 1 hour
        max_ttl: 86400,              // Maximum 24 hours
        cleanup_interval: 300,       // Clean up every 5 minutes
        ..Default::default()
    };

    let cache = RatMemCacheBuilder::new()
        .l1_config(l1_config)
        .l2_config(l2_config)
        .ttl_config(ttl_config)
        .build()
        .await?;

    // Use cache...

    Ok(())
}
```

#### Production Best Practices

```rust
use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{L1Config, L2Config, PerformanceConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Production configuration - optimized performance configuration
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
            ..Default::default()
        })
        .build()
        .await?;

    // Application main logic...

    Ok(())
}
```

### Using as Standalone Server

```bash
# Clone project
git clone https://github.com/0ldm0s/rat_memcache.git
cd rat_memcache

# Build (enable server functionality)
cargo build --release

# Start server with default configuration
cargo run --bin rat_memcached

# Specify binding address
cargo run --bin rat_memcached -- --bind 0.0.0.0:11211

# Use configuration file
cargo run --bin rat_memcached -- --config custom_config.toml

# Run as daemon
cargo run --bin rat_memcached -- --daemon --pid-file /var/run/rat_memcached.pid
```

#### Windows Platform Special Notes

On Windows platform, RatMemCache provides completely consistent functionality with Linux/macOS:

```powershell
# Windows build
cargo build --release

# Windows start server
cargo run --bin rat_memcached

# Windows specify port
cargo run --bin rat_memcached -- --bind 127.0.0.1:11211

# Windows background run (using PowerShell Start-Process)
Start-Process cargo -ArgumentList "run --bin rat_memcached -- --bind 0.0.0.0:11211" -NoNewWindow
```

**Windows Advantages**:
- No need to install WSL or virtual machine
- Native performance, no virtualization overhead
- Perfect integration with Windows services
- Support for Windows native paths and permission management

### Protocol Compatibility

RatMemCache is fully compatible with Memcached protocol, supporting the following commands:

- `get` / `gets` - Get data
- `set` / `add` / `replace` / `append` / `prepend` / `cas` - Set data
- `delete` - Delete data
- `incr` / `decr` - Increment/decrement values
- `flush_all` - Clear all data
- `version` - Get version information

You can use any standard Memcached client to connect to RatMemCache server:

```bash
# Test with telnet
telnet 127.0.0.1 11211

# Use memcached-cli
memcached-cli --server 127.0.0.1:11211
```

## Configuration

The project uses TOML format configuration files, supporting flexible configuration options:

### Basic Configuration

```toml
[l1]
max_memory = 1073741824  # 1GB
max_entries = 100000
eviction_strategy = "Lru"

[l2]
enable_l2_cache = true
data_dir = "./rat_memcache_data"
max_disk_size = 1073741824  # 1GB
enable_compression = true

[compression]
enable_lz4 = true
compression_threshold = 1024
compression_level = 6

[ttl]
default_ttl = 3600  # 1 hour
cleanup_interval = 300  # 5 minutes

[performance]
worker_threads = 4
enable_concurrency = true
read_write_separation = true
large_value_threshold = 10240  # 10KB
```

### Advanced Logging Configuration

RatMemCache provides flexible logging configuration based on rat_logger, supporting performance tuning:

```toml
[logging]
# Basic logging configuration
level = "INFO"                    # Log level: trace, debug, info, warn, error, off
enable_colors = true               # Enable colored output
show_timestamp = true              # Show timestamp
enable_performance_logs = true     # Enable performance logs
enable_audit_logs = true           # Enable operation audit logs
enable_cache_logs = true           # Enable cache operation logs

# Advanced logging configuration (performance tuning)
enable_logging = true               # Whether to completely disable logging system (set to false for highest performance)
enable_async = false               # Whether to enable async mode (async mode can improve performance but may lose logs on program crash)

# Batch configuration for async mode (only effective when enable_async=true)
batch_size = 2048                  # Batch size (bytes)
batch_interval_ms = 25             # Batch time interval (milliseconds)
buffer_size = 16384                # Buffer size (bytes)
```

#### Logging Performance Tuning Recommendations

1. **Highest Performance Mode** (suitable for production environment):
   ```toml
   [logging]
   enable_logging = false
   ```

2. **Async High Performance Mode** (suitable for high-load scenarios):
   ```toml
   [logging]
   enable_logging = true
   enable_async = true
   batch_size = 4096
   batch_interval_ms = 50
   buffer_size = 32768
   ```

3. **Debug Mode** (development environment):
   ```toml
   [logging]
   enable_logging = true
   enable_async = false
   level = "DEBUG"
   enable_performance_logs = true
   enable_cache_logs = true
   ```

#### Configuration Description

- **enable_logging**: Switch to completely disable logging system, when set to false all logging functions will be disabled, providing highest performance
- **enable_async**: Async mode switch, async mode can improve performance but may lose logs on program crash
- **batch_size**: Batch size in async mode, affecting logging processing efficiency
- **batch_interval_ms**: Batch time interval in async mode, affecting logging real-time performance
- **buffer_size**: Buffer size in async mode, affecting memory usage

## Build and Test

```bash
# Build project
cargo build

# Build release version
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Check code formatting
cargo fmt

# Check code quality
cargo clippy
```

## Features

### Cache Features
- ✅ Basic cache operations (get/set/delete)
- ✅ TTL expiration management
- ✅ Batch operation support
- ✅ Conditional operations (cas)
- ✅ Data compression

### Protocol Support
- ✅ Complete Memcached protocol implementation
- ✅ Binary protocol support
- ✅ ASCII protocol support
- ✅ Multi-connection handling
- ✅ Concurrent access control

### Performance Features
- ✅ Asynchronous I/O
- ✅ Read-write separation
- ✅ Memory pool management
- ✅ Smart cache warm-up
- ✅ High-performance async design

### Reliability
- ✅ Data persistence
- ✅ Graceful shutdown
- ✅ Error recovery
- ✅ Memory protection

## Architecture Design

```
┌─────────────────────────────────────────────────────────┐
│                    RatMemCache                          │
├─────────────────┬───────────────────────────────────────┤
│   Server Layer   │          Library Interface           │
│  (Memcached     │         (Rust API)                   │
│   Protocol)     │                                       │
├─────────────────┴───────────────────────────────────────┤
│                     Core Layer                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   L1 Cache  │  │   TTL Mgmt  │  │ Streaming   │    │
│  │   (Memory)  │  │             │  │             │    │
│  └─────────────┘  └─────────────┘  └─────────────┘    │
├─────────────────────────────────────────────────────────┤
│                  Storage Layer                          │
│  ┌─────────────────────────────────────────────────┐  │
│  │              MelangeDB L2 Cache                 │  │
│  │           (Persistent Storage)                   │  │
│  └─────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Performance Benchmarks

In standard test environment (4-core CPU, 8GB memory):

- **QPS**: 50,000+ (simple get operations)
- **Memory Usage**: < 50MB base footprint
- **Concurrent Connections**: 10,000+
- **Latency**: < 1ms (99th percentile)

## ⚠️ Large Value Data Transfer Warning

**Important Reminder**: When transferring large values exceeding 40KB, standard memcached protocol may encounter socket buffer limitations, causing transfer timeouts or incomplete transfers.

### Recommended Solution

RatMemCache provides **enhanced streaming protocol** that can effectively solve large value transfer problems:

#### Streaming GET Command
```bash
# Standard GET (may timeout)
get large_key

# Streaming GET (recommended)
streaming_get large_key 16384  # 16KB chunk size
```

#### Streaming Protocol Advantages
- 🚀 **Avoid Timeouts**: Chunked transfer bypasses socket buffer limitations
- 📊 **Progress Visibility**: Real-time display of transfer progress and chunk information
- 💾 **Memory Friendly**: Clients can process data chunks on demand
- 🔧 **Backward Compatible**: Fully compatible with standard memcached protocol

#### Usage Example
```python
# See demo/streaming_protocol_demo.py - Complete performance comparison demo
```

### Detailed Description
- **Problem Threshold**: Data >40KB may trigger socket buffer limitations
- **Recommended Practice**: Use streaming protocol for large value transfers
- **Performance Improvement**: Streaming transfer is 10-100x faster than traditional methods (for large values)

## Dependencies

Main dependencies:
- **tokio**: Async runtime
- **melange_db**: Persistent storage (optional) - High-performance embedded database
- **dashmap**: Concurrent hash table
- **lz4**: Data compression
- **rat_logger**: Logging system
- **clap**: Command line argument parsing
- **mimalloc**: High-performance memory allocator

## Version Compatibility

- **Rust**: 1.70+ (edition 2021)
- **Operating Systems**: Linux, macOS, Windows (fully native support)
- **Memcached Protocol**: 1.4.0+
- **Windows Features**: Native support, no WSL or virtual machine required

## Contribution Guide

Contributions are welcome! Please follow these steps:

1. Fork this project
2. Create feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to branch (`git push origin feature/AmazingFeature`)
5. Create Pull Request

## Maintainers

- [@0ldm0s](https://github.com/0ldm0s) - Main developer

## Acknowledgments

Thanks to the following open source projects:
- [Tokio](https://tokio.rs/) - Async runtime
- [melange_db](https://github.com/melange-db/melange_db) - High-performance embedded persistent storage
- [Rust](https://www.rust-lang.org/) - Programming language

## Roadmap

- [ ] Enhanced cluster support
- [ ] Add more eviction strategies
- [ ] Redis protocol support
- [ ] Web management interface

## License Details

This project is licensed under **GNU Lesser General Public License v3.0 or later (LGPL-3.0-or-later)**.

This means:
- You can link this library to any type of software (including closed-source software)
- When modifying this library source code, modified versions must be released under the same license
- Applications using this library can maintain their own license

See [LICENSE](LICENSE) file for details.