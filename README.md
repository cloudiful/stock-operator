# stock-operator

Standalone macOS-only local operator for a configured desktop trading
application. Extracted from `stock-goes-stonk/apps/stock-operator` at
`25abae0b9353d75d6b3bde8066090b0cd569349f` including all current preflight
sources. This repository builds without parent workspace path dependencies.

Its authenticated MCP surface supports read-only account inspection,
broker-side trade preflight, and verified order staging; MCP tools do not
confirm, submit, or cancel orders. The binary also contains hidden,
tightly constrained one-shot CLI diagnostics used for supervised live
submission and cancellation testing.

## Run

Grant Accessibility permission to the process in **System Settings -> Privacy
& Security -> Accessibility**, then run the desktop app:

```sh
nu package.nu
open "target/stock-operator/Stock Operator.app"
```

Double-clicking `Stock Operator.app` with no CLI arguments opens the desktop
window. Existing CLI commands remain available; headless/server mode now
requires an explicit subcommand:

```sh
STOCK_OPERATOR_AUTH_TOKEN='replace-with-a-local-token' cargo run -- serve
# or
STOCK_OPERATOR_AUTH_TOKEN='...' target/debug/stock-operator serve
```

`STOCK_OPERATOR_AUTH_TOKEN` is required for the HTTP/MCP server. When launched
as a desktop app with no token configured, the window remains usable for
first-run setup (Connection tab shows “server not running”); saving a token
via Keychain starts the background server without an unsafe fallback.
Environment tokens take precedence over Keychain.

The OCR probe also requires **System Settings -> Privacy & Security -> Screen
Recording** permission for the `stock-operator` process and its macOS OCR
helper. The helper uses ScreenCaptureKit and Vision in memory and does not save
the captured window image.

At startup the process calls macOS `AXIsProcessTrustedWithOptions` with the
system prompt enabled. If macOS does not show a prompt, add the executable
manually in Accessibility settings. The current debug executable is:

```text
target/debug/stock-operator
```

After adding it, switch the entry on. A rebuilt debug executable can have a new
ad-hoc code identity, so a stable signed `.app` bundle should be used before
long-running or production-like use.

The MCP endpoint is:

```text
http://127.0.0.1:5190/mcp
```

Register that endpoint in the stock-goes-stonk AI MCP settings with the same
static Bearer token. The server refuses non-loopback bind addresses by default;
see `docs/protocol.md` for cross-machine guidance (phase 4).

## Desktop

The Tauri v2 desktop shell (`tauri.conf.json`, `ui/`) launches by double-click
and starts the authenticated HTTP/MCP server in the background when a token is
configured. Single-instance is enforced so re-opening the app focuses the
existing window instead of competing with the running server.

- **Connection** tab: stock main-service URL, operator bind/MCP/broker bundle and
  process settings, traversal limits, bearer-token status and secure entry
  (Keychain), save/test controls, server and Accessibility status, restart-required
  notice, instance identifier.
- **History** tab: paginated operation history (kind/state/time/payload summary),
  audit event view with operation filter, refresh and empty/loading/error states.

Tauri commands (`window.__TAURI__.core.invoke`) are narrow and typed:
`get_settings`, `save_settings`, `get_runtime_status`, `get_token_status`,
`save_token`, `clear_token`, `test_stock_service_url`, `list_operations`,
`list_audit_events`. Secrets are never returned or stored in SQLite.

Ordinary settings are persisted in SQLite and reloaded on restart when env
overrides are absent. URL, socket address, paths, and numeric bounds are
validated; loopback-only binding is preserved. Settings that affect the running
HTTP server or Accessibility inspector are reported as restart-required.

Browser preview without Tauri shows a read-only banner and disables mutations.

## Probe

Use the one-shot inspect commands without starting the MCP server:

```sh
cargo run -- inspect probe
```

The default target is the running process named `中信证券网上交易` with bundle
identifier `com.citics.mac.tdx`. Override the exact process name with
`STOCK_OPERATOR_TARGET_PROCESS_NAME` if the broker client uses another name.

The server exposes inspection tools including `operator_health`,
`accessibility_status`, `inspect_target_app`, `read_trade_snapshot`,
`ui_inventory`, `current_view`, `read_trade_form`, `read_positions`,
`read_positions_ocr`, `read_orders`, `read_orders_ocr`, `read_executions`,
`read_executions_ocr`, `read_funds`, `read_funds_ocr`,
`diagnose_positions_table`, `navigation_candidates`, `navigate_readonly`, and
`ocr_visible_text`, `trade_preflight`, and `stage_order`. Trade preflight reads the
broker security, form, quote, limits, funds, and trade status, and can compare the
broker price with a caller-provided backend reference quote without writing to the
form. Buy and sell staging can select a requested
security through the form's Accessibility code field, but the MCP tool never
confirms, submits, or cancels an order. The client may derive a new price and
buying/selling limit after security selection; the operator waits and revalidates
those values before writing. Successfully
staged price and quantity values remain visible in the live form for manual
review; clear or replace them when the order should not proceed.

The CLI uses layered subcommands so top-level help exposes only capability
groups. Run `stock-operator <group> --help` to reveal the next level.

