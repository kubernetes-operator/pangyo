{{/* 공통 라벨 */}}
{{- define "memonest.labels" -}}
app.kubernetes.io/name: memonest
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
{{- define "memonest.dbHost" -}}
{{- if .Values.postgres.enabled }}{{ .Release.Name }}-postgres{{ else }}{{ required "postgres.externalHost 필요" .Values.postgres.externalHost }}{{ end }}
{{- end }}
