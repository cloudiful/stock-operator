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

Remote: `https://forgejo.cloud1ful.com/research/stock-operator` (private, no auto-init; `Cargo.toml` repository field points to this canonical Forgejo path). If a GitHub mirror is used, the Forgejo URL remains canonical for `git remote`.

## Run

Grant Accessibility permission to the **Stock Operator** app bundle in
**System Settings -> Privacy & Security -> Accessibility**, then run the
desktop app:

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

At startup the app does a non-prompting Accessibility status check
(`AXIsProcessTrusted` / `is_process_trusted`, surfaced via `accessibility_status`
and the desktop Status card) and does not repeatedly open the system permission
dialog. If permission is not granted, add the entry manually in
**System Settings -> Privacy & Security -> Accessibility** and switch it on.

macOS Accessibility permission is identity-specific (TCC): grant the exact
released `.app` bundle for normal use, or the exact debug executable only when
intentionally running `cargo run`:

- Released app (recommended): `Stock Operator.app` bundle identifier
  `com.cloudiful.stock-operator` — grant this bundle (the installed
  `Stock Operator.app`, not a bare binary)
- Debug `cargo run` (when intentionally testing): `target/debug/stock-operator`
  — a rebuilt ad-hoc-signed debug binary has a new code identity and may
  require re-granting, so prefer the stable signed `.app` before long-running
  use

`cargo run` with no arguments loads the local Vue UI from `ui/dist`
via `tauri.conf.json` `frontendDist: "ui/dist"`; no `http://localhost:5173` dev
server is used in the normal static launch path. The UI is Vue 3 + Vite +
TypeScript + `vue-i18n` (Composition API, no Nuxt/SSR). Build it with Bun:

```sh
cd ui
bun install   # do not commit bun.lock
bun run typecheck
bun run test
bun run build # -> ui/dist/index.html + assets (base './', relative for Tauri)
```

`tauri.conf.json` `build.beforeBuildCommand` is `bun --cwd ui run build`
so `cargo tauri build` also builds the frontend. `nu package.nu` builds the
frontend before Rust packaging and verifies `ui/dist/index.html` exists; both
`cargo run` and the packaged `.app` use the same local assets —
`tauri.conf.json` does not set `build.devUrl`. `ui/dist` and
`ui/node_modules` are ignored.

Language: `zh-CN` / `en` — initial locale follows `navigator.language`
(`zh` → `zh-CN`, otherwise `en`) with a visible selector; switching updates
the UI immediately without restart and persists in `localStorage` under
`stock-operator.locale` (non-secret). Theme: `system` / `light` / `dark`
(default `system`) with a visible selector; system mode reacts to
`prefers-color-scheme` changes, persists in `localStorage` under
`stock-operator.theme`, and is applied before first paint via the
CSP-compatible external asset `ui/dist/theme-bootstrap.js` (no inline script;
current CSP `script-src 'self'` unchanged).

The MCP endpoint defaults to:

```text
http://127.0.0.1:5190/mcp
```

When a private overlay is used, the endpoint is `http://<private-bind>:5190/mcp`
where `<private-bind>` is a private-network address and `mcp_path` remains
`/mcp` unless reconfigured. Register that endpoint in the stock-goes-stonk AI
MCP settings with the same static Bearer token. The server binds loopback by
default and refuses public/unspecified binds; see “Cross-machine” below and
`docs/protocol.md` for private-overlay requirements.

## Install from Release

Tagged releases (`v*`) publish macOS `.app` archives with the OCR helper and
`SHA256SUMS`.

```sh
# Download from GitHub Releases or Forgejo Releases
# Example GitHub:
#   stock-operator-v0.2.12-aarch64-apple-darwin.app.zip  (Apple Silicon M4, arm64)
#   stock-operator-v0.2.12-x86_64-apple-darwin.app.zip    (Intel x86_64)
# Each has a matching .app.zip.sha256 and combined SHA256SUMS in the release.

# Verify checksum (GitHub example)
shasum -a 256 -c SHA256SUMS
# or
shasum -a 256 stock-operator-v0.2.12-aarch64-apple-darwin.app.zip
# compare with SHA256SUMS entry

# Unzip and run — keep the .app bundle intact (TCC/OCR permissions require the bundle, not a bare binary)
unzip stock-operator-v*.app.zip
open "Stock Operator.app"
# or double-click Stock Operator.app in Finder

# First launch: macOS Gatekeeper may block an ad-hoc-signed build
# Right-click -> Open, or: xattr -dr com.apple.quarantine "Stock Operator.app"

# Update: quit the running app, replace the .app bundle, reopen. SQLite and Keychain persist across updates.
```

