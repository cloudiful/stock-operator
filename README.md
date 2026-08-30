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
& Security -> Accessibility**, then run:

```sh
STOCK_OPERATOR_AUTH_TOKEN='replace-with-a-local-token' cargo run
```

`STOCK_OPERATOR_AUTH_TOKEN` is required for the long-running MCP server. If it
is missing, the process exits with an explicit configuration error. The token
is not needed by the one-shot read-only commands below.

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
see `docs/protocol.md` for cross-machine guidance (future phase).

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

Build a stable local app bundle with ad-hoc signing:

```sh
nu package.nu
```

The bundle is written to `target/stock-operator/Stock Operator.app` with bundle
identifier `com.cloudiful.stock-operator`. Pass `--identity` to use an installed
Apple code-signing identity. Grant Accessibility and Screen Recording permission
to this bundle before long-running use.

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

## Protocol and versioning

See `docs/protocol.md` for the HTTP/MCP compatibility boundary, versioning, and
live-operation safety gates.

## Project status

This is phase 1 of the standalone Tauri extraction (`standalone-extract`).
SQLite persistence, audit/history endpoints, and the Tauri desktop UI are
intentionally not yet implemented; those belong to subsequent phases. This
repository retains the existing in-memory live-operation state, loopback-only
binding, and macOS Accessibility/OCR behavior from the parent.

No secrets or bearer tokens are persisted; `STOCK_OPERATOR_AUTH_TOKEN` remains
environment-only.
