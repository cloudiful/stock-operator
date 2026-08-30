# Protocol and compatibility

## Scope

This document records the HTTP/MCP boundary preserved by the standalone
`stock-operator` extract and explicitly scopes work that remains for later
phases. It does not claim features that are not yet implemented.

## Current implementation (phase 3: tauri-desktop)

- **Source**: Extracted from `stock-goes-stonk/apps/stock-operator` at
  `25abae0b9353d75d6b3bde8066090b0cd569349f`, phase 2 baseline
  `b31e3b423cc707ed39bf70bb8676a5b8527bf65c`.
- **Platform**: macOS-only. Non-macOS builds emit a stub and exit; macOS builds
  use Accessibility (`axuielement`), ScreenCaptureKit/Vision OCR helper
  (`macos/window_ocr.swift`), bundle identifier
  `com.cloudiful.stock-operator` (`LSMinimumSystemVersion 26.0`), and
  single-instance UI serialization via `tauri-plugin-single-instance`.
- **Desktop**: Tauri v2 shell (`tauri.conf.json`, `capabilities/default.json`,
  `ui/` vanilla HTML/CSS/JS, `src/desktop/` commands). Double-clicking the
  `.app` with no CLI arguments opens the window; `stock-operator serve` is the
  explicit headless/server path. Background HTTP/MCP server shares the same
  `OperatorService`/`Storage` instance as Tauri commands. First-run without a
  token remains usable; saving a token (Keychain) makes the server available
  without an unsafe fallback. Single-instance prevents competing UI/server
  processes.
- **Config**: Env-over-SQLite with validation. `STOCK_OPERATOR_AUTH_TOKEN` is
  env-preferred or Keychain (`com.cloudiful.stock-operator` /
  `operator-bearer-token`), never SQLite. `STOCK_OPERATOR_DB_PATH` overrides the
  per-user default `~/Library/Application Support/Stock Operator/operator.sqlite3`
  (created with parents, WAL + FK). Ordinary settings persisted and reloaded on
  restart when env absent: `STOCK_OPERATOR_MAIN_SERVICE_URL` (alias
  `STOCK_OPERATOR_STOCK_SERVICE_URL`), bind/MCP path, `STOCK_OPERATOR_TARGET_BUNDLE_ID`/
  `STOCK_OPERATOR_TARGET_PROCESS_NAME`, traversal limits, and `operator_instance_id`.
  URL, socket address, paths, and numeric bounds are validated; loopback-only
  binding is enforced. Server/inspector-affecting changes are reported as
  restart-required.
- **Storage**: `rusqlite 0.32.1` (bundled) behind synchronous repository
  (`src/storage.rs`), migrations in `migrations/0001_initial.sql`. Tables:
  `operator_settings`, `operations` (durable live state + redacted payload,
  one-time confirmation tokens never persisted), `audit_events`. Bearer tokens
  live in Keychain; Keychain unavailability returns a clear non-secret error
  without plaintext fallback. Redacted summaries store only security code /
  side / price / quantity and fingerprint.
- **Build**: `build.rs` combines `tauri_build::try_build` (embedding
  `tauri.conf.json` + `ui/`) with the OCR helper `xcrun swiftc` when on macOS;
  missing `swiftc`/SDK or Tauri context is a warning, not a hard error.
- **Package**: `nu package.nu` builds the release Tauri binary, embeds `ui/`,
  and produces `target/stock-operator/Stock Operator.app` with ad-hoc signing
  and `Contents/Helpers/window-ocr`. When a `target/release/bundle/macos`
  Tauri bundle exists it is reused and the helper is injected.

## HTTP surface

Same loopback listener for REST and MCP:

