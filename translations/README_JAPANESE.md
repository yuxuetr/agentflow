# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **エンタープライズレベルの堅牢性と可観測性を備えたインテリジェントエージェントワークフローを構築するための、モダンで非同期ファーストなRustフレームワーク。**

AgentFlowはPocketFlowのコンセプトにインスパイアされた新しいRustフレームワークで、非同期並行性、可観測性、信頼性パターンを備えた本番対応のワークフローオーケストレーションを提供します。

## 🚀 主要機能

### ⚡ **非同期ファーストアーキテクチャ**

- 高性能な非同期実行のためのTokioランタイム上に構築
- 並列処理とバッチ処理のネイティブサポート
- Rustの所有権モデルによるゼロコスト抽象化
- 安全な並行性のためのSend + Sync準拠

### 🛡️ **エンタープライズレベルの堅牢性**

- **サーキットブレーカー**: 自動障害検出と復旧
- **レート制限**: トラフィック制御のためのスライディングウィンドウアルゴリズム
- **リトライポリシー**: ジッター付き指数バックオフ
- **タイムアウト管理**: 負荷下での優雅な劣化
- **リソースプール**: 安全なリソース管理のためのRAIIガード
- **負荷シェディング**: 適応的容量管理

### 📊 **包括的可観測性**

- フローおよびノードレベルでのリアルタイムメトリクス収集
- タイムスタンプと継続時間を含む構造化イベントログ
- パフォーマンスプロファイリングとボトルネック検出
- 設定可能なアラートシステム
- 分散トレーシングサポート
- 監視プラットフォームとの統合対応

### 🔄 **柔軟な実行モデル**

- **シーケンシャルフロー**: 従来のノード間実行
- **並列実行**: `futures::join_all`を使用した並行ノード処理
- **バッチ処理**: 設定可能なバッチサイズでの並行バッチ実行
- **ネストされたフロー**: 階層的ワークフロー構成
- **条件付きルーティング**: ランタイム状態に基づく動的フロー制御

## 📦 インストール

`Cargo.toml`にAgentFlowを追加してください：

```toml
[dependencies]
agentflow-core = "0.2.0"
tokio = { version = "1.0", features = ["full"] }
```

## 🎯 クイックスタート

### 基本的なシーケンシャルフロー

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;

// カスタムノードを定義
struct GreetingNode {
    name: String,
}

#[async_trait]
impl AsyncNode for GreetingNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        Ok(Value::String(format!("{}への挨拶を準備中", self.name)))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        Ok(Value::String(format!("こんにちは、{}！", self.name)))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("greeting".to_string(), exec);
        Ok(None) // フロー終了
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let node = Box::new(GreetingNode {
        name: "AgentFlow".to_string()
    });

    let flow = AsyncFlow::new(node);
    let shared = SharedState::new();

    let result = flow.run_async(&shared).await?;
    println!("フロー完了: {:?}", result);

    Ok(())
}
```

### 可観測性を備えた並列実行

```rust
use agentflow_core::{AsyncFlow, MetricsCollector};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // 並列実行用のノードを作成
    let nodes = vec![
        Box::new(ProcessingNode { id: "worker_1".to_string() }),
        Box::new(ProcessingNode { id: "worker_2".to_string() }),
        Box::new(ProcessingNode { id: "worker_3".to_string() }),
    ];

    // 可観測性を設定
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("parallel_processing".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await?;

    // メトリクスを確認
    let execution_count = metrics.get_metric("parallel_processing.execution_count");
    println!("実行回数: {:?}", execution_count);

    Ok(())
}
```

### サーキットブレーカーを備えた堅牢なフロー

```rust
use agentflow_core::{CircuitBreaker, TimeoutManager};
use std::time::Duration;

async fn robust_workflow() -> Result<()> {
    // 堅牢性パターンを設定
    let circuit_breaker = CircuitBreaker::new(
        "api_calls".to_string(),
        3, // 障害閾値
        Duration::from_secs(30) // 復旧タイムアウト
    );

    let timeout_manager = TimeoutManager::new(
        "operations".to_string(),
        Duration::from_secs(10) // デフォルトタイムアウト
    );

    // ワークフローロジックで使用
    let result = circuit_breaker.call(async {
        timeout_manager.execute_with_timeout("api_call", async {
            // ビジネスロジックをここに記述
            Ok("成功")
        }).await
    }).await?;

    Ok(())
}
```

## 🏗️ アーキテクチャ

AgentFlowは4つのコアピラーの上に構築されています：

1. **実行モデル**: prep/exec/postライフサイクルを持つAsyncNodeトレイト
2. **並行制御**: 並列、バッチ、ネストされた実行パターン
3. **堅牢性保証**: サーキットブレーカー、リトライ、タイムアウト、リソース管理
4. **可観測性**: メトリクス、イベント、アラート、分散トレーシング

詳細なアーキテクチャ情報については、[docs/design.md](docs/design.md)をご覧ください。

## 📚 ドキュメント

- **[設計ドキュメント](docs/design.md)** - システムアーキテクチャとコンポーネント図
- **[機能仕様](docs/functional-spec.md)** - 機能要件とAPI仕様
- **[ユースケース](docs/use-cases.md)** - 実世界の実装シナリオ
- **[APIリファレンス](docs/api/)** - 完全なAPIドキュメント
- **[移行ガイド](docs/migration.md)** - PocketFlowからのアップグレード

## 🧪 テスト

AgentFlowは包括的なテストスイートで100%のテストカバレッジを維持しています：

```bash
# すべてのテストを実行
cargo test

# 出力付きで実行
cargo test -- --nocapture

# 特定のモジュールテストを実行
cargo test async_flow
cargo test robustness
cargo test observability
```

**現在のステータス**: 67/67 テスト合格 ✅

## 🚢 本番対応

AgentFlowは次の特徴を持つ本番環境向けに設計されています：

- **メモリ安全性**: Rustの所有権モデルがデータ競合とメモリリークを防止
- **パフォーマンス**: ゼロコスト抽象化と効率的な非同期ランタイム
- **信頼性**: 包括的なエラーハンドリングと優雅な劣化
- **スケーラビリティ**: 水平スケーリングパターンの組み込みサポート
- **監視**: 本番インサイトのための完全な可観測性スタック

## 🛣️ ロードマップ

- **v0.3.0**: MCP（Model Context Protocol）統合
- **v0.4.0**: 分散実行エンジン
- **v0.5.0**: WebAssemblyプラグインシステム
- **v1.0.0**: 本番安定性保証

## 🤝 貢献

貢献を歓迎します！ガイドラインについては[CONTRIBUTING.md](CONTRIBUTING.md)をご覧ください。

## 📄 ライセンス

このプロジェクトはMITライセンスの下でライセンスされています - 詳細は[LICENSE](LICENSE)ファイルをご覧ください。

## 🙏 謝辞

- オリジナルのPocketFlowコンセプトの基盤の上に構築
- モダンな分散システムパターンにインスパイア
- RustエコシステムとTokioランタイムによって動力を得ています

---

**AgentFlow**: インテリジェントワークフローとエンタープライズ信頼性が出会う場所。🦀✨