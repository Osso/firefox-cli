#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg_attr(coverage_nightly, coverage(off))]
mod app;
#[cfg_attr(coverage_nightly, coverage(off))]
mod browser;
mod cli;
#[cfg_attr(coverage_nightly, coverage(off))]
mod connection;
mod get_handlers;
mod session;
mod tabs_handlers;

use anyhow::Result;

#[tokio::main]
#[cfg_attr(coverage_nightly, coverage(off))]
async fn main() -> Result<()> {
    app::run()
}