```
GET  /healthz                    (public, no account data)
GET  /api/openapi.json           (public, OpenAPI 3.1)
POST /mcp                        (Bearer required)
GET  /api/v1/operator/view
POST /api/v1/operator/read
POST /api/v1/operator/navigate/{target}
POST /api/v1/operator/securities/select
POST /api/v1/operator/orders/stage
POST /api/v1/operator/orders/preflight
POST /api/v1/operator/orders/prepare
POST /api/v1/operator/cancellations/prepare
POST /api/v1/operator/operations/confirm
POST /api/v1/operator/operations/{operation_id}/abort
GET  /api/v1/operator/operations/{operation_id}
GET  /api/v1/operator/operations          # history (Bearer, ?limit&offset&kind&state)
GET  /api/v1/operator/audit/events        # audit trail (Bearer, ?limit&offset&operation_id)
```

History and audit listings are the Tauri UI foundation and preserve the existing
single-operation fetch. All new paths are authenticated and included in the
generated utoipa OpenAPI document.

`/healthz` and `/api/openapi.json` are public. All other paths require
`Authorization: Bearer <token>` matching the env or Keychain token.

OpenAPI is exportable without starting the server:
`target/debug/stock-operator inspect openapi`.

## Tauri command surface

Invoked via `window.__TAURI__.core.invoke`; browser fallback is minimal and
non-mutating:

```
get_settings              -> PublicSettings (no secrets)
save_settings             -> SaveSettingsResponse { restart_required, reasons }
get_runtime_status        -> RuntimeStatus { server_running, token_configured/source, accessibility, restart, db_path }
get_token_status          -> { configured, source }
save_token / clear_token  -> TokenStatus (Keychain, env-preferred)
test_stock_service_url    -> { ok, status, message, latency_ms }
list_operations           -> OperationHistoryResponse (paginated, redacted)
list_audit_events         -> AuditHistoryResponse (paginated, filtered)
```

No command returns or logs bearer/confirmation tokens; only booleans/status.

## MCP surface

Streamable-HTTP transport at `POST /mcp` with `rmcp` 3.1.1. Tools are
read-only except for the gated live operations exposed via CLI/REST (not MCP).
Current tools include: `operator_health`, `accessibility_status`,
`inspect_target_app`, `read_trade_snapshot`, `ui_inventory`, `current_view`,
`read_trade_form`, `read_positions*`, `read_orders*`, `read_executions*`,
`read_funds*`, `diagnose_positions_table`, `navigation_candidates`,
`navigate_readonly`, `ocr_visible_text`, `trade_preflight`, `stage_order`,
plus the read-only `list_recent_operations` (redacted history, no tokens;
HTTP `GET /api/v1/operator/operations` is the primary UI path).

## Live-operation safety gates (durable, same behavior)

- CLI live commands require explicit `--live`; REST/MCP live endpoints require
  `prepare` then `confirm`.
- `prepare` opens and verifies the broker confirmation dialog, then persists a
  `confirmation_opened` operation with a 30-second TTL, redacted payload, and
  `prepare` audit event; it returns a one-time confirmation token that is kept
  only in memory and never written to SQLite.
- `confirm` requires `(operation_id, confirmation_token, fingerprint,
  Idempotency-Key)` to match a `ConfirmationOpened` operation with the stored
  fingerprint and bound idempotency key. It transitions to `confirming` +
  `confirm_start` audit before the UI call, then to `confirmed` or `unknown` +
  respective audit after the UI call. Expired, already-consumed, or mismatched
  tokens are rejected (`409`/`422`/`400`).
- `abort` may close `confirmation_opened` / `expired` / `unknown` dialogs;
  it writes `abort` or `unknown` audits atomically. `unknown`/`expired`
  outcomes and any unresolved pending operation continue to block new `prepare`
  calls until explicitly resolved.
- TTL expiry is enforced in SQLite: `expire_stale_operations` marks
  `confirmation_opened`/`confirming` past `expires_at` as `expired` with
  `expired` audit. On startup, `reconcile_pending_operations` marks remaining
  `confirmation_opened`/`confirming` as `expired` (if past TTL) or `unknown`
  so an abandoned broker dialog cannot silently resume.
