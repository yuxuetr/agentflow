# Kubernetes Deployment with AgentFlow Health Checks

This guide demonstrates how to deploy AgentFlow workflows in Kubernetes with integrated health checks, leveraging Phase 1.5 features for production-ready deployments.

## Table of Contents

- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Health Check Integration](#health-check-integration)
- [Deployment Configuration](#deployment-configuration)
- [Service Configuration](#service-configuration)
- [ConfigMap for Workflow Configuration](#configmap-for-workflow-configuration)
- [Complete Deployment Example](#complete-deployment-example)
- [Monitoring and Observability](#monitoring-and-observability)
- [Best Practices](#best-practices)

## Overview

AgentFlow's Phase 1.5 health check system is designed to integrate seamlessly with Kubernetes health probes:

- **Liveness Probes**: Detect if the application is alive and responsive
- **Readiness Probes**: Determine if the application can accept traffic
- **Startup Probes**: Handle slow-starting containers

Key Features:
- HTTP endpoint for health checks (`/health`)
- Liveness endpoint (`/health/live`)
- Readiness endpoint (`/health/ready`)
- Prometheus metrics endpoint (`/metrics`)
- Graceful shutdown support
- Resource limit enforcement
- Checkpoint-based recovery

## Prerequisites

- Kubernetes cluster (v1.19+)
- kubectl configured
- Docker registry for images
- AgentFlow application built with `observability` feature

## Health Check Integration

### Health Check Endpoints

AgentFlow provides three health check endpoints:

1. **`/health`** - Full health check (all registered checks)
2. **`/health/live`** - Liveness check (basic responsiveness)
3. **`/health/ready`** - Readiness check (ready to serve traffic)

### Implementing Health Endpoints

```rust
use agentflow_core::health::{HealthChecker, HealthStatus};
use axum::{routing::get, Router, Json};
use std::sync::Arc;

/// Setup health check routes
async fn setup_health_routes(checker: Arc<HealthChecker>) -> Router {
    Router::new()
        .route("/health", get(full_health_check))
        .route("/health/live", get(liveness_check))
        .route("/health/ready", get(readiness_check))
        .with_state(checker)
}

/// Full health check handler
async fn full_health_check(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> Json<agentflow_core::health::HealthReport> {
    Json(checker.check_health().await)
}

/// Liveness probe handler
async fn liveness_check(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> &'static str {
    if checker.is_alive().await {
        "OK"
    } else {
        "FAIL"
    }
}

/// Readiness probe handler
async fn readiness_check(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> &'static str {
    if checker.is_ready().await {
        "OK"
    } else {
        "FAIL"
    }
}
```

## Deployment Configuration

### Basic Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: agentflow-workflow
  namespace: default
  labels:
    app: agentflow
    component: workflow-runner
    version: v0.2.0
spec:
  replicas: 2
  selector:
    matchLabels:
      app: agentflow
      component: workflow-runner
  template:
    metadata:
      labels:
        app: agentflow
        component: workflow-runner
        version: v0.2.0
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: agentflow
        image: your-registry/agentflow:v0.2.0
        imagePullPolicy: Always

        # Environment variables
        env:
        - name: RUST_LOG
          value: "info,agentflow=debug"
        - name: ENV
          value: "production"
        - name: LOG_FORMAT
          value: "json"
        - name: CHECKPOINT_DIR
          value: "/data/checkpoints"

        # Resource limits (aligned with agentflow resource management)
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "2Gi"  # Matches workflow_memory_limit
            cpu: "1000m"

        # Health check configuration
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8080
            scheme: HTTP
          initialDelaySeconds: 10
          periodSeconds: 10
          timeoutSeconds: 5
          successThreshold: 1
          failureThreshold: 3

        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8080
            scheme: HTTP
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 3
          successThreshold: 1
          failureThreshold: 2

        startupProbe:
          httpGet:
            path: /health/live
            port: 8080
            scheme: HTTP
          initialDelaySeconds: 0
          periodSeconds: 2
          timeoutSeconds: 3
          successThreshold: 1
          failureThreshold: 30  # 60 seconds (30 * 2s) max startup time

        # Ports
        ports:
        - name: http
          containerPort: 8080
          protocol: TCP
        - name: metrics
          containerPort: 9090
          protocol: TCP

        # Volume mounts
        volumeMounts:
        - name: checkpoint-storage
          mountPath: /data/checkpoints
        - name: config
          mountPath: /etc/agentflow
          readOnly: true

        # Security context
        securityContext:
          allowPrivilegeEscalation: false
          runAsNonRoot: true
          runAsUser: 1000
          capabilities:
            drop:
            - ALL
          readOnlyRootFilesystem: true

      # Volumes
      volumes:
      - name: checkpoint-storage
        persistentVolumeClaim:
          claimName: agentflow-checkpoint-pvc
      - name: config
        configMap:
          name: agentflow-config

      # Restart policy
      restartPolicy: Always

      # Termination grace period (for graceful shutdown)
      terminationGracePeriodSeconds: 30

      # Service account
      serviceAccountName: agentflow
```

### PersistentVolumeClaim for Checkpoints

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: agentflow-checkpoint-pvc
  namespace: default
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 10Gi
  storageClassName: standard  # Adjust based on your storage class
```

## Service Configuration

```yaml
apiVersion: v1
kind: Service
metadata:
  name: agentflow-service
  namespace: default
  labels:
    app: agentflow
    component: workflow-runner
spec:
  type: ClusterIP
  selector:
    app: agentflow
    component: workflow-runner
  ports:
  - name: http
    port: 80
    targetPort: 8080
    protocol: TCP
  - name: metrics
    port: 9090
    targetPort: 9090
    protocol: TCP
  sessionAffinity: None
```

## ConfigMap for Workflow Configuration

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: agentflow-config
  namespace: default
data:
  # Timeout configuration
  timeout.yaml: |
    # Production timeout configuration
    environment: production
    workflow_timeout_secs: 300
    node_execution_timeout_secs: 60
    llm_request_timeout_secs: 30

  # Checkpoint configuration
  checkpoint.yaml: |
    checkpoint_dir: /data/checkpoints
    success_retention_days: 7
    failure_retention_days: 30
    auto_cleanup: true

  # Resource limits
  resource_limits.yaml: |
    workflow_memory_limit: 2147483648  # 2 GB
    node_memory_limit: 104857600       # 100 MB
    max_state_size: 104857600          # 100 MB
    max_value_size: 10485760           # 10 MB
    cleanup_threshold: 0.8
    auto_cleanup: true

  # Workflow definition
  workflow.yaml: |
    name: production-workflow
    timeout:
      workflow_timeout_secs: 300
    nodes:
      - id: extract_data
        type: Custom
        # ... workflow definition

## Complete Deployment Example

### Full Deployment with All Components

```yaml
---
# Namespace
apiVersion: v1
kind: Namespace
metadata:
  name: agentflow
  labels:
    name: agentflow

---
# ServiceAccount
apiVersion: v1
kind: ServiceAccount
metadata:
  name: agentflow
  namespace: agentflow

---
# Role for checkpoint management
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: agentflow-checkpoint-manager
  namespace: agentflow
rules:
- apiGroups: [""]
  resources: ["persistentvolumeclaims"]
  verbs: ["get", "list"]
- apiGroups: [""]
  resources: ["configmaps"]
  verbs: ["get", "list", "watch"]

---
# RoleBinding
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: agentflow-checkpoint-manager-binding
  namespace: agentflow
subjects:
- kind: ServiceAccount
  name: agentflow
  namespace: agentflow
roleRef:
  kind: Role
  name: agentflow-checkpoint-manager
  apiGroup: rbac.authorization.k8s.io

---
# PersistentVolumeClaim
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: agentflow-checkpoint-pvc
  namespace: agentflow
  labels:
    app: agentflow
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 10Gi
  storageClassName: standard

---
# ConfigMap (see above for full content)
apiVersion: v1
kind: ConfigMap
metadata:
  name: agentflow-config
  namespace: agentflow
data:
  timeout.yaml: |
    environment: production
    workflow_timeout_secs: 300
  # ... (see above)

---
# Deployment (see above for full configuration)
apiVersion: apps/v1
kind: Deployment
# ... (see above)

---
# Service
apiVersion: v1
kind: Service
metadata:
  name: agentflow-service
  namespace: agentflow
# ... (see above)

---
# HorizontalPodAutoscaler
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: agentflow-hpa
  namespace: agentflow
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: agentflow-workflow
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 0
      policies:
      - type: Percent
        value: 100
        periodSeconds: 30
```

## Monitoring and Observability

### ServiceMonitor for Prometheus

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: agentflow-metrics
  namespace: agentflow
  labels:
    app: agentflow
spec:
  selector:
    matchLabels:
      app: agentflow
      component: workflow-runner
  endpoints:
  - port: metrics
    path: /metrics
    interval: 30s
    scrapeTimeout: 10s
```

### Grafana Dashboard

Key metrics to monitor:

1. **Health Status**
   - `agentflow_health_status{component="system"}` - Overall system health
   - `agentflow_health_check_duration_seconds` - Health check latency

2. **Resource Usage**
   - `agentflow_memory_usage_bytes` - Current memory usage
   - `agentflow_memory_limit_bytes` - Memory limit
   - `agentflow_state_size_bytes` - Workflow state size

3. **Workflow Execution**
   - `agentflow_workflow_duration_seconds` - Workflow execution time
   - `agentflow_node_execution_duration_seconds` - Node execution time
   - `agentflow_workflow_failures_total` - Workflow failures

4. **Checkpoints**
   - `agentflow_checkpoint_save_duration_seconds` - Checkpoint save latency
   - `agentflow_checkpoint_load_duration_seconds` - Checkpoint load latency
   - `agentflow_checkpoint_count` - Number of checkpoints

### Logging Configuration

For JSON structured logging in production:

```yaml
env:
- name: RUST_LOG
  value: "info,agentflow=debug"
- name: LOG_FORMAT
  value: "json"
```

Example log aggregation with Fluentd:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluentd-config
  namespace: kube-system
data:
  fluent.conf: |
    <source>
      @type tail
      path /var/log/containers/*agentflow*.log
      pos_file /var/log/fluentd-agentflow.pos
      tag kubernetes.agentflow
      <parse>
        @type json
        time_key timestamp
        time_format %Y-%m-%dT%H:%M:%S.%NZ
      </parse>
    </source>

    <filter kubernetes.agentflow>
      @type record_transformer
      <record>
        app agentflow
        environment production
      </record>
    </filter>

    <match kubernetes.agentflow>
      @type elasticsearch
      host elasticsearch.logging.svc.cluster.local
      port 9200
      index_name agentflow
      type_name _doc
    </match>
```

## Best Practices

### 1. Health Check Configuration

**Recommended Settings:**

- **Liveness Probe**: Longer timeout and failure threshold
  - `initialDelaySeconds: 10` - Give app time to start
  - `periodSeconds: 10` - Check every 10 seconds
  - `timeoutSeconds: 5` - Allow 5 seconds for response
  - `failureThreshold: 3` - Restart after 3 failures (30 seconds)

- **Readiness Probe**: Stricter timing for traffic management
  - `initialDelaySeconds: 5` - Quick initial check
  - `periodSeconds: 5` - Frequent checks
  - `timeoutSeconds: 3` - Shorter timeout
  - `failureThreshold: 2` - Remove from service faster

### 2. Resource Management

**Align Kubernetes and AgentFlow Limits:**

```yaml
# Kubernetes
resources:
  limits:
    memory: "2Gi"

# AgentFlow ResourceManagerConfig
workflow_memory_limit: 2147483648  # 2 GB
```

**Monitor Memory Usage:**
- Set up alerts for approaching memory limits
- Use `cleanup_threshold: 0.8` for proactive cleanup
- Enable `auto_cleanup: true` in production

### 3. Checkpoint Recovery

**Use Persistent Storage:**

```yaml
volumes:
- name: checkpoint-storage
  persistentVolumeClaim:
    claimName: agentflow-checkpoint-pvc
```

**Retention Policy:**
- Success: 7 days (short retention to save space)
- Failure: 30 days (keep for debugging)
- Enable auto-cleanup to manage storage

### 4. Graceful Shutdown

**Configure Termination:**

```yaml
terminationGracePeriodSeconds: 30
```

**Implement Shutdown Handler:**

```rust
use tokio::signal;

async fn shutdown_handler(health_checker: Arc<HealthChecker>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, performing graceful shutdown...");

    // Mark unhealthy to stop receiving traffic
    health_checker.set_metadata("status", "shutting_down").await;

    // Give time for connections to drain
    tokio::time::sleep(Duration::from_secs(5)).await;

    info!("Graceful shutdown complete");
}
```

### 5. High Availability

**Multi-Replica Deployment:**

```yaml
spec:
  replicas: 2  # Minimum for HA
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
```

**Pod Disruption Budget:**

```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: agentflow-pdb
  namespace: agentflow
spec:
  minAvailable: 1
  selector:
    matchLabels:
      app: agentflow
      component: workflow-runner
```

### 6. Security

**Security Context:**

```yaml
securityContext:
  allowPrivilegeEscalation: false
  runAsNonRoot: true
  runAsUser: 1000
  capabilities:
    drop:
    - ALL
  readOnlyRootFilesystem: true
```

**NetworkPolicy:**

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: agentflow-netpol
  namespace: agentflow
spec:
  podSelector:
    matchLabels:
      app: agentflow
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - podSelector:
        matchLabels:
          role: monitoring
    ports:
    - protocol: TCP
      port: 9090
  egress:
  - to:
    - podSelector: {}
    ports:
    - protocol: TCP
      port: 443
```

## Deployment Steps

### 1. Build and Push Image

```bash
# Build with observability features
cargo build --release --features observability

# Build Docker image
docker build -t your-registry/agentflow:v0.2.0 .

# Push to registry
docker push your-registry/agentflow:v0.2.0
```

### 2. Apply Kubernetes Resources

```bash
# Create namespace
kubectl apply -f namespace.yaml

# Apply all resources
kubectl apply -f kubernetes/

# Verify deployment
kubectl get all -n agentflow

# Check health
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://agentflow-service.agentflow/health
```

### 3. Monitor Deployment

```bash
# Watch pod status
kubectl get pods -n agentflow -w

# Check logs
kubectl logs -f -n agentflow -l app=agentflow

# Describe pod for events
kubectl describe pod -n agentflow <pod-name>

# Check health endpoints
kubectl port-forward -n agentflow svc/agentflow-service 8080:80
curl http://localhost:8080/health
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready
```

### 4. Test Fault Recovery

```bash
# Simulate pod failure
kubectl delete pod -n agentflow <pod-name>

# Check checkpoint recovery in new pod logs
kubectl logs -f -n agentflow <new-pod-name> | grep checkpoint
```

## Troubleshooting

### Health Check Failures

**Symptom**: Pods constantly restarting

```bash
# Check health check configuration
kubectl describe pod -n agentflow <pod-name> | grep -A 10 Liveness

# Check health endpoint manually
kubectl exec -it -n agentflow <pod-name> -- \
  curl http://localhost:8080/health/live

# Review logs for health check errors
kubectl logs -n agentflow <pod-name> | grep health
```

**Common Issues**:
- `initialDelaySeconds` too short
- Application slow to start
- Database connection issues
- Resource constraints

### Memory Issues

**Symptom**: OOMKilled errors

```bash
# Check resource usage
kubectl top pods -n agentflow

# Review memory metrics
kubectl exec -it -n agentflow <pod-name> -- \
  curl http://localhost:9090/metrics | grep agentflow_memory
```

**Solutions**:
- Increase `resources.limits.memory`
- Adjust `workflow_memory_limit`
- Enable `auto_cleanup`
- Lower `cleanup_threshold`

### Checkpoint Recovery Issues

**Symptom**: Workflows restart from beginning

```bash
# Check checkpoint directory permissions
kubectl exec -it -n agentflow <pod-name> -- ls -la /data/checkpoints

# Verify PVC is mounted
kubectl describe pod -n agentflow <pod-name> | grep -A 5 Mounts

# Check checkpoint retention
kubectl exec -it -n agentflow <pod-name> -- \
  ls -lah /data/checkpoints/
```

## References

- [AgentFlow Timeout Control Guide](./TIMEOUT_CONTROL.md)
- [AgentFlow Health Checks Guide](./HEALTH_CHECKS.md)
- [AgentFlow Checkpoint Recovery Guide](./CHECKPOINT_RECOVERY.md)
- [Kubernetes Liveness, Readiness, and Startup Probes](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/configuration/overview/)

---

**Last Updated**: 2025-11-16
**AgentFlow Version**: 0.2.0 (Phase 1.5 Complete)
**Kubernetes Version**: 1.19+
