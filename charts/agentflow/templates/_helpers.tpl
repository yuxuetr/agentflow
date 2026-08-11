{{- define "agentflow.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "agentflow.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "agentflow.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version | replace "+" "_" }}
app.kubernetes.io/name: {{ include "agentflow.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "agentflow.selectorLabels" -}}
app.kubernetes.io/name: {{ include "agentflow.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "agentflow.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "agentflow.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{/*
W4.2a: fail the render rather than let an operator silently deploy an
unsafe multi-replica gateway. `agentflow-server` today keeps several
pieces of state process-local (SSE fan-out, per-tenant run admission
limits, harness approval/cancellation routing) — see TODOs.md W4.2 and
values.yaml's `allowMultiReplica` comment for the full list. Nothing in
the chart previously stopped `--set replicaCount=2` or
`--set autoscaling.enabled=true --set autoscaling.minReplicas=2` from
producing a deployment with silently-broken cross-replica behavior
(admission limits multiplied by replica count, approval/cancel requests
404ing or no-op'ing depending on which pod an operator's HTTP request
lands on, etc.). `allowMultiReplica: true` is an explicit, informed
opt-out for operators who understand and accept those gaps.
*/}}
{{- define "agentflow.validateReplicaCount" -}}
{{- if not .Values.allowMultiReplica -}}
{{- if gt (.Values.replicaCount | int) 1 -}}
{{- fail "replicaCount > 1 is not yet safe: agentflow-server keeps SSE fan-out, run admission limits, and harness approval/cancellation state process-local (see TODOs.md W4.2). Set allowMultiReplica=true to opt out of this guard once you understand and accept those gaps." -}}
{{- end -}}
{{- if and .Values.autoscaling.enabled (gt (.Values.autoscaling.minReplicas | int) 1) -}}
{{- fail "autoscaling.minReplicas > 1 is not yet safe: agentflow-server keeps SSE fan-out, run admission limits, and harness approval/cancellation state process-local (see TODOs.md W4.2). Set allowMultiReplica=true to opt out of this guard once you understand and accept those gaps." -}}
{{- end -}}
{{- end -}}
{{- end -}}
