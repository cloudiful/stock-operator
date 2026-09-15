mod app;
mod backend;
mod cli;
mod config;
mod desktop;
mod http_api;
mod live_ops;
mod mcp;
mod operator_service;
mod operator_types;
mod pages;
mod storage;
mod win_backend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run().await
}