```sh
target/debug/stock-operator inspect view
target/debug/stock-operator inspect inventory
target/debug/stock-operator inspect form
target/debug/stock-operator inspect ocr
target/debug/stock-operator inspect positions-table
target/debug/stock-operator inspect preflight

target/debug/stock-operator read positions
target/debug/stock-operator read orders --source ocr
target/debug/stock-operator read executions --source structured
target/debug/stock-operator read funds --source ax
target/debug/stock-operator read cancellations

target/debug/stock-operator navigate candidates
target/debug/stock-operator navigate positions
target/debug/stock-operator navigate executions

target/debug/stock-operator order select-security 600028
target/debug/stock-operator order stage \
  --security-code 600028 --side buy --price 5.06 --quantity 100
```

Live submission and cancellation commands are disclosed under `order --help`
and `cancel --help`. They require an explicit `--live` flag and retain the
broker-dialog, amount-limit, and exact-order verification checks.

## Package

Build a stable local app bundle with ad-hoc signing (Tauri shell + OCR helper):

```sh
nu package.nu
```

The bundle is written to `target/stock-operator/Stock Operator.app` with bundle
identifier `com.cloudiful.stock-operator` and `LSMinimumSystemVersion 26.0`.
The script builds the release Tauri binary, embeds `ui/`, and adds
`Contents/Helpers/window-ocr`. If a `target/release/bundle/macos` Tauri bundle
already exists, it is reused and the helper is injected. Pass `--identity` to
use an installed Apple code-signing identity. Grant Accessibility and Screen
Recording permission to this bundle before long-running use.

## Storage

SQLite stores ordinary operator settings and durable operation history at
`~/Library/Application Support/Stock Operator/operator.sqlite3` (configurable
via `STOCK_OPERATOR_DB_PATH`). Parent directories are created and
`migrations/0001_initial.sql` is applied at startup; a clear error is reported
if SQLite cannot initialize.

Ordinary settings persisted through the desktop UI and SQLite APIs are:
stock main-service URL (`STOCK_OPERATOR_MAIN_SERVICE_URL`),
`STOCK_OPERATOR_BIND_ADDR`, `STOCK_OPERATOR_MCP_PATH`,
`STOCK_OPERATOR_TARGET_BUNDLE_ID`/`STOCK_OPERATOR_TARGET_PROCESS_NAME`,
traversal limits (`STOCK_OPERATOR_MAX_DEPTH`/`STOCK_OPERATOR_MAX_NODES`), and a
generated `operator_instance_id` (`STOCK_OPERATOR_INSTANCE_ID` may configure it).
On restart, persisted settings are used when env overrides are absent.

Bearer tokens (`STOCK_OPERATOR_AUTH_TOKEN`) are stored in macOS Keychain
(service `com.cloudiful.stock-operator`, account `operator-bearer-token`), never
in SQLite. The desktop shows only a configured boolean/status; env tokens take
precedence and Keychain failures return a clear non-secret error without
falling back to plaintext.

Live operations and audit events are persisted with redacted summaries
(security code / side / price / quantity only, fingerprint retained). No
`Authorization` header or raw request payload containing secrets is stored. Writes
for state transitions are atomic with their audit event. SQLite access never
holds a mutex while performing Accessibility UI calls; pending
`ConfirmationOpened`/`Confirming` operations are reconciled on startup to
`Expired` (if TTL elapsed) or `Unknown` so an abandoned broker dialog cannot
silently resume, and any unresolved terminal `Unknown`/`Expired` operation still
blocks new live operations until explicitly aborted.

Use an isolated path for development/tests:

```sh
STOCK_OPERATOR_DB_PATH=/tmp/operator-test.sqlite3 cargo test
```

## HTTP API

Server mode exposes MCP and REST on the same loopback listener:

```text
GET  /healthz
GET  /api/openapi.json
POST /mcp
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
GET  /api/v1/operator/operations          # history, authenticated, ?limit&offset&kind&state
GET  /api/v1/operator/audit/events        # audit trail, authenticated, ?limit&offset&operation_id
```

`/healthz` and `/api/openapi.json` are public and contain no account data. MCP
and `/api/v1/operator/*` require the same `Authorization: Bearer ...` token.
Export the OpenAPI 3.1 document without starting the server:

```sh
target/debug/stock-operator inspect openapi
```

All UI and OCR operations are serialized across CLI, MCP, and REST. Live REST
operations use a two-step state machine: `prepare` opens and verifies the broker
dialog, then returns a 30-second one-time confirmation token and payload
fingerprint. `confirm` requires that token, fingerprint, the original
`Idempotency-Key`, and an operation that is still awaiting confirmation. An
unknown confirmation outcome is not retryable. Use the abort endpoint to close
a still-open confirmation dialog. An unknown or abandoned live operation blocks
new live operations until it is explicitly resolved.

The history endpoints power the Tauri UI; they expose the SQLite-backed
operation state after restart. The MCP read-only tool `list_recent_operations`
provides the same redacted history for AI use and does not expose tokens.

## Protocol and versioning

See `docs/protocol.md` for the HTTP/MCP compatibility boundary, versioning, and
live-operation safety gates.

## Project status

This is phase 3 (`tauri-desktop`) of the standalone Tauri extraction.
SQLite persistence, durable live-operation state, and audit history are complete;
the Tauri v2 desktop shell, double-clickable bundling with `ui/`, Keychain token
storage, and settings/status/history commands are now implemented. Loopback-only
binding is preserved; authenticated encrypted transport for Linux-to-Mac remains
phase 4. The repository retains macOS Accessibility/OCR behavior with durable
recovery and `Unknown`/`Expired` semantics across restart.

Linux main service to Mac operator topology is loopback by default; cross-machine
use requires an explicit private overlay or TLS proxy with the same bearer
token—plaintext HTTP over the public network is not supported.
