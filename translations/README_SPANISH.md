# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **Un framework moderno de Rust con prioridad asíncrona para construir flujos de trabajo de agentes inteligentes con robustez y observabilidad de nivel empresarial.**

AgentFlow es un nuevo framework de Rust inspirado en los conceptos de PocketFlow, que ofrece orquestación de flujos de trabajo lista para producción con concurrencia asíncrona, observabilidad y patrones de confiabilidad.

## 🚀 Características Clave

### ⚡ **Arquitectura Asíncrona Primero**

- Construido sobre el runtime de Tokio para ejecución asíncrona de alto rendimiento
- Soporte nativo para procesamiento paralelo y por lotes
- Abstracciones de costo cero con el modelo de propiedad de Rust
- Cumplimiento Send + Sync para concurrencia segura

### 🛡️ **Robustez Empresarial**

- **Cortocircuitos**: Detección automática de fallos y recuperación
- **Limitación de Velocidad**: Algoritmos de ventana deslizante para control de tráfico
- **Políticas de Reintento**: Retroceso exponencial con jitter
- **Gestión de Tiempo de Espera**: Degradación elegante bajo carga
- **Agrupación de Recursos**: Guardias RAII para gestión segura de recursos
- **Descarte de Carga**: Gestión adaptativa de capacidad

### 📊 **Observabilidad Integral**

- Recolección de métricas en tiempo real a nivel de flujo y nodo
- Registro de eventos estructurado con marcas de tiempo y duraciones
- Perfilado de rendimiento y detección de cuellos de botella
- Sistema de alertas configurable
- Soporte de trazado distribuido
- Listo para integración con plataformas de monitoreo

### 🔄 **Modelos de Ejecución Flexibles**

- **Flujos Secuenciales**: Ejecución tradicional de nodo a nodo
- **Ejecución Paralela**: Procesamiento concurrente de nodos con `futures::join_all`
- **Procesamiento por Lotes**: Tamaños de lote configurables con ejecución concurrente de lotes
- **Flujos Anidados**: Composición jerárquica de flujos de trabajo
- **Enrutamiento Condicional**: Control dinámico de flujo basado en estado de tiempo de ejecución

## 📦 Instalación

Agrega AgentFlow a tu `Cargo.toml`:

```toml
[dependencies]
agentflow-core = "0.2.0"
tokio = { version = "1.0", features = ["full"] }
```

## 🎯 Inicio Rápido

### Flujo Secuencial Básico

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;

// Define un nodo personalizado
struct GreetingNode {
    name: String,
}

#[async_trait]
impl AsyncNode for GreetingNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        Ok(Value::String(format!("Preparando saludo para {}", self.name)))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        Ok(Value::String(format!("¡Hola, {}!", self.name)))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("greeting".to_string(), exec);
        Ok(None) // Terminar flujo
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
    println!("Flujo completado: {:?}", result);

    Ok(())
}
```

### Ejecución Paralela con Observabilidad

```rust
use agentflow_core::{AsyncFlow, MetricsCollector};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Crear nodos para ejecución paralela
    let nodes = vec![
        Box::new(ProcessingNode { id: "worker_1".to_string() }),
        Box::new(ProcessingNode { id: "worker_2".to_string() }),
        Box::new(ProcessingNode { id: "worker_3".to_string() }),
    ];

    // Configurar observabilidad
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("parallel_processing".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await?;

    // Verificar métricas
    let execution_count = metrics.get_metric("parallel_processing.execution_count");
    println!("Ejecuciones: {:?}", execution_count);

    Ok(())
}
```

### Flujo Robusto con Cortocircuito

```rust
use agentflow_core::{CircuitBreaker, TimeoutManager};
use std::time::Duration;

async fn robust_workflow() -> Result<()> {
    // Configurar patrones de robustez
    let circuit_breaker = CircuitBreaker::new(
        "api_calls".to_string(),
        3, // umbral de falla
        Duration::from_secs(30) // tiempo de espera de recuperación
    );

    let timeout_manager = TimeoutManager::new(
        "operations".to_string(),
        Duration::from_secs(10) // tiempo de espera predeterminado
    );

    // Usar en tu lógica de flujo de trabajo
    let result = circuit_breaker.call(async {
        timeout_manager.execute_with_timeout("api_call", async {
            // Tu lógica de negocio aquí
            Ok("Éxito")
        }).await
    }).await?;

    Ok(())
}
```

## 🏗️ Arquitectura

AgentFlow está construido sobre cuatro pilares fundamentales:

1. **Modelo de Ejecución**: Trait AsyncNode con ciclo de vida prep/exec/post
2. **Control de Concurrencia**: Patrones de ejecución paralelos, por lotes y anidados
3. **Garantías de Robustez**: Cortocircuitos, reintentos, tiempos de espera y gestión de recursos
4. **Observabilidad**: Métricas, eventos, alertas y trazado distribuido

Para información detallada de arquitectura, ver [docs/design.md](docs/design.md).

## 📚 Documentación

- **[Documento de Diseño](docs/design.md)** - Arquitectura del sistema y diagramas de componentes
- **[Especificación Funcional](docs/functional-spec.md)** - Requisitos de características y especificaciones de API
- **[Casos de Uso](docs/use-cases.md)** - Escenarios de implementación del mundo real
- **[Referencia de API](docs/api/)** - Documentación completa de API
- **[Guía de Migración](docs/migration.md)** - Actualización desde PocketFlow

## 🧪 Pruebas

AgentFlow mantiene 100% de cobertura de pruebas con suites de pruebas integrales:

```bash
# Ejecutar todas las pruebas
cargo test

# Ejecutar con salida
cargo test -- --nocapture

# Ejecutar pruebas de módulos específicos
cargo test async_flow
cargo test robustness
cargo test observability
```

**Estado Actual**: 67/67 pruebas pasando ✅

## 🚢 Listo para Producción

AgentFlow está diseñado para entornos de producción con:

- **Seguridad de Memoria**: El modelo de propiedad de Rust previene carreras de datos y fugas de memoria
- **Rendimiento**: Abstracciones de costo cero y runtime asíncrono eficiente
- **Confiabilidad**: Manejo integral de errores y degradación elegante
- **Escalabilidad**: Soporte integrado para patrones de escalado horizontal
- **Monitoreo**: Stack completo de observabilidad para insights de producción

## 🛣️ Hoja de Ruta

- **v0.3.0**: Integración MCP (Protocolo de Contexto de Modelo)
- **v0.4.0**: Motor de ejecución distribuida
- **v0.5.0**: Sistema de plugins WebAssembly
- **v1.0.0**: Garantías de estabilidad de producción

## 🤝 Contribuir

¡Damos la bienvenida a las contribuciones! Por favor, ver [CONTRIBUTING.md](CONTRIBUTING.md) para pautas.

## 📄 Licencia

Este proyecto está licenciado bajo la Licencia MIT - ver el archivo [LICENSE](LICENSE) para detalles.

## 🙏 Reconocimientos

- Construido sobre la base del concepto original de PocketFlow
- Inspirado por patrones modernos de sistemas distribuidos
- Impulsado por el ecosistema de Rust y el runtime de Tokio

---

**AgentFlow**: Donde los flujos de trabajo inteligentes se encuentran con la confiabilidad empresarial. 🦀✨