# Stock Operator

[简体中文](README.zh-CN.md)

Stock Operator is a macOS desktop application for supervised operations with a
supported brokerage desktop client.

## Features

- Inspect account, positions, orders, executions, and funds
- Read brokerage screens through macOS Accessibility and OCR
- Run trade preflight checks and stage orders for explicit confirmation
- Provide an authenticated local MCP/HTTP endpoint for an assistant or main service
- Keep operation history and audit events on the local Mac

The MCP interface does not autonomously confirm, submit, or cancel orders.

## Requirements

- macOS 26 or later
- A supported brokerage desktop client installed on the Mac
- Accessibility permission for Stock Operator
- Screen Recording permission for the OCR helper

## Install

1. Download the Apple Silicon archive from [Releases](../../releases).
2. Verify the downloaded archive with the matching entry in `SHA256SUMS`:

   ```sh
   shasum -a 256 -c SHA256SUMS
   ```

3. Unzip the archive and move `Stock Operator.app` to `/Applications`.
4. Open the app and grant the requested macOS permissions.

The current release may be ad-hoc signed. If macOS blocks the first launch,
use Finder's **Open** command for a build you trust.

## First Use

1. Open **Settings** and select the brokerage application target.
2. Enter the main service URL when one is used.
3. Save the bearer token in **Settings**.
4. Use **Connection** to check the target and server status.

The default local MCP endpoint is:

```text
http://127.0.0.1:5190/mcp
```

Keep the endpoint on loopback unless a private encrypted network and explicit
authentication are configured. Do not expose it to the public internet.

## From Source

Install Rust, Bun, and Nushell, then run:

```sh
nu package.nu
open "target/stock-operator/Stock Operator.app"
```

The bearer token is kept in macOS Keychain. Application settings and history
remain on the local Mac.
