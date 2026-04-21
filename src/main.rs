mod browser;
mod cli;
mod connection;
mod session;

use anyhow::Result;
use clap::Parser;

use browser::{
    click_element, close_browser, eval_script, fill_element, go_back, go_forward, handle_get,
    handle_tabs, open_url, press_key, reload_page, take_screenshot, type_into_element, wait_for,
};
use cli::{Cli, Command};
use session::handle_session;

fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Session { action } => handle_session(action, cli.json)?,
        Command::Open { url } => open_url(url, cli.port, cli.json)?,
        Command::Back => go_back(cli.port)?,
        Command::Forward => go_forward(cli.port)?,
        Command::Reload => reload_page(cli.port)?,
        Command::Close => close_browser(cli.port)?,
        Command::Click { selector } => click_element(selector, cli.port)?,
        Command::Type { selector, text } => type_into_element(selector, text, cli.port)?,
        Command::Fill { selector, text } => fill_element(selector, text, cli.port)?,
        Command::Press { key } => press_key(key, cli.port)?,
        Command::Screenshot { path, full } => take_screenshot(path, full, cli.port)?,
        Command::Eval { script } => eval_script(script, cli.port, cli.json)?,
        Command::Get { what } => handle_get(what, cli.port, cli.json)?,
        Command::Tabs { action } => handle_tabs(action, cli.port, cli.json)?,
        Command::Wait { target, url } => wait_for(target, url, cli.port)?,
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run_cli(cli)
}
