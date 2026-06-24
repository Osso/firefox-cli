use anyhow::Result;
use clap::Parser;

use crate::browser::{
    click_element, close_browser, eval_script, fill_element, go_back, go_forward, handle_get,
    handle_tabs, open_url, press_key, reload_page, take_screenshot, type_into_element, wait_for,
};
use crate::cli::{Cli, Command};
use crate::session::handle_session;

fn run_cli(cli: Cli) -> Result<()> {
    let Cli {
        command,
        port,
        json,
    } = cli;
    match command {
        Command::Session { action } => handle_session(action, json),
        other => run_browser_command(other, port, json),
    }
}

fn run_browser_command(command: Command, port: u16, json: bool) -> Result<()> {
    match command {
        Command::Open { url } => open_url(url, port, json),
        Command::Back => go_back(port),
        Command::Forward => go_forward(port),
        Command::Reload => reload_page(port),
        Command::Close => close_browser(port),
        Command::Click { selector } => click_element(selector, port),
        Command::Type { selector, text } => type_into_element(selector, text, port),
        Command::Fill { selector, text } => fill_element(selector, text, port),
        Command::Press { key } => press_key(key, port),
        Command::Screenshot { path, full } => take_screenshot(path, full, port),
        Command::Eval { script } => eval_script(script, port, json),
        Command::Get { what } => handle_get(what, port, json),
        Command::Tabs { action } => handle_tabs(action, port, json),
        Command::Wait { target, url } => wait_for(target, url, port),
        Command::Session { .. } => unreachable!(),
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    run_cli(cli)
}
