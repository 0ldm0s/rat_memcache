# RatMemCache

**melange_db**永続化ストレージを備えた高性能Memcachedプロトコル互換サーバー、デュアルレイヤーキャッシュをサポート

[![Crates.io](https://img.shields.io/crates/v/rat_memcache.svg)](https://crates.io/crates/rat_memcache)
[![Documentation](https://docs.rs/rat_memcache/badge.svg)](https://docs.rs/rat_memcache)
[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)
[![Coverage](https://img.shields.io/codecov/c/github/0ldm0s/rat_memcache)](https://codecov.io/gh/0ldm0s/rat_memcache)
[![Downloads](https://img.shields.io/crates/d/rat_memcache.svg)](https://crates.io/crates/rat_memcache)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://rust-lang.org)

---

🇨🇳 [中文](README.md) | 🇺🇸 [English](README_EN.md) | 🇯🇵 [日本語](README_JA.md)

## プロジェクト概要

RatMemCacheはRustベースの高性能キャッシュシステムで、以下の2つの使用モードを提供します：

1. **ライブラリとして使用**: メモリと**melange_db**永続化デュアルレイヤーキャッシュを備えた高性能キャッシュAPIを提供
2. **スタンドアロンサーバーとして使用**: 100% Memcachedプロトコル互換のスタンドアロンサーバー

### 🪟 Windowsプラットフォームネイティブサポート

**RatMemCacheはWindowsプラットフォームでネイティブに実行できる数少ない高性能Memcached互換サーバーです！**

- ✅ **ネイティブWindowsサポート**: WSLや仮想マシン不要、Windows上で直接実行
- ✅ **100%プロトコル互換**: 完全なMemcachedプロトコル互換、標準のmemcachedと直接置き換え可能
- ✅ **クロスプラットフォーム一貫性**: Windows、Linux、macOSで機能が完全に同一
- ✅ **Windowsの課題解決**: 元のmemcachedはWindowsでの展開が複雑、RatMemCacheはすぐに使用可能

LGPL-v3ライセンスで提供、自由な使用と変更をサポート。

## 主要機能

- 🚀 **高性能**: 非同期ランタイムベース、高同時実行アクセスをサポート
- 📦 **デュアルレイヤーキャッシュアーキテクチャ**: メモリL1キャッシュ + MelangeDB L2永続化キャッシュ
- 🔌 **100% Memcachedプロトコル互換**: 標準のmemcachedサーバーと直接置き換え可能
- 🪟 **Windowsネイティブサポート**: WSL不要、Windowsプラットフォームで直接実行
- 🧠 **インテリジェントな追い出し戦略**: LRU、LFU、FIFO、ハイブリッド戦略などをサポート
- ⏰ **TTLサポート**: 柔軟な有効期限管理
- 🐘 **大容量値処理最適化**: しきい値を超える大きな値は自動的にL2ストレージに移行、メモリ枯渇を防止
- 🗜️ **データ圧縮**: LZ4圧縮アルゴリズム、ストレージ容量を節約
- 🎨 **構造化ロギング**: rat_loggerベースの高性能ロギングシステム
- 🔧 **柔軟な設定**: 複数のプリセット設定とカスタム設定をサポート

## ライセンス

このプロジェクトは**LGPL-v3**ライセンスで提供されています。これは以下を意味します：

- ✅ 自由に使用、変更、配布可能
- ✅ 商用プロジェクトで使用可能
- ✅ プロジェクトにリンク可能
- ⚠️ 変更されたライブラリソースコードはLGPLライセンスでオープンソース化する必要あり
- ⚠️ アプリケーションにリンクする場合、アプリケーションはクローズソースを維持可能

詳細は[LICENSE](LICENSE)ファイルを参照してください。

## クイックスタート

### 使用シナリオの選択

RatMemCacheは柔軟な機能選択を提供し、さまざまなシナリオのニーズに対応します：

#### 1. 純粋メモリキャッシュ（デフォルト）
```toml
[dependencies]
rat_memcache = "0.2.1"
```
- ✅ 基本的なメモリキャッシュ機能
- ✅ TTLサポート
- ❌ 永続化ストレージ
- ❌ パフォーマンスメトリクス
- 適合: シンプルなキャッシュシナリオ

#### 2. デュアルレイヤーキャッシュ（メモリ+永続化）
```toml
[dependencies]
rat_memcache = { version = "0.2.1", features = ["full-features"] }
```
- ✅ すべてのライブラリ機能
- ✅ MelangeDB永続化ストレージ
- ✅ LZ4圧縮
- ✅ パフォーマンスメトリクス
- ✅ mimallocメモリアロケーター
- 適合: 永続化が必要な本番環境

#### 3. 完全サーバー
```toml
[dependencies]
rat_memcache = { version = "0.2.1", features = ["server"] }
```
- ✅ すべてのライブラリ機能を含む
- ✅ rat_memcachedバイナリ
- 適合: スタンドアロンのmemcachedサーバーとして使用

#### 4. カスタム組み合わせ
```toml
[dependencies]
rat_memcache = { version = "0.2.1", features = ["cache-lib", "ttl-support", "metrics"] }
```
- 必要に応じて特定機能を選択
- 依存関係とコンパイル時間を最小化

### ライブラリとして使用

RatMemCacheはRustライブラリとしてプロジェクトに統合でき、高性能なデュアルレイヤーキャッシュ機能を提供します。

#### 基本的な統合

```toml
[dependencies]
rat_memcache = "0.2.1"
tokio = { version = "1.0", features = ["full"] }
```

#### クイックスタート

```rust
use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // キャッシュインスタンスの作成 - デフォルト設定を使用
    let cache = RatMemCacheBuilder::new()
        .build()
        .await?;

    // 基本的な操作
    let key = "my_key".to_string();
    let value = Bytes::from("my_value");

    // キャッシュの設定
    cache.set(key.clone(), value.clone()).await?;

    // キャッシュの取得
    if let Some(retrieved) = cache.get(&key).await? {
        println!("取得値: {:?}", retrieved);
    }

    // TTL付きキャッシュの設定（60秒後期限切れ）
    cache.set_with_ttl("temp_key".to_string(), Bytes::from("temp_value"), 60).await?;

    // キャッシュが存在するか確認
    let exists = cache.contains_key("temp_key").await?;
    println!("キーの存在: {}", exists);

    // キャッシュキーリストの取得
    let keys = cache.keys().await?;
    println!("キャッシュキー: {:?}", keys);

    // 条件付き削除
    let deleted = cache.delete("temp_key").await?;
    println!("キー削除: {}", deleted);

    // グレースフルシャットダウン
    cache.shutdown().await?;

    Ok(())
}
```

#### 高度な設定

```rust
use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{L1Config, L2Config, TtlConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // カスタムL1設定（2GBメモリ制限）
    let l1_config = L1Config {
        max_memory: 2 * 1024 * 1024 * 1024,  // 2GB in bytes
        max_entries: 1_000_000,             // 100万エントリ
        eviction_strategy: EvictionStrategy::Lru,
    };

    // カスタムL2設定（10GBディスク容量）
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

    // TTL設定
    let ttl_config = TtlConfig {
        default_ttl: Some(3600),     // デフォルト1時間
        max_ttl: 86400,              // 最大24時間
        cleanup_interval: 300,       // 5分ごとにクリーンアップ
        ..Default::default()
    };

    let cache = RatMemCacheBuilder::new()
        .l1_config(l1_config)
        .l2_config(l2_config)
        .ttl_config(ttl_config)
        .build()
        .await?;

    // キャッシュの使用...

    Ok(())
}
```

#### 本番環境のベストプラクティス

```rust
use rat_memcache::{RatMemCacheBuilder, EvictionStrategy};
use rat_memcache::config::{L1Config, L2Config, PerformanceConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 本番環境設定 - パフォーマンス設定を最適化
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

    // アプリケーションのメインロジック...

    Ok(())
}
```

### スタンドアロンサーバーとして使用

```bash
# プロジェクトをクローン
git clone https://github.com/0ldm0s/rat_memcache.git
cd rat_memcache

# ビルド（サーバー機能を有効化）
cargo build --release

# デフォルト設定でサーバーを起動
cargo run --bin rat_memcached

# バインドアドレスを指定
cargo run --bin rat_memcached -- --bind 0.0.0.0:11211

# 設定ファイルを使用
cargo run --bin rat_memcached -- --config custom_config.toml

# デーモンとして実行
cargo run --bin rat_memcached -- --daemon --pid-file /var/run/rat_memcached.pid
```

#### Windowsプラットフォームの特記事項

Windowsプラットフォームでは、RatMemCacheはLinux/macOSと完全に一貫した機能を提供します：

```powershell
# Windowsビルド
cargo build --release

# Windowsサーバー起動
cargo run --bin rat_memcached

# Windowsポート指定
cargo run --bin rat_memcached -- --bind 127.0.0.1:11211

# Windowsバックグラウンド実行（PowerShell Start-Process使用）
Start-Process cargo -ArgumentList "run --bin rat_memcached -- --bind 0.0.0.0:11211" -NoNewWindow
```

**Windowsの利点**:
- WSLや仮想マシンのインストール不要
- ネイティブパフォーマンス、仮想化オーバーヘッドなし
- Windowsサービスとの完全な統合
- Windowsネイティブパスと権限管理のサポート

### プロトコル互換性

RatMemCacheはMemcachedプロトコルと完全互換性があり、以下のコマンドをサポートします：

- `get` / `gets` - データ取得
- `set` / `add` / `replace` / `append` / `prepend` / `cas` - データ設定
- `delete` - データ削除
- `incr` / `decr` - 数値の増減
- `flush_all` - 全データクリア
- `version` - バージョン情報取得

標準のMemcachedクライアントを使用してRatMemCacheサーバーに接続できます：

```bash
# telnetでテスト
telnet 127.0.0.1 11211

# memcached-cliを使用
memcached-cli --server 127.0.0.1:11211
```

## 設定説明

プロジェクトはTOML形式の設定ファイルを使用し、柔軟な設定オプションをサポートします：

### 基本設定

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
default_ttl = 3600  # 1時間
cleanup_interval = 300  # 5分

[performance]
worker_threads = 4
enable_concurrency = true
read_write_separation = true
large_value_threshold = 10240  # 10KB
```

### 高度なロギング設定

RatMemCacheはrat_loggerベースの柔軟なロギング設定を提供し、パフォーマンスチューニングをサポートします：

```toml
[logging]
# 基本ロギング設定
level = "INFO"                    # ログレベル: trace, debug, info, warn, error, off
enable_colors = true               # カラー出力を有効化
show_timestamp = true              # タイムスタンプを表示
enable_performance_logs = true     # パフォーマンスログを有効化
enable_audit_logs = true           # 操作監査ログを有効化
enable_cache_logs = true           # キャッシュ操作ログを有効化

# 高度なロギング設定（パフォーマンスチューニング）
enable_logging = true               # ロギングシステムを完全に無効化するかどうか（false設定で最高パフォーマンス）
enable_async = false               # 非同期モードを有効化するかどうか（非同期モードはパフォーマンス向上するが、プログラムクラッシュ時にログが失われる可能性あり）

# 非同期モードのバッチ設定（enable_async=true時のみ有効）
batch_size = 2048                  # バッチサイズ（バイト）
batch_interval_ms = 25             # バッチ時間間隔（ミリ秒）
buffer_size = 16384                # バッファサイズ（バイト）
```

#### ロギングパフォーマンスチューニング推奨

1. **最高パフォーマンスモード**（本番環境向け）：
   ```toml
   [logging]
   enable_logging = false
   ```

2. **非同期高性能モード**（高負荷シナリオ向け）：
   ```toml
   [logging]
   enable_logging = true
   enable_async = true
   batch_size = 4096
   batch_interval_ms = 50
   buffer_size = 32768
   ```

3. **デバッグモード**（開発環境）：
   ```toml
   [logging]
   enable_logging = true
   enable_async = false
   level = "DEBUG"
   enable_performance_logs = true
   enable_cache_logs = true
   ```

#### 設定説明

- **enable_logging**: ロギングシステムを完全に無効化するスイッチ、false設定時に全てのロギング機能が無効化され、最高パフォーマンスを提供
- **enable_async**: 非同期モードスイッチ、非同期モードはパフォーマンス向上するがプログラムクラッシュ時にログが失われる可能性あり
- **batch_size**: 非同期モードでのバッチサイズ、ログ処理効率に影響
- **batch_interval_ms**: 非同期モードでのバッチ時間間隔、ログリアルタイム性に影響
- **buffer_size**: 非同期モードでのバッファサイズ、メモリ使用量に影響

## ビルドとテスト

```bash
# プロジェクトをビルド
cargo build

# リリースバージョンをビルド
cargo build --release

# テストを実行
cargo test

# ベンチマークを実行
cargo bench

# コードフォーマットをチェック
cargo fmt

# コード品質をチェック
cargo clippy
```

## 機能特性

### キャッシュ機能
- ✅ 基本的なキャッシュ操作 (get/set/delete)
- ✅ TTL有効期限管理
- ✅ バッチ操作サポート
- ✅ 条件付き操作 (cas)
- ✅ データ圧縮

### プロトコルサポート
- ✅ 完全なMemcachedプロトコル実装
- ✅ バイナリプロトコルサポート
- ✅ ASCIIプロトコルサポート
- ✅ 複数接続処理
- ✅ 同時実行アクセス制御

### パフォーマンス特性
- ✅ 非同期I/O
- ✅ 読み書き分離
- ✅ メモリプール管理
- ✅ スマートキャッシュウォームアップ
- ✅ 高性能非同期設計

### 信頼性
- ✅ データ永続化
- ✅ グレースフルシャットダウン
- ✅ エラー回復
- ✅ メモリ保護

## アーキテクチャ設計

```
┌─────────────────────────────────────────────────────────┐
│                    RatMemCache                          │
├─────────────────┬───────────────────────────────────────┤
│    サーバー層    │          ライブラリインターフェース     │
│  (Memcached     │         (Rust API)                   │
│   プロトコル)    │                                       │
├─────────────────┴───────────────────────────────────────┤
│                     コア層                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   L1キャッシュ│  │   TTL管理   │  │  ストリーミング│    │
│  │   (メモリ)   │  │            │  │             │    │
│  └─────────────┘  └─────────────┘  └─────────────┘    │
├─────────────────────────────────────────────────────────┤
│                  ストレージ層                           │
│  ┌─────────────────────────────────────────────────┐  │
│  │              MelangeDB L2 キャッシュ              │  │
│  │            (永続化ストレージ)                    │  │
│  └─────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## パフォーマンスベンチマーク

標準テスト環境（4コアCPU、8GBメモリ）で：

- **QPS**: 50,000+ (シンプルなget操作)
- **メモリ使用量**: < 50MB ベースフットプリント
- **同時接続数**: 10,000+
- **レイテンシ**: < 1ms (99パーセンタイル)

## ⚠️ 大容量データ転送警告

**重要な注意**: 40KBを超える大容量値を転送する場合、標準のmemcachedプロトコルはソケットバッファ制限に遭遇し、転送タイムアウトまたは不完全な転送を引き起こす可能性があります。

### 推奨ソリューション

RatMemCacheは**強化型ストリーミングプロトコル**を提供し、大容量値転送問題を効果的に解決します：

#### ストリーミングGETコマンド
```bash
# 標準GET（タイムアウト可能性）
get large_key

# ストリーミングGET（推奨）
streaming_get large_key 16384  # 16KBチャンクサイズ
```

#### ストリーミングプロトコルの利点
- 🚀 **タイムアウト回避**: チャンク転送によりソケットバッファ制限をバイパス
- 📊 **進捗可視化**: 転送進捗とチャンク情報のリアルタイム表示
- 💾 **メモリフレンドリー**: クライアントはオンデマンドでデータチャンクを処理可能
- 🔧 **後方互換性**: 標準memcachedプロトコルと完全互換

#### 使用例
```python
# demo/streaming_protocol_demo.pyを参照 - 完全なパフォーマンス比較デモ
```

### 詳細説明
- **問題しきい値**: >40KBのデータはソケットバッファ制限をトリガーする可能性あり
- **推奨プラクティス**: 大容量値転送にはストリーミングプロトコルを使用
- **パフォーマンス向上**: ストリーミング転送は従来方式より10-100倍高速（大容量値に対して）

## 依存関係

主要な依存関係：
- **tokio**: 非同期ランタイム
- **melange_db**: 永続化ストレージ（オプション） - 高性能組み込みデータベース
- **dashmap**: 同時実行ハッシュテーブル
- **lz4**: データ圧縮
- **rat_logger**: ロギングシステム
- **clap**: コマンドライン引数解析
- **mimalloc**: 高性能メモリアロケーター

## バージョン互換性

- **Rust**: 1.70+ (edition 2021)
- **オペレーティングシステム**: Linux, macOS, Windows（完全ネイティブサポート）
- **Memcachedプロトコル**: 1.4.0+
- **Windows機能**: ネイティブサポート、WSLや仮想マシン不要

## コントリビューションガイド

コントリビューションを歓迎します！以下の手順に従ってください：

1. このプロジェクトをFork
2. 機能ブランチを作成 (`git checkout -b feature/AmazingFeature`)
3. 変更をコミット (`git commit -m 'Add some AmazingFeature'`)
4. ブランチにプッシュ (`git push origin feature/AmazingFeature`)
5. Pull Requestを作成

## メンテナー

- [@0ldm0s](https://github.com/0ldm0s) - メイン開発者

## 謝辞

以下のオープンソースプロジェクトに感謝：
- [Tokio](https://tokio.rs/) - 非同期ランタイム
- [melange_db](https://github.com/melange-db/melange_db) - 高性能組み込み永続化ストレージ
- [Rust](https://www.rust-lang.org/) - プログラミング言語

## ロードマップ

- [ ] 拡張クラスターサポート
- [ ] さらに多くの追い出し戦略の追加
- [ ] Redisプロトコルサポート
- [ ] Web管理インターフェース

## ライセンス詳細

このプロジェクトは**GNU Lesser General Public License v3.0 or later (LGPL-3.0-or-later)**ライセンスで提供されています。

これは以下を意味します：
- このライブラリを任意のタイプのソフトウェアにリンクできます（クローズソースソフトウェアを含む）
- このライブラリのソースコードを変更する場合、変更されたバージョンは同じライセンスでリリースする必要があります
- このライブラリを使用するアプリケーションは独自のライセンスを維持できます

詳細は[LICENSE](LICENSE)ファイルを参照してください。