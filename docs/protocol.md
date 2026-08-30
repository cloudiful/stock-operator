# Protocol and compatibility

## Scope

This document records the HTTP/MCP boundary preserved by the standalone
`stock-operator` extract and explicitly scopes work that remains for later
phases. It does not claim features that are not yet implemented.

## Current implementation (phase 1: standalone-extract)

- **Source**: Extracted from `stock-goes-stonk/apps/stock-operator` at
  `25abae0b9353d75d6b3bde8066090b0cd569349f`, including uncommitted preflight
  files (`trade_preflight`, `trade_preflight_validation`,
  `trade_preflight_tests` and current staged edits).
- **Platform**: macOS-only. Non-macOS builds emit a stub and exit; macOS builds
  use Accessibility (`axuielement`), ScreenCaptureKit/Vision OCR helper
  (`macos/window_ocr.swift`), bundle identifier
  `com.cloudiful.stock-operator`, and single-instance UI serialization.
- **Config**: Environment-only (`STOCK_OPERATOR_AUTH_TOKEN`,
  `STOCK_OPERATOR_BIND_ADDR`, `STOCK_OPERATOR_MCP_PATH`,
  `STOCK_OPERATOR_TARGET_BUNDLE_ID`, etc.). Default bind is loopback
  `127.0.0.1:5190`; non-loopback binds are rejected in this phase.
- **Build**: `build.rs` compiles the OCR helper via `xcrun swiftc` when on
  macOS; missing `swiftc`/SDK is a warning, not a hard error.
- **Package**: `nu package.nu` builds the release binary and helper and
  produces `target/stock-operator/Stock Operator.app` with ad-hoc signing.

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
```

`/healthz` and `/api/openapi.json` are public. All other paths require
`Authorization: Bearer <token>` matching `STOCK_OPERATOR_AUTH_TOKEN`.

OpenAPI is exportable without starting the server:
`target/debug/stock-operator inspect openapi`.

## MCP surface

Streamable-HTTP transport at `POST /mcp` with `rmcp` 3.1.1. Tools are
read-only except for the gated live operations exposed via CLI/REST (not MCP).
Current tools include: `operator_health`, `accessibility_status`,
`inspect_target_app`, `read_trade_snapshot`, `ui_inventory`, `current_view`,
`read_trade_form`, `read_positions*`, `read_orders*`, `read_executions*`,
`read_funds*`, `diagnose_positions_table`, `navigation_candidates`,
`navigate_readonly`, `ocr_visible_text`, `trade_preflight`, `stage_order`.

## Live-operation safety gates (unchanged)

- CLI live commands require explicit `--live`; REST live endpoints require
  `prepare` then `confirm`.
- `prepare` opens and verifies the broker confirmation dialog, returns a
  30-second one-time confirmation token and SHA-256 payload fingerprint.
- `confirm` requires `(operation_id, confirmation_token, fingerprint,
  Idempotency-Key)` to match an operation in `ConfirmationOpened` state.
  Expired, already-consumed, or mismatched tokens are rejected (`409`/`422`).
- Unknown confirmation outcome is non-retryable; the single active live
  operation blocks new live operations until aborted or explicitly resolved.
- Amount-limit and exact-order verification remain enforced in the broker page
  layer (`submit_order`, `cancel_order`).
- All UI/OCR operations are serialized via a single async mutex across CLI,
  MCP, and REST.

## Versioning

- Crate version `0.2.12` (inherited from parent workspace) is the protocol
  version anchor for this extract. `Info.plist` is stamped from this version.
- HTTP/MCP paths above are the compatibility boundary for this phase.
  Additive, backward-compatible additions (e.g., history/status endpoints) are
  allowed in later phases; breaking removals require a major version bump.
- Dependency pins are recorded in `Cargo.toml` from the parent workspace
  versions (axum 0.8.9, rmcp =3.1.1, tokio 1.53.1, etc.).

## Explicitly out of scope for this phase

The following are **not implemented** in `standalone-extract` and must not be
claimed as present:

- **SQLite persistence** for configuration, live operations, or audit/history.
  Current live state is in-memory only and does not survive restart.
- **Durable audit/history** beyond the existing in-memory tracing; no
  `history`/`status` persistence endpoints yet (only the current
  `GET /api/v1/operator/operations/{id}` for active in-memory ops).
- **Tauri shell/UI** beyond the existing `.app` packaging via `package.nu`.
  Double-clickable bundling still uses `package.nu`; Tauri frontend,
  settings UI, and Keychain secret storage are later phases.
- **Cross-machine transport**: loopback-only remains enforced; authenticated
  encrypted transport / overlay / reverse-proxy is a documented future
  requirement, not yet implemented.
- **Secrets in storage**: bearer tokens are not persisted anywhere (no SQLite,
  no Keychain integration yet beyond the environment contract).

These items are tracked as subsequent phases; this document will be updated
when they land.

## References

- Parent source: `/Volumes/Enterprise/codes/research/stock-goes-stonk/apps/stock-operator`
- Bundle identifier: `com.cloudiful.stock-operator` / `com.cloudiful.stock-operator.window-ocr`
- Default process: `中信证券网上交易` (`com.citics.mac.tdx`)