The bundle identifier is `com.cloudiful.stock-operator` with `LSMinimumSystemVersion 26.0`.
The helper is at `Contents/Helpers/window-ocr` inside the bundle. Grant
Accessibility and Screen Recording permission to **this bundle** (not the debug
binary) before long-running use; a rebuilt debug binary has a new ad-hoc
identity, so re-grant permissions to the released `.app`.

## Cross-machine Linux → Mac operator

Default topology is loopback. The Linux `stock-goes-stonk` main service talks
to the operator at `http://127.0.0.1:5190` when co-located, or via an explicit
private-network configuration otherwise. The macOS GUI and Accessibility/OCR
always run locally on the Mac; only the bearer-authenticated MCP/HTTP API is
remoted.

### Private-overlay mode (explicit, authenticated)

Non-loopback binds are **only** permitted when:

1. `network_mode` is `private-overlay` (env `STOCK_OPERATOR_NETWORK_MODE=private-overlay` or desktop Connection → Network mode → Private overlay), **and**
2. `bind_addr` is a private-network address (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, CGNAT `100.64.0.0/10` including Tailscale 100.x, ULA `fc00::/7`, link-local), **and**
3. bearer auth is present (`STOCK_OPERATOR_AUTH_TOKEN` env or Keychain token), **and**
4. the transport is encrypted or private: Tailscale, WireGuard, or an authenticated TLS-terminating reverse proxy on the private network.

Public or unspecified binds (`0.0.0.0`, `::`, `8.8.8.8`, etc.) are rejected in
both modes. Plaintext HTTP over the public internet is not supported and is
not to be tunneled without TLS/overlay. Loopback remains the safe default.

### Environment / config examples

```sh
# Loopback (default) — no extra config
STOCK_OPERATOR_BIND_ADDR=127.0.0.1:5190
STOCK_OPERATOR_MCP_PATH=/mcp
STOCK_OPERATOR_AUTH_TOKEN='< bearer token >'
STOCK_OPERATOR_MAIN_SERVICE_URL=http://127.0.0.1:3000  # optional, for desktop health probe

# Private overlay via Tailscale (Mac has 100.x address)
STOCK_OPERATOR_NETWORK_MODE=private-overlay
STOCK_OPERATOR_BIND_ADDR=100.64.12.34:5190
STOCK_OPERATOR_AUTH_TOKEN='< bearer token >'
STOCK_OPERATOR_MCP_PATH=/mcp
# Linux main service then uses http://100.64.12.34:5190/mcp with Authorization: Bearer ...

# Private overlay via WireGuard private address
STOCK_OPERATOR_NETWORK_MODE=private-overlay
STOCK_OPERATOR_BIND_ADDR=10.47.0.5:5190
STOCK_OPERATOR_AUTH_TOKEN='< bearer token >'

# Private overlay behind authenticated TLS reverse proxy on private LAN
STOCK_OPERATOR_NETWORK_MODE=private-overlay
STOCK_OPERATOR_BIND_ADDR=192.168.1.20:5190
STOCK_OPERATOR_AUTH_TOKEN='< bearer token >'
# Put a TLS proxy (e.g. Caddy/nginx with mTLS or token forwarding) in front;
# do not expose the operator plaintext port to the public network.
```

Persisted settings (SQLite, when env overrides are absent) also support
`network_mode` (`loopback` | `private-overlay`) alongside `bind_addr`,
`mcp_path`, `stock_main_service_url`, broker target, and traversal limits.
Changing network mode or bind requires explicit acknowledgement in the desktop
UI (checkbox) and a restart. Secrets are never persisted in SQLite.

### Desktop UI for cross-machine

Connection tab:

- **Network mode** selector: `Loopback only` vs `Private overlay (...)`
- When `Private overlay` is selected, a warning is shown and an acknowledgement
  checkbox must be ticked — without it, saving is rejected. This prevents a
  generic toggle from silently exposing the listener.
- **Bind address** and **MCP path** follow the mode validation above and are
  marked restart-required.
- The status card shows `Server` (`running` / `server not running` with error
  reason), `Bind`, `MCP`, and `Restart` details.

### Firewall / ACL guidance

- On the Mac, allow the operator port only on the private interface / overlay:
  e.g. `socketfilterfw` or `pf` rules that permit `100.64.0.0/10` or `10.x` /
  WireGuard subnet and deny public ingress. Do not add a `0.0.0.0` allow rule.
