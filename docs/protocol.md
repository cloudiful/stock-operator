# Protocol and compatibility

## Scope

This document records the HTTP/MCP boundary preserved by the standalone
`stock-operator` extract and the cross-machine topology. It does not claim
features that are not yet implemented.

## Current implementation (phase 4: network-release)

- **Source**: Extracted from `stock-goes-stonk/apps/stock-operator` at
  `25abae0b9353d75d6b3bde8066090b0cd569349f`, phases 2 (`b31e3b4`) and 3 (`e47e8a9`)
  baselines with SQLite/ Tauri.
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
  restart when env absent: `STOCK_OPERATOR_MAIN_SERVICE_URL` (aliases
  `STOCK_OPERATOR_STOCK_SERVICE_URL`, `STOCK_MAIN_SERVICE_URL`), `bind`/`MCP path`,
  `STOCK_OPERATOR_NETWORK_MODE` (`loopback` | `private-overlay`), `STOCK_OPERATOR_TARGET_BUNDLE_ID`/
  `STOCK_OPERATOR_TARGET_PROCESS_NAME`, traversal limits, and `operator_instance_id`.
  URL, socket address, network mode, paths, and numeric bounds are validated;
  private-overlay mode requires explicit acknowledgement in the desktop UI. Server/
  inspector-affecting changes are reported as restart-required.
