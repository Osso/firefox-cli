mod app;
mod browser;
mod cli;
mod connection;
mod get_handlers;
mod session;
mod tabs_handlers;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    app::run()
}