- On Tailscale, use ACLs to allow only the Linux host(s) to reach the Mac on
  `TCP 5190`, and keep `STOCK_OPERATOR_AUTH_TOKEN` required.
- On WireGuard, restrict `AllowedIPs` and firewall to the two peers.
- If using a reverse proxy, terminate TLS at the proxy, forward with the same
  `Authorization: Bearer` header, and keep the operator bind on `127.0.0.1` or a
  private address that is not published.

### TLS / overlay / token requirements

- Tailscale and WireGuard already encrypt and authenticate the overlay; the
  bearer token is still required at the MCP/HTTP layer (`Authorization: Bearer`)
  and is never logged or stored in SQLite/audit events.
- Private LAN reverse proxy must provide TLS (valid cert) and preserve the
  bearer header; do not run plaintext HTTP across a shared or public LAN.
- Tokens live in env or macOS Keychain (`service com.cloudiful.stock-operator`,
  account `operator-bearer-token`), never in `operator.sqlite3`. Env takes
  precedence. The desktop shows only `configured`/`source` (env/keychain/none).
- Rotate tokens by saving a new value in the desktop (Keychain) or replacing
  `STOCK_OPERATOR_AUTH_TOKEN`; clearing a token stops the server until a new
  one is supplied.

### macOS GUI stays on the Mac

Accessibility and Screen Recording permissions, the trading application window,
and the OCR helper (`window-ocr`) all execute locally on the Mac. The Linux
service never drives the broker UI directly — it calls the operator over the
bearer-authenticated network and the operator serializes a single UI operation
locally.

## Desktop

The Tauri v2 desktop shell (`tauri.conf.json`, `ui/dist`) is Vue 3 + Vite +
TypeScript. `ui/src/App.vue` composes `ConnectionPanel` and `HistoryPanel`
with composables for Tauri invoke, `vue-i18n`, and theme; non-generated files
stay cohesive and below ~300 lines. The app launches by double-click and starts
the authenticated HTTP/MCP server in the background when a token is configured.
Single-instance is enforced so re-opening the app focuses the existing window
instead of competing with the running server.

- **Connection** tab: stock main-service URL, operator network mode/bind/MCP/
  broker bundle and process settings, traversal limits, bearer-token status and
  secure entry (Keychain), save/test controls, server and Accessibility status,
  restart-required notice, instance identifier. Network mode changes require
  explicit safety acknowledgement. Language and theme selectors are in the
  topbar.
- **History** tab: paginated operation history (kind/state/time/payload summary),
  audit event view with operation filter, refresh and empty/loading/error states.
  Stale `unknown`/`expired` rows show a "Resolve stale" button that requires explicit confirmation that the broker dialog is closed before writing the supervised `stale_resolved` audit.

All visible labels, status values, buttons, empty/error messages, and the stale
confirmation are translated in `zh-CN` and `en`; switching locale updates the
UI immediately. Theme `system`/`light`/`dark` covers page, topbar, cards,
controls, banners, tables, warnings, borders, and status colors; `system`
reacts to `prefers-color-scheme` and the choice persists in
`localStorage` (`stock-operator.theme` / `stock-operator.locale`, non-secret)
and is applied before first paint via `ui/dist/theme-bootstrap.js`.

Tauri commands (`window.__TAURI__.core.invoke`) are narrow and typed and
unchanged: `get_settings`, `save_settings`, `get_runtime_status`,
`get_token_status`, `save_token`, `clear_token`, `test_stock_service_url`,
`list_operations` (`limit`/`offset`/`kind`/`stateFilter`), `list_audit_events`
(`limit`/`offset`/`operationId`), `resolve_stale_operation`
(`operation_id` + compat `operationId` → `aborted` with `stale_resolved` audit
and token clear; requires explicit dialog-closed acknowledgement). Browser
fallback stays read-only: mutations disabled, tables safe via text interpolation
(`{{ }}`), never `v-html`. Secrets are never returned or stored in SQLite.

Ordinary settings are persisted in SQLite and reloaded on restart when env
overrides are absent. Presentation preferences (`locale`, `theme`) are the only
`localStorage` keys and never touch secrets. URL, socket address, paths, network
mode, and numeric bounds are validated; private-overlay mode requires
acknowledgement. Settings that affect the running HTTP server or Accessibility
inspector are reported as restart-required.

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

Build a stable local app bundle with ad-hoc signing (Tauri shell + OCR helper).
A fresh checkout must build the frontend first (Bun):

