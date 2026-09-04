# Stock Operator

[English](README.md)

Stock Operator 是一个 macOS 桌面应用，用于在受监督的情况下操作受支持的券商桌面客户端。

## 功能

- 查看账户、持仓、委托、成交和资金信息
- 通过 macOS 辅助功能和 OCR 读取券商界面
- 执行交易前置检查，并将订单暂存到明确确认流程
- 提供经过认证的本地 MCP/HTTP 接口，供助手或主服务使用
- 在本机保存操作历史和审计事件

MCP 接口不会自动确认、提交或撤销订单。

## 使用要求

- macOS 26 或更高版本
- Mac 上已安装受支持的券商桌面客户端
- 为 Stock Operator 授予辅助功能权限
- 为 OCR helper 授予屏幕录制权限

## 安装

1. 从 [Releases](../../releases) 下载适用于 Apple Silicon 的 DMG。
2. 使用 `SHA256SUMS` 校验 DMG：

   ```sh
   shasum -a 256 -c SHA256SUMS
   ```

3. 打开 DMG，将 `Stock Operator.app` 拖到 `/Applications`。
4. 从 `/Applications` 启动应用，并按提示授予 macOS 权限。

当前 release 可能使用 ad-hoc 签名。如果 macOS 阻止首次启动，请在 Finder
中对可信的构建使用“打开”命令。

## 首次使用

1. 打开“Settings”，选择券商应用目标。
2. 如使用主服务，填写主服务 URL。
3. 在“Settings”中保存 Bearer Token。
4. 在“Connection”中检查目标和服务器状态。

默认本地 MCP 地址为：

```text
http://127.0.0.1:5190/mcp
```

除非已配置私有加密网络和明确的身份认证，否则请保持使用本机地址，
不要将接口暴露到公网。

## 从源码构建

安装 Rust 和 Bun 后运行：

```sh
sh package.sh
open "target/stock-operator/Stock Operator.app"
```

Bearer Token 保存在 macOS 钥匙串中，应用设置和历史记录保存在本机。