- **Network mode**: `loopback` (default) allows only `127.0.0.1`/`::1`; `private-overlay`
  additionally allows private addresses (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`,
  CGNAT `100.64.0.0/10` including Tailscale 100.x, ULA `fc00::/7`, link-local `fe80::/10`/`169.254.0.0/16`).
  Unspecified (`0.0.0.0`/`::`) and public addresses are rejected in both modes.
  Non-loopback in private-overlay also requires bearer auth (env or Keychain) at
  startup and an encrypted overlay or TLS reverse proxy; plaintext HTTP over the
  public network is rejected by policy. See Topology below.
- **Storage**: `rusqlite 0.32.1` (bundled) behind synchronous repository
  (`src/storage/` module tree: `mod.rs`, `operations.rs`, `transitions.rs`, `audit.rs`, `settings.rs`, `redaction.rs`), migrations in `migrations/0001_initial.sql` + `0002_stale_resolve.sql` (adds `stale_resolved` audit event for supervised stale recovery). Tables:
  `operator_settings`, `operations` (durable live state + redacted payload,
  one-time confirmation tokens never persisted), `audit_events`. Extra key
  `network_mode` added in phase 4 (default `loopback`). Bearer tokens live in
  Keychain; Keychain unavailability returns a clear non-secret error without
  plaintext fallback. Redacted summaries store only security code / side / price
  / quantity and fingerprint. `seed_from_config` now includes `network_mode`.
- **Build**: `build.rs` combines `tauri_build::try_build` (embedding
  `tauri.conf.json` + `ui/`) with the OCR helper `xcrun swiftc` when on macOS;
  missing `swiftc`/SDK or Tauri context is a warning, not a hard error. Helper
  target is `<arch>-apple-macosx26.0`.
- **Package**: `nu package.nu` builds the release Tauri binary, embeds `ui/`,
  and produces `target/stock-operator/Stock Operator.app` with ad-hoc signing
  and `Contents/Helpers/window-ocr`. With `--target aarch64-apple-darwin` or
  `--target x86_64-apple-darwin` the output is
  `target/stock-operator-<target>/Stock Operator.app` for cross-arch CI. When a
  `target/release/bundle/macos` Tauri bundle exists it is reused and the helper
  is injected. `ditto -c -k --keepParent` creates the release `.app.zip` so the
  bundle (required for TCC/OCR) is preserved; bare `zip -r` is a documented
  fallback.

## HTTP surface

Same listener for REST and MCP (loopback by default, private-overlay when
explicitly configured):

```
GET  /healthz                    (public, no account data)
GET  /api/openapi.json           (public, OpenAPI 3.1)
POST /mcp                        (Bearer required) — path is STOCK_OPERATOR_MCP_PATH, default /mcp
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
`Authorization: Bearer <token>` matching the env or Keychain token. The MCP
endpoint is `http://<bind_addr><mcp_path>`; `bind_addr` defaults to
`127.0.0.1:5190`. `operator_health` tool reports `endpoint_scope`
`loopback_only` vs `private_overlay` to distinguish the mode.

OpenAPI is exportable without starting the server:
`target/debug/stock-operator inspect openapi`.

Environment variables (all optional except auth token when starting server):

```
STOCK_OPERATOR_BIND_ADDR=127.0.0.1:5190              # default loopback; private address allowed only with private-overlay mode
STOCK_OPERATOR_NETWORK_MODE=loopback                 # or private-overlay / private / overlay / tailscale / wireguard (canonical private-overlay)
STOCK_OPERATOR_MCP_PATH=/mcp
STOCK_OPERATOR_AUTH_TOKEN=...                        # env preferred; desktop Keychain is alternative (service com.cloudiful.stock-operator, account operator-bearer-token)
STOCK_OPERATOR_MAIN_SERVICE_URL=http://127.0.0.1:3000  # aliases STOCK_OPERATOR_STOCK_SERVICE_URL, STOCK_MAIN_SERVICE_URL
STOCK_OPERATOR_TARGET_BUNDLE_ID=com.citics.mac.tdx
STOCK_OPERATOR_TARGET_PROCESS_NAME=中信证券网上交易
STOCK_OPERATOR_MAX_DEPTH=6                            # 1..12
STOCK_OPERATOR_MAX_NODES=300                          # 1..2000
STOCK_OPERATOR_DB_PATH=~/Library/Application\ Support/Stock\ Operator/operator.sqlite3
STOCK_OPERATOR_INSTANCE_ID=...                        # optional, generated if absent
```

Persisted SQLite keys mirror the env names in snake_case: `bind_addr`,
`mcp_path`, `network_mode`, `stock_main_service_url`, `target_bundle_id`,
`target_process_name`, `max_depth`, `max_nodes`, `operator_instance_id`. Secrets
are never in SQLite.

## Tauri command surface

Invoked via `window.__TAURI__.core.invoke`; browser fallback is minimal and
non-mutating:

```
get_settings              -> PublicSettings { stock_service_url, bind_addr, mcp_path, target_bundle_id, target_process_name, max_depth, max_nodes, instance_id, network_mode } (no secrets)
save_settings             -> SaveSettingsResponse { restart_required, reasons, message, settings }  # requires private_overlay_ack=true when network_mode=private-overlay or bind is non-loopback
get_runtime_status        -> RuntimeStatus { server_running, server_bind_addr, server_error, token_configured/source, accessibility, restart_required/reasons, instance_id, db_path }
get_token_status          -> { configured, source }  # env | keychain | none
save_token / clear_token  -> TokenStatus (Keychain, env-preferred)
test_stock_service_url    -> { ok, status, message, latency_ms }
list_operations           -> OperationHistoryResponse (paginated, redacted)
list_audit_events         -> AuditHistoryResponse (paginated, filtered)
resolve_stale_operation   -> { operation_id, previous_state, state, message }  # desktop-only, only unknown/expired -> aborted with stale_resolved audit; clears in-memory token; future prepares unblocked only after explicit call
```

`save_settings` validates `network_mode` (`loopback` | `private-overlay`,
aliases `private`/`overlay`/`tailscale`/`wireguard`), `bind_addr` against that
mode (unspecified/public rejected, private ranges allowed only in
private-overlay), and `private_overlay_ack`. Network mode and bind are
restart-required. No command returns or logs bearer/confirmation tokens; only
booleans/status.

## MCP surface

Streamable-HTTP transport at `POST <mcp_path>` (default `/mcp`) with `rmcp` 3.1.1.
Tools are read-only except for the gated live operations exposed via CLI/REST
(not MCP). Current tools include: `operator_health` (now reports `endpoint_scope`
`loopback_only` | `private_overlay`), `accessibility_status`,
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
- Desktop-only `resolve_stale_operation` (Tauri `resolve_stale_operation` + History UI "Resolve stale") is the supervised recovery for durable `unknown`/`expired` that permanently blocks new `prepare` calls (e.g., after restart the dialog is gone and the normal `abort` cannot drive the broker UI). It accepts only `unknown`/`expired`, never submits or confirms the broker dialog, requires explicit user acknowledgement that the dialog is closed, writes a distinct `stale_resolved` audit (state `unknown`/`expired` -> `aborted`) and clears any in-memory confirmation token. Future prepares are unblocked only after this explicit desktop action; there is no silent auto-resolve, drop, or time-window. HTTP/MCP `abort` remains broker-dialog-driven and unchanged; the recovery is intentionally exposed only to the local desktop.
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
  Additive, backward-compatible additions (e.g., history/status endpoints,
  `network_mode`) are allowed; breaking removals require a major version bump.
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
  summaries and never expose bearer or confirmation tokens. `network_mode` is
  public and may appear in settings/history status.

## Topology

Default binding is `127.0.0.1:5190` with `network_mode=loopback` (validated by
`validate_bind_for_mode` in `src/config.rs` and `src/desktop/validation.rs`).
The Linux main service reaches the Mac operator over loopback when co-located,
or via an explicit private-network configuration otherwise.

Private-overlay mode is for Tailscale / WireGuard / authenticated TLS reverse
proxy on a private LAN. It requires:

- `STOCK_OPERATOR_NETWORK_MODE=private-overlay` (or `private`/`overlay` alias),
  persisted as SQLite `network_mode` and selectable in the desktop
  **Connection → Network mode** with an explicit acknowledgement checkbox
  (without `private_overlay_ack=true`, saving is rejected, so a generic UI
  toggle cannot silently expose the listener);
- a private bind address (RFC1918 `10/8`, `172.16/12`, `192.168/16`, CGNAT
  `100.64/10`, ULA `fc00::/7`, link-local), not `0.0.0.0`/`::` or any public IP;
- the same bearer token on the Linux side (`Authorization: Bearer ...`),
  stored in env or macOS Keychain (`com.cloudiful.stock-operator` /
  `operator-bearer-token`), never in SQLite/audits;
- encrypted transport: Tailscale/WireGuard overlay or a TLS-terminating proxy
  that preserves the bearer header. Plaintext HTTP over the public internet or
  an open LAN without overlay/TLS is not supported and is rejected by policy.
- macOS Accessibility, Screen Recording, the broker window, and
  `Contents/Helpers/window-ocr` remain local on the Mac; the Linux side only
  calls the authenticated MCP/HTTP API. Firewall/ACL on the Mac should allow
  the operator port only on the overlay/private interface (e.g., Tailscale ACL
  for TCP 5190, WireGuard `AllowedIPs`, or `pf`/`socketfilterfw` rules) and
  deny public ingress.

Example cross-machine environment (Tailscale):

```
# Mac operator (100.x is the Mac's Tailscale address)
STOCK_OPERATOR_NETWORK_MODE=private-overlay
STOCK_OPERATOR_BIND_ADDR=100.64.12.34:5190
STOCK_OPERATOR_AUTH_TOKEN=<same token as Linux>
STOCK_OPERATOR_MCP_PATH=/mcp

# Linux main service (stock-goes-stonk) MCP config
# endpoint http://100.64.12.34:5190/mcp, Authorization: Bearer <same token>

# Verify no public exposure:
#   lsof -i :5190 | grep LISTEN  # should show only 100.64.x.x and 127.0.0.1, not 0.0.0.0
#   nmap -p5190 <mac-tailscale-ip>  # from allowed host only
```

Example behind private TLS proxy (no direct private bind needed exposure):

```
STOCK_OPERATOR_NETWORK_MODE=private-overlay
STOCK_OPERATOR_BIND_ADDR=192.168.1.20:5190  # private LAN only, not 0.0.0.0
# Proxy at https://operator.internal.example.com proxies to 192.168.1.20:5190
# with TLS and forwards Authorization: Bearer ...; firewall denies public 5190.
```

`operator_health` reports `endpoint_scope: "private_overlay"` when the mode is
private-overlay with a non-loopback bind, otherwise `loopback_only`.

## CI and release

- **CI**: `.github/workflows/ci.yml` (macOS `macos-15`) and
  `.forgejo/workflows/ci.yml` (intranet `aio` Linux) run on PRs, pushes to `main`,
  tags `v*`, and manual dispatch. GitHub `macos-15` runs the full crate: `cargo fmt --all -- --check`, `SQLX_OFFLINE=true cargo check --all-targets` + `cargo test --all-targets` (97+ macOS tests) and static validation of `tauri.conf.json` (now requires `ipc:` in CSP and `CFBundleIconFile` without `LSUIElement`), `capabilities/default.json`, `ui/`, `icons/`, `package.nu` (absolute-path guard and empty-glob guard fixed, icon copied to `Resources`), and migrations. Forgejo on Linux `aio` has no macOS toolchain and would compile only the non-macOS stub with 0 tests, so it runs **only** formatting, static asset, and documentation validation with an explicit note that full Rust validation is on GitHub `macos-15`; it does not claim full Rust tests. No secrets are included.
- **Release** on strict `v*` tags: `.github/workflows/release.yml` builds both
  macOS targets on GitHub-hosted runners with explicit triples
  `aarch64-apple-darwin` (`macos-15`, Apple Silicon M4) and
  `x86_64-apple-darwin` (`macos-13`, Intel). Each job runs
  `nu package.nu --target <triple> --output target/stock-operator-<triple>/Stock Operator.app`,
  verifies `Contents/MacOS/stock-operator`, `Contents/Helpers/window-ocr`,
  `Contents/Info.plist`, and `codesign --verify`, then
  `ditto -c -k --keepParent` to `stock-operator-<tag>-<target>.app.zip`
  (clickable `.app` archive; `zip -r` is a documented reliable equivalent).
  `SHA256SUMS` is generated and all archives + checksums are published to a
  GitHub Release. The bundle, not a bare binary, is the release unit because
  OCR/TCC require the bundle.
- **Forgejo release**: `.forgejo/workflows/release.yml` is test-focused on
  `aio` (Linux) because no macOS runner is currently provisioned on the
  intranet; it validates the same assets and documents that macOS `.app`
  packaging is performed on GitHub. A commented `build-macos-optional` job shows
  how to enable Forgejo macOS builds when a runner with Xcode/Swift is added.
  Shared Forgejo thin wrappers do not fit this Tauri + Swift helper project, so
  direct toolchain steps are used.

## Explicitly out of scope for this phase

- **Public internet exposure**: `0.0.0.0`/`::` and public IPs remain rejected;
  no direct public bind is supported.
- **Autonomous trading / arbitrary remote commands**: not implemented; live
  operations remain supervised with confirmation gates.
- **Generated build output / lockfiles**: `target/`, `.app` bundles, and
  `Cargo.lock` are not committed.

SQLite persistence, durable live-operation state, audit history, Tauri desktop
shell, double-click bundling with `ui/` and Keychain token storage, loopback
default, and explicit private-overlay mode with bearer auth and acknowledgement
are now implemented. CI and release workflows for both GitHub and Forgejo are
present.

## References

- Parent source: `/Volumes/Enterprise/codes/research/stock-goes-stonk/apps/stock-operator`
- Canonical Forgejo remote: `https://forgejo.cloud1ful.com/research/stock-operator`
- Bundle identifier: `com.cloudiful.stock-operator` / `com.cloudiful.stock-operator.window-ocr`
- Default process: `中信证券网上交易` (`com.citics.mac.tdx`)
- SQLite: `~/Library/Application Support/Stock Operator/operator.sqlite3`
- Keychain service: `com.cloudiful.stock-operator` / `operator-bearer-token`
- Default endpoint: `http://127.0.0.1:5190/mcp` (loopback) or `http://<private-ip>:5190/mcp` (private-overlay)
