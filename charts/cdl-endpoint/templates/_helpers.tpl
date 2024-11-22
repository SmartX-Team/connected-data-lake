{{/*
Expand the name of the chart.
*/}}
{{- define "cdl-endpoint.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "cdl-endpoint.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "cdl-endpoint.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "cdl-endpoint.labels" -}}
cdl.ulagbulag.io/modelstorage-type: "object-storage"
helm.sh/chart: {{ include "cdl-endpoint.chart" . }}
{{ include "cdl-endpoint.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "cdl-endpoint.selectorLabels" -}}
app.kubernetes.io/name: {{ include "cdl-endpoint.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "cdl-endpoint.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "cdl-endpoint.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create the name of the clusterrole to use
*/}}
{{- define "cdl-endpoint.clusterRoleName" -}}
{{- printf "dash:cdl-endpoint:%s:%s" .Release.Namespace .Release.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create the name of the ingressclass to use
*/}}
{{- define "cdl-endpoint.ingressClassName" -}}
{{- default (printf "cdl.%s.%s" .Release.Name .Release.Namespace) .Values.ingressClassNameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create the name of the ingress controller to use
*/}}
{{- define "cdl-endpoint.ingressControllerName" -}}
{{- default (printf "k8s.io/dash/%s/%s" .Release.Name .Release.Namespace) .Values.ingressControllerNameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}