```sh
cd ui && bun install && bun run build   # -> ui/dist/index.html + theme-bootstrap.js + assets
# or let package.nu do it:
nu package.nu
# with explicit target (for CI cross builds)
nu package.nu --target aarch64-apple-darwin
nu package.nu --target x86_64-apple-darwin
```

The bundle is written to `target/stock-operator/Stock Operator.app` (or
`target/<target>/stock-operator-…/Stock Operator.app` when `--target` is used)
with bundle identifier `com.cloudiful.stock-operator` and
`LSMinimumSystemVersion 26.0`. The script builds the Vue frontend with Bun,
then the release Tauri binary, embeds `ui/dist`, and adds
`Contents/Helpers/window-ocr` (Vite `base: './'` gives relative assets for the
custom protocol). If a `target/release/bundle/macos` Tauri bundle already
exists, it is reused and the helper is injected. Pass `--identity` to use an
installed Apple code-signing identity. Grant Accessibility and Screen Recording
permission to this bundle before long-running use. Release archives are
`.app.zip` created with `ditto -c -k --keepParent` so macOS preserves bundle
metadata; a bare binary is not sufficient because `window-ocr` and TCC
permissions depend on the bundle.

## Storage

SQLite stores ordinary operator settings and durable operation history at
`~/Library/Application Support/Stock Operator/operator.sqlite3` (configurable
via `STOCK_OPERATOR_DB_PATH`). Parent directories are created and
`migrations/0001_initial.sql` + `0002_stale_resolve.sql` are applied at startup; a clear error is reported
if SQLite cannot initialize.

Ordinary settings persisted through the desktop UI and SQLite APIs are:
stock main-service URL (`STOCK_OPERATOR_MAIN_SERVICE_URL`),
`STOCK_OPERATOR_BIND_ADDR`, `STOCK_OPERATOR_MCP_PATH`,
`STOCK_OPERATOR_TARGET_BUNDLE_ID`/`STOCK_OPERATOR_TARGET_PROCESS_NAME`,
traversal limits (`STOCK_OPERATOR_MAX_DEPTH`/`STOCK_OPERATOR_MAX_NODES`),
network mode (`STOCK_OPERATOR_NETWORK_MODE` = `loopback` | `private-overlay`),
and a generated `operator_instance_id` (`STOCK_OPERATOR_INSTANCE_ID` may
configure it). On restart, persisted settings are used when env overrides are
absent. `network_mode` defaults to `loopback` and requires explicit
acknowledgement when set to `private-overlay`.

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
blocks new live operations until explicitly resolved via the desktop-only
`resolve_stale_operation` Tauri command (History tab "Resolve stale" button) —
which requires explicit acknowledgement that the broker dialog is closed, never
submits/confirms the dialog, writes a distinct `stale_resolved` audit (`unknown`/`expired` → `aborted`) and clears the in-memory token. HTTP/MCP `abort` remains broker-dialog-driven; the recovery is intentionally desktop-only.

Use an isolated path for development/tests:

```sh
STOCK_OPERATOR_DB_PATH=/tmp/operator-test.sqlite3 cargo test
```

## HTTP API

Server mode exposes MCP and REST on the same listener (loopback by default,
private-overlay when explicitly configured):

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
The MCP endpoint path is `STOCK_OPERATOR_MCP_PATH` (default `/mcp`) appended to
`STOCK_OPERATOR_BIND_ADDR`; `operator_health` reports `endpoint_scope`
`loopback_only` vs `private_overlay` so callers can distinguish the mode.
Export the OpenAPI 3.1 document without starting the server:

```sh
target/debug/stock-operator inspect openapi
```

All UI and OCR operations are serialized across CLI, MCP, and REST. Live REST
operations use a two-step state machine: `prepare` opens and verifies the broker
dialog, then returns a 30-second one-time confirmation token and payload
fingerprint. `confirm` requires that token, fingerprint, the original
`Idempotency-Key`, and an operation that is still awaiting confirmation. An
unknown confirmation outcome is not retryable. Use the `abort` endpoint to close
a still-open confirmation dialog while the broker dialog is still present; after a
restart the dialog is gone and `abort` will fail — use the desktop History tab
"Resolve stale" (Tauri `resolve_stale_operation`, `unknown`/`expired` → `aborted` with `stale_resolved` audit) after verifying the dialog is closed. An unknown, expired, or otherwise unresolved operation blocks new prepares until explicitly resolved via the supervised desktop action.

The history endpoints power the Tauri UI; they expose the SQLite-backed
operation state after restart. The MCP read-only tool `list_recent_operations`
provides the same redacted history for AI use and does not expose tokens.

