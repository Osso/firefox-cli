mod app;
mod browser;
mod cli;
mod connection;
mod get_handlers;
mod session;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    app::run()
}
