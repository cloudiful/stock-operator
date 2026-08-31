export interface PublicSettings {
  stock_service_url: string | null
  bind_addr: string
  mcp_path: string
  target_bundle_id: string
  target_process_name: string
  max_depth: number
  max_nodes: number
  instance_id: string
  network_mode: string
}

export interface SaveSettingsRequest {
  stock_service_url: string | null
  bind_addr: string
  mcp_path: string
  target_bundle_id: string
  target_process_name: string
  max_depth: number
  max_nodes: number
  network_mode: string
  private_overlay_ack: boolean
}

export interface SaveSettingsResponse {
  settings: PublicSettings
  restart_required: boolean
  restart_reasons: string[]
  message: string
}

export interface AccessibilityStatusDto {
  api_enabled: boolean
  process_trusted: boolean
  target_process_name: string
  target_bundle_id: string
  target_pid: number | null
  target_found: boolean
  notes: string[]
}

export interface RuntimeStatus {
  server_running: boolean
  server_bind_addr: string | null
  server_error: string | null
  token_configured: boolean
  token_source: string
  accessibility: AccessibilityStatusDto
  restart_required: boolean
  restart_reasons: string[]
  instance_id: string
  db_path: string
}

export interface TestConnectionResult {
  ok: boolean
  status: number | null
  message: string
  latency_ms: number | null
}

export interface OperationHistoryResponse {
  total: number
  operations: Array<{
    operation_id?: string
    id?: string
    created_at: string
    kind: string
    state: string
    fingerprint: string | null
    payload_summary: unknown
    actor_source: string
  }>
}

export interface AuditHistoryResponse {
  total: number
  events: Array<{
    created_at: string
    operation_id: string | null
    event_type: string
    from_state: string | null
    to_state: string | null
    actor_source: string
    detail: unknown
  }>
}