- Idempotency is durable: `idempotency_key` is unique in `operations` and
  survives restart; duplicate keys are rejected (`409`).
- SQLite mutexes are never held while performing Accessibility UI calls.
  State transitions are atomic with their audit row; confirmation tokens are
  cleared after any terminal transition.
- Amount-limit and exact-order verification remain enforced in the broker page
  layer (`submit_order`, `cancel_order`).
- All UI/OCR operations are serialized via a single async mutex across CLI,
  MCP, and REST, and the Tauri background server shares the same `OperatorService`
  instance.

## Versioning

- Crate version `0.2.12` (inherited from parent workspace) is the protocol
  version anchor for this extract. `Info.plist` is stamped from this version.
- HTTP/MCP paths above are the compatibility boundary for this phase.
  Additive, backward-compatible additions (e.g., history/status endpoints) are
  allowed in later phases; breaking removals require a major version bump.
- Dependency pins are recorded in `Cargo.toml` from the parent workspace
  versions (axum 0.8.9, rmcp =3.1.1, tokio 1.53.1, etc.) plus Tauri v2,
  `tauri-plugin-single-instance`, `keyring`, and `reqwest` for the desktop phase.

## Audit and redaction

- Every `prepare`, `confirm_start`, `confirmed`, `abort`, `expired`, and
  `unknown` transition appends an `audit_events` row with `operation_id`, kind,
  from/to state, RFC 3339 timestamps, fingerprint, optional actor (`http`/`mcp`/`
  system:startup`/`system:expiry`), and a redacted JSON detail. No row stores
  `confirmation_token`, `STOCK_OPERATOR_AUTH_TOKEN`, `Authorization` headers, or
  raw payloads containing secrets. Payload summaries in `operations` and audit
  `detail` contain only trading fields (security code, side, price, quantity)
  and fingerprint bindings.
- Retrieval endpoints (`GET /api/v1/operator/operations`, `GET /api/v1/operator/audit/events`,
  and `GET /api/v1/operator/operations/{id}`) hide confirmation tokens except
  for an immediately-prepared `confirmation_opened` operation within the same
  process; after restart or terminal states the token is absent.
- Tauri history/audit commands and the `ui/` history tables show only redacted
  summaries and never expose bearer or confirmation tokens.

## Topology

Default binding remains loopback (`127.0.0.1:5190`). The Linux main service talks
to the Mac operator over this loopback when co-located, or via an explicit
private-network configuration otherwise. Cross-machine use requires a private
overlay (e.g., Tailscale/WireGuard), reverse proxy, or TLS-terminating tunnel
that preserves the bearer token and does not expose plaintext HTTP to the public
internet. Plaintext cross-machine HTTP is not safe and is not supported in this
phase; authenticated encrypted transport is phase 4. The desktop shows bind/MCP
settings and marks network changes as restart-required without pretending hot
reload works.

## Explicitly out of scope for this phase

- **Cross-machine authenticated encrypted transport**: loopback-only remains
  enforced; private overlay/TLS proxy is documented above and implemented in
  phase 4.
- **Generated build output / lockfiles**: `target/`, `.app` bundles, and
  `Cargo.lock` are not committed.

SQLite persistence, durable live-operation state, audit history, Tauri desktop
shell, double-click bundling with `ui/`, and Keychain token storage are now
implemented; cross-machine hardening and CI release artifacts remain next.

## References

- Parent source: `/Volumes/Enterprise/codes/research/stock-goes-stonk/apps/stock-operator`
- Bundle identifier: `com.cloudiful.stock-operator` / `com.cloudiful.stock-operator.window-ocr`
- Default process: `中信证券网上交易` (`com.citics.mac.tdx`)
- SQLite: `~/Library/Application Support/Stock Operator/operator.sqlite3`
- Keychain service: `com.cloudiful.stock-operator` / `operator-bearer-token`
