# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **Ein modernes, async-first Rust Framework zum Erstellen intelligenter Agent-Workflows mit Unternehmens-Robustheit und Observabilität.**

AgentFlow ist ein neues Rust Framework, inspiriert von PocketFlow-Konzepten, das produktionsfertige Workflow-Orchestrierung mit asynchroner Nebenläufigkeit, Observabilität und Zuverlässigkeitsmustern bietet.

## 🚀 Hauptmerkmale

### ⚡ **Async-First Architektur**

- Auf Tokio Runtime für hochperformante asynchrone Ausführung aufgebaut
- Native Unterstützung für parallele und Batch-Verarbeitung
- Nullkosten-Abstraktionen mit Rusts Ownership-Modell
- Send + Sync Compliance für sichere Nebenläufigkeit

### 🛡️ **Unternehmens-Robustheit**

- **Circuit Breaker**: Automatische Fehlererkennung und Wiederherstellung
- **Rate Limiting**: Sliding-Window-Algorithmen für Traffic-Kontrolle
- **Retry-Richtlinien**: Exponentieller Backoff mit Jitter
- **Timeout-Management**: Elegante Degradation unter Last
- **Resource Pooling**: RAII-Guards für sichere Ressourcenverwaltung
- **Load Shedding**: Adaptive Kapazitätsverwaltung

### 📊 **Umfassende Observabilität**

- Echtzeit-Metriken-Sammlung auf Flow- und Node-Ebene
- Strukturierte Event-Logs mit Zeitstempeln und Dauern
- Performance-Profiling und Engpass-Erkennung
- Konfigurierbares Alerting-System
- Distributed Tracing Support
- Integrationsbereit für Monitoring-Plattformen

### 🔄 **Flexible Ausführungsmodelle**

- **Sequentielle Flows**: Traditionelle Node-zu-Node-Ausführung
- **Parallele Ausführung**: Nebenläufige Node-Verarbeitung mit `futures::join_all`
- **Batch-Verarbeitung**: Konfigurierbare Batch-Größen mit nebenläufiger Batch-Ausführung
- **Verschachtelte Flows**: Hierarchische Workflow-Komposition
- **Conditional Routing**: Dynamische Flow-Kontrolle basierend auf Runtime-Zustand

## 📦 Installation

Fügen Sie AgentFlow zu Ihrer `Cargo.toml` hinzu:

```toml
[dependencies]
agentflow-core = "0.2.0"
tokio = { version = "1.0", features = ["full"] }
```

## 🎯 Schnellstart

### Grundlegender Sequentieller Flow

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;

// Definiere einen benutzerdefinierten Node
struct GreetingNode {
    name: String,
}

#[async_trait]
impl AsyncNode for GreetingNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        Ok(Value::String(format!("Bereite Begrüßung für {} vor", self.name)))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        Ok(Value::String(format!("Hallo, {}!", self.name)))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("greeting".to_string(), exec);
        Ok(None) // Flow beenden
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
    println!("Flow abgeschlossen: {:?}", result);

    Ok(())
}
```

### Parallele Ausführung mit Observabilität

```rust
use agentflow_core::{AsyncFlow, MetricsCollector};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Erstelle Nodes für parallele Ausführung
    let nodes = vec![
        Box::new(ProcessingNode { id: "worker_1".to_string() }),
        Box::new(ProcessingNode { id: "worker_2".to_string() }),
        Box::new(ProcessingNode { id: "worker_3".to_string() }),
    ];

    // Observabilität einrichten
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("parallel_processing".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await?;

    // Metriken prüfen
    let execution_count = metrics.get_metric("parallel_processing.execution_count");
    println!("Ausführungen: {:?}", execution_count);

    Ok(())
}
```

### Robuster Flow mit Circuit Breaker

```rust
use agentflow_core::{CircuitBreaker, TimeoutManager};
use std::time::Duration;

async fn robust_workflow() -> Result<()> {
    // Robustheitsmuster einrichten
    let circuit_breaker = CircuitBreaker::new(
        "api_calls".to_string(),
        3, // Fehler-Schwellenwert
        Duration::from_secs(30) // Wiederherstellungs-Timeout
    );

    let timeout_manager = TimeoutManager::new(
        "operations".to_string(),
        Duration::from_secs(10) // Standard-Timeout
    );

    // In Ihrer Workflow-Logik verwenden
    let result = circuit_breaker.call(async {
        timeout_manager.execute_with_timeout("api_call", async {
            // Ihre Geschäftslogik hier
            Ok("Erfolg")
        }).await
    }).await?;

    Ok(())
}
```

## 🏗️ Architektur

AgentFlow ist auf vier Kernsäulen aufgebaut:

1. **Ausführungsmodell**: AsyncNode Trait mit prep/exec/post Lebenszyklus
2. **Nebenläufigkeitskontrolle**: Parallele, Batch- und verschachtelte Ausführungsmuster
3. **Robustheitsgarantien**: Circuit Breaker, Retries, Timeouts und Ressourcenverwaltung
4. **Observabilität**: Metriken, Events, Alerting und Distributed Tracing

Für detaillierte Architekturinformationen siehe [docs/design.md](docs/design.md).

## 📚 Dokumentation

- **[Design-Dokument](docs/design.md)** - Systemarchitektur und Komponentendiagramme
- **[Funktionale Spezifikation](docs/functional-spec.md)** - Feature-Anforderungen und API-Spezifikationen
- **[Use Cases](docs/use-cases.md)** - Reale Implementierungsszenarien
- **[API-Referenz](docs/api/)** - Vollständige API-Dokumentation
- **[Migrationshandbuch](docs/migration.md)** - Upgrade von PocketFlow

## 🧪 Testing

AgentFlow hält 100% Testabdeckung mit umfassenden Testsuiten aufrecht:

```bash
# Alle Tests ausführen
cargo test

# Mit Ausgabe ausführen
cargo test -- --nocapture

# Spezifische Modul-Tests ausführen
cargo test async_flow
cargo test robustness
cargo test observability
```

**Aktueller Status**: 67/67 Tests bestanden ✅

## 🚢 Produktionsbereitschaft

AgentFlow ist für Produktionsumgebungen konzipiert mit:

- **Speichersicherheit**: Rusts Ownership-Modell verhindert Data Races und Memory Leaks
- **Performance**: Nullkosten-Abstraktionen und effiziente asynchrone Runtime
- **Zuverlässigkeit**: Umfassendes Error Handling und elegante Degradation
- **Skalierbarkeit**: Eingebaute Unterstützung für horizontale Skalierungsmuster
- **Monitoring**: Vollständiger Observability-Stack für Produktions-Insights

## 🛣️ Roadmap

- **v0.3.0**: MCP (Model Context Protocol) Integration
- **v0.4.0**: Verteilte Ausführungs-Engine
- **v0.5.0**: WebAssembly Plugin-System
- **v1.0.0**: Produktionsstabilitäts-Garantien

## 🤝 Beitragen

Wir begrüßen Beiträge! Bitte lesen Sie [CONTRIBUTING.md](CONTRIBUTING.md) für Richtlinien.

## 📄 Lizenz

Dieses Projekt ist unter der MIT-Lizenz lizenziert - siehe die [LICENSE](LICENSE) Datei für Details.

## 🙏 Danksagungen

- Aufgebaut auf der Grundlage des ursprünglichen PocketFlow-Konzepts
- Inspiriert von modernen verteilten Systemmustern
- Angetrieben vom Rust-Ökosystem und der Tokio-Runtime

---

**AgentFlow**: Wo intelligente Workflows auf Unternehmens-Zuverlässigkeit treffen. 🦀✨