## Protocol and versioning

See `docs/protocol.md` for the HTTP/MCP compatibility boundary, versioning, and
live-operation safety gates. The stock main-service URL is `STOCK_OPERATOR_MAIN_SERVICE_URL`
(alias `STOCK_OPERATOR_STOCK_SERVICE_URL` / `STOCK_MAIN_SERVICE_URL` for legacy),
the MCP path defaults to `/mcp`, and `operator_health` is the compatibility
probe.

## CI and Release

- **CI** — GitHub `.github/workflows/ci.yml` runs on `macos-15` (PRs, pushes to `main`, tags `v*`, manual dispatch) and exercises the full macOS crate: installs Bun and runs `bun --cwd ui install` + `bun --cwd ui run typecheck` + `bun --cwd ui run test` + `bun --cwd ui run build` and checks `ui/dist/index.html`/`assets` (relative base), then `cargo fmt --all -- --check`, `SQLX_OFFLINE=true cargo check --all-targets` + `cargo test --all-targets` (97+ tests), plus static validation of `tauri.conf.json` (`frontendDist: "ui/dist"`, no `devUrl`, IPC CSP `ipc:` + `http://ipc.localhost`, `CFBundleIconFile` without `LSUIElement`), `capabilities/default.json`, `ui/dist` assets, icons, `package.nu` (frontend build + absolute-path and empty-glob guards, icon copy to `Resources`), and migrations (`0001` + `0002` `stale_resolved`). Forgejo `.forgejo/workflows/ci.yml` on Linux `aio` has no macOS toolchain and would compile only the non-macOS stub with 0 tests, so it runs Bun frontend checks when Bun is available and otherwise checks source metadata plus formatting/static assets/docs validation with an explicit note that full Rust validation is on GitHub `macos-15`; it does not claim full Rust tests. No secrets are included in either workflow.

- **Release** on strict `v*` tags (`.github/workflows/release.yml`) builds both
  macOS targets on GitHub-hosted macOS runners with explicit triples:
  `aarch64-apple-darwin` on `macos-15` (Apple Silicon M4) and
  `x86_64-apple-darwin` on `macos-13` (Intel). Each job installs Bun, runs
  `bun --cwd ui run build` (via `beforeBuildCommand` and `package.nu`), then
  `nu package.nu --target <triple> --output target/stock-operator-<triple>/Stock Operator.app`,
  verifies `Contents/MacOS/stock-operator` + `Contents/Helpers/window-ocr` +
  `Contents/Info.plist` + `codesign --verify`, then creates a clickable
  `.app.zip` via `ditto -c -k --keepParent` (documented reliable equivalent is
  `zip -r` if `ditto` is unavailable). The bundle is required — a bare binary
  is not sufficient because OCR/TCC depend on the bundle. `SHA256SUMS` and per-archive
  `.sha256` are generated and all archives plus checksums are published to a
  GitHub Release.

- **Forgejo release** (`.forgejo/workflows/release.yml`) is validation-focused on
  Linux (`aio`) for now and confirms the release assets and GitHub workflow
  triples. It includes a commented optional `build-macos-optional` job that can
  be enabled once a self-hosted macOS runner with Xcode/Swift is available.
  No secrets are included in either workflow.

Verify a release locally with `shasum -a 256 -c SHA256SUMS`.

## Project status

This is phase 5 (`vue-i18n-theme`) of the standalone Tauri extraction.
Vue 3 + Vite + TypeScript + `vue-i18n` migration is complete: componentized
`App.vue`/`ConnectionPanel`/`HistoryPanel`, `zh-CN`/`en` i18n with
`navigator.language` default and immediate selector, `system`/`light`/`dark`
theme with `localStorage` persistence (`stock-operator.locale` /
`stock-operator.theme`), system `prefers-color-scheme` reactivity, and
CSP-compatible external bootstrap (`theme-bootstrap.js`) covering page/topbar/
cards/controls/banners/tables/warnings/borders/status colors without white
flash. All current Connection/History capabilities and Tauri invoke shapes
(`get_settings`, `get_runtime_status`, `save_settings`, `save_token`,
`clear_token`, `test_stock_service_url`, `list_operations`, `list_audit_events`,
`resolve_stale_operation` with compat `operation_id`/`operationId`, single-instance
refresh, browser fallback read-only) remain unchanged. SQLite persistence,
durable live-operation state, audit history, private-overlay mode, and bundling
with `ui/dist` are retained. CI now validates the Vue frontend before Rust.
