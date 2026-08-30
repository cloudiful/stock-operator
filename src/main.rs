#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("stock-operator is only supported on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
mod ax;
#[cfg(target_os = "macos")]
mod cli;
#[cfg(target_os = "macos")]
mod config;
#[cfg(target_os = "macos")]
mod http_api;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod mcp;
#[cfg(target_os = "macos")]
mod operator_service;
#[cfg(target_os = "macos")]
mod pages;

#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    macos::run().await
}
