use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::io::{Read as IoRead, Write as IoWrite};
use std::net::TcpStream;
use std::path::PathBuf;

const DEFAULT_MARIONETTE_PORT: u16 = 2828;

#[derive(Parser)]
#[command(name = "firefox-cli")]
#[command(about = "Firefox automation CLI using Marionette protocol")]
struct Cli {
    /// Marionette port to connect to
    #[arg(long, default_value_t = DEFAULT_MARIONETTE_PORT)]
    port: u16,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Navigate to a URL
    #[command(visible_alias = "goto", visible_alias = "navigate")]
    Open { url: String },

    /// Go back in history
    Back,

    /// Go forward in history
    Forward,

    /// Reload current page
    Reload,

    /// Close browser/tab
    #[command(visible_alias = "quit", visible_alias = "exit")]
    Close,

    /// Click an element
    Click { selector: String },

    /// Type text into an element
    Type { selector: String, text: String },

    /// Clear and fill an element
    Fill { selector: String, text: String },

    /// Press a key
    #[command(visible_alias = "key")]
    Press { key: String },

    /// Take a screenshot (PNG format)
    Screenshot {
        /// Output path
        #[arg(default_value = "/tmp/claude/screenshot.png")]
        path: String,
        /// Full page screenshot
        #[arg(short, long)]
        full: bool,
    },

    /// Evaluate JavaScript
    Eval { script: String },

    /// Get page information
    Get {
        #[command(subcommand)]
        what: GetCommand,
    },

    /// Manage tabs (live via Marionette)
    Tabs {
        #[command(subcommand)]
        action: TabsCommand,
    },

    /// List tabs from Firefox session file (no Marionette needed)
    Session {
        #[command(subcommand)]
        action: SessionCommand,
    },

    /// Wait for element, time, or condition
    Wait {
        /// Selector or milliseconds
        target: Option<String>,
        /// Wait for URL pattern
        #[arg(short, long)]
        url: Option<String>,
    },
}

#[derive(Subcommand)]
enum GetCommand {
    /// Get page title
    Title,
    /// Get current URL
    Url,
    /// Get element text
    Text { selector: Option<String> },
    /// Get element HTML
    Html { selector: String },
    /// Get input value
    Value { selector: String },
    /// Get element attribute
    Attr { selector: String, name: String },
    /// Count matching elements
    Count { selector: String },
}

#[derive(Subcommand)]
enum TabsCommand {
    /// List open tabs
    List,
    /// Open new tab
    New { url: Option<String> },
    /// Close tab
    Close { index: Option<usize> },
    /// Switch to tab by index
    Switch { index: usize },
}

#[derive(Subcommand)]
enum SessionCommand {
    /// List all tabs from session file
    List {
        /// Print URLs only
        #[arg(short = 'u', long)]
        urls_only: bool,
        /// Print titles only
        #[arg(short = 't', long)]
        titles_only: bool,
        /// Search tabs by title or URL
        #[arg(short, long)]
        search: Option<String>,
        /// Limit number of results
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },
    /// Count tabs
    Count,
}

// --- Session File Reading ---

fn find_session_file() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let paths = [
        home.join("snap/firefox/common/.mozilla/firefox"),
        home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
        home.join(".mozilla/firefox"),
    ];

    let mut candidates = Vec::new();
    for base in paths {
        if base.exists() {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir()
                        && !path
                            .file_name()
                            .map_or(true, |n| n.to_string_lossy().starts_with('.'))
                    {
                        let recovery = path.join("sessionstore-backups/recovery.jsonlz4");
                        if recovery.exists() {
                            candidates.push(recovery);
                        }
                    }
                }
            }
        }
    }

    candidates
        .into_iter()
        .max_by_key(|p| p.metadata().ok().and_then(|m| m.modified().ok()))
}

fn load_session(path: &PathBuf) -> Result<serde_json::Value> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;

    let mut compressed = Vec::new();
    file.read_to_end(&mut compressed)?;

    let decompressed = lz4_flex::decompress_size_prepended(&compressed)
        .map_err(|e| anyhow!("LZ4 decompression failed: {}", e))?;

    let session: serde_json::Value = serde_json::from_slice(&decompressed)?;
    Ok(session)
}

#[derive(Serialize)]
struct SessionTab {
    window: usize,
    title: String,
    url: String,
}

fn get_session_tabs(session: &serde_json::Value) -> Vec<SessionTab> {
    let mut tabs = Vec::new();
    if let Some(windows) = session.get("windows").and_then(|w| w.as_array()) {
        for (win_idx, win) in windows.iter().enumerate() {
            if let Some(win_tabs) = win.get("tabs").and_then(|t| t.as_array()) {
                for tab in win_tabs {
                    let index = tab.get("index").and_then(|i| i.as_u64()).unwrap_or(1) as usize;
                    if let Some(entries) = tab.get("entries").and_then(|e| e.as_array()) {
                        if index > 0 && index <= entries.len() {
                            let entry = &entries[index - 1];
                            tabs.push(SessionTab {
                                window: win_idx + 1,
                                title: entry
                                    .get("title")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("No title")
                                    .to_string(),
                                url: entry
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
    tabs
}

// --- Marionette Protocol ---

struct MarionetteConnection {
    stream: TcpStream,
    message_id: u32,
}

impl MarionetteConnection {
    fn connect(port: u16) -> Result<Self> {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).context(
            "Failed to connect to Firefox Marionette. Start Firefox with: firefox --marionette",
        )?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;

        let mut conn = Self {
            stream,
            message_id: 0,
        };

        // Read initial handshake
        conn.read_message()?;

        // Send newSession
        conn.send(
            "WebDriver:NewSession",
            serde_json::json!({
                "capabilities": {}
            }),
        )?;

        Ok(conn)
    }

    fn read_message(&mut self) -> Result<serde_json::Value> {
        let mut len_buf = Vec::new();
        let mut byte = [0u8; 1];

        loop {
            self.stream.read_exact(&mut byte)?;
            if byte[0] == b':' {
                break;
            }
            len_buf.push(byte[0]);
        }

        let len: usize = String::from_utf8(len_buf)?.trim().parse()?;
        let mut msg_buf = vec![0u8; len];
        self.stream.read_exact(&mut msg_buf)?;

        let msg: serde_json::Value = serde_json::from_slice(&msg_buf)?;
        Ok(msg)
    }

    fn send(&mut self, command: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.message_id += 1;

        let msg = serde_json::json!([0, self.message_id, command, params]);
        let msg_str = msg.to_string();
        let packet = format!("{}:{}", msg_str.len(), msg_str);

        self.stream.write_all(packet.as_bytes())?;
        self.stream.flush()?;

        let response = self.read_message()?;

        // Response format: [1, messageId, error, result]
        if let Some(arr) = response.as_array() {
            if arr.len() >= 4 {
                if !arr[2].is_null() {
                    let err = &arr[2];
                    let error_type = err
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown");
                    let message = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
                    bail!("Marionette error ({}): {}", error_type, message);
                }
                return Ok(arr[3].clone());
            }
        }

        Ok(serde_json::json!(null))
    }

    fn execute_script(&mut self, script: &str) -> Result<serde_json::Value> {
        self.send(
            "WebDriver:ExecuteScript",
            serde_json::json!({
                "script": script,
                "args": []
            }),
        )
    }

    fn find_by_css(&mut self, command: &str, selector: &str) -> Result<serde_json::Value> {
        self.send(
            command,
            serde_json::json!({
                "using": "css selector",
                "value": selector
            }),
        )
    }

    fn find_element(&mut self, selector: &str) -> Result<serde_json::Value> {
        self.find_by_css("WebDriver:FindElement", selector)
    }

    fn find_elements(&mut self, selector: &str) -> Result<serde_json::Value> {
        self.find_by_css("WebDriver:FindElements", selector)
    }
}

// --- Subcommand Handlers ---

fn handle_session(action: SessionCommand, json: bool) -> Result<()> {
    let session_file = find_session_file().context("No Firefox session file found")?;
    let session = load_session(&session_file)?;
    let mut tabs = get_session_tabs(&session);

    match action {
        SessionCommand::List {
            urls_only,
            titles_only,
            search,
            limit,
        } => {
            if let Some(ref term) = search {
                let term_lower = term.to_lowercase();
                tabs.retain(|t| {
                    t.title.to_lowercase().contains(&term_lower)
                        || t.url.to_lowercase().contains(&term_lower)
                });
            }

            if let Some(n) = limit {
                tabs.truncate(n);
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&tabs)?);
            } else if urls_only {
                for tab in &tabs {
                    println!("{}", tab.url);
                }
            } else if titles_only {
                for tab in &tabs {
                    println!("{}", tab.title);
                }
            } else {
                for (i, tab) in tabs.iter().enumerate() {
                    let title: String = tab.title.chars().take(70).collect();
                    println!("{:4}. {}", i + 1, title);
                    println!("      {}", tab.url);
                }
            }
        }
        SessionCommand::Count => {
            println!("{}", tabs.len());
        }
    }

    Ok(())
}

fn open_url(url: String, port: u16, json: bool) -> Result<()> {
    let url = if url.contains("://") {
        url
    } else {
        format!("https://{}", url)
    };
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Navigate", serde_json::json!({ "url": url }))?;

    let title = conn.execute_script("return document.title")?;
    let final_url = conn.execute_script("return window.location.href")?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "title": title.get("value").unwrap_or(&title),
                "url": final_url.get("value").unwrap_or(&final_url)
            })
        );
    } else {
        println!(
            "✓ {}",
            title.get("value").and_then(|v| v.as_str()).unwrap_or("")
        );
        println!(
            "  {}",
            final_url
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
        );
    }

    Ok(())
}

fn go_back(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Back", serde_json::json!({}))?;
    println!("✓ Back");
    Ok(())
}

fn go_forward(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Forward", serde_json::json!({}))?;
    println!("✓ Forward");
    Ok(())
}

fn reload_page(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Refresh", serde_json::json!({}))?;
    println!("✓ Reloaded");
    Ok(())
}

fn close_browser(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:CloseWindow", serde_json::json!({}))?;
    println!("✓ Closed");
    Ok(())
}

fn click_element(selector: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let element = conn.find_element(&selector)?;
    let elem_id = element
        .get("value")
        .and_then(|v| v.as_object())
        .and_then(|o| o.values().next())
        .and_then(|v| v.as_str())
        .context("Element not found")?;

    conn.send(
        "WebDriver:ElementClick",
        serde_json::json!({
            "id": elem_id
        }),
    )?;
    println!("✓ Clicked");
    Ok(())
}

fn type_into_element(selector: String, text: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let element = conn.find_element(&selector)?;
    let elem_id = element
        .get("value")
        .and_then(|v| v.as_object())
        .and_then(|o| o.values().next())
        .and_then(|v| v.as_str())
        .context("Element not found")?;

    conn.send(
        "WebDriver:ElementSendKeys",
        serde_json::json!({
            "id": elem_id,
            "text": text
        }),
    )?;
    println!("✓ Typed");
    Ok(())
}

fn fill_element(selector: String, text: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let element = conn.find_element(&selector)?;
    let elem_id = element
        .get("value")
        .and_then(|v| v.as_object())
        .and_then(|o| o.values().next())
        .and_then(|v| v.as_str())
        .context("Element not found")?;

    conn.send(
        "WebDriver:ElementClear",
        serde_json::json!({
            "id": elem_id
        }),
    )?;
    conn.send(
        "WebDriver:ElementSendKeys",
        serde_json::json!({
            "id": elem_id,
            "text": text
        }),
    )?;
    println!("✓ Filled");
    Ok(())
}

fn press_key(key: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    // Use Actions API for key press
    conn.send(
        "WebDriver:PerformActions",
        serde_json::json!({
            "actions": [{
                "type": "key",
                "id": "keyboard",
                "actions": [
                    { "type": "keyDown", "value": key },
                    { "type": "keyUp", "value": key }
                ]
            }]
        }),
    )?;
    println!("✓ Pressed {}", key);
    Ok(())
}

fn take_screenshot(path: String, full: bool, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let result = conn.send(
        "WebDriver:TakeScreenshot",
        serde_json::json!({
            "full": full,
            "hash": false
        }),
    )?;

    let data = result
        .get("value")
        .and_then(|v| v.as_str())
        .context("No screenshot data")?;

    use std::io::Write;
    let bytes = base64_decode(data)?;
    let mut file = std::fs::File::create(&path)?;
    file.write_all(&bytes)?;
    println!("✓ Screenshot saved to {}", path);
    Ok(())
}

fn eval_script(script: String, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let result = conn.execute_script(&format!("return {}", script))?;

    let value = result.get("value").unwrap_or(&result);
    if json {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn handle_get(what: GetCommand, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;

    match what {
        GetCommand::Title => {
            let result = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
            let title = result.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if json {
                println!("{}", serde_json::json!({ "title": title }));
            } else {
                println!("{}", title);
            }
        }
        GetCommand::Url => {
            let result = conn.send("WebDriver:GetCurrentURL", serde_json::json!({}))?;
            let url = result.get("value").and_then(|v| v.as_str()).unwrap_or("");
            if json {
                println!("{}", serde_json::json!({ "url": url }));
            } else {
                println!("{}", url);
            }
        }
        GetCommand::Text { selector } => {
            let text = match selector {
                Some(sel) => {
                    let element = conn.find_element(&sel)?;
                    let elem_id = element
                        .get("value")
                        .and_then(|v| v.as_object())
                        .and_then(|o| o.values().next())
                        .and_then(|v| v.as_str())
                        .context("Element not found")?;
                    let result = conn.send(
                        "WebDriver:GetElementText",
                        serde_json::json!({
                            "id": elem_id
                        }),
                    )?;
                    result
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                }
                None => {
                    let result = conn.execute_script("return document.body.innerText")?;
                    result
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                }
            };
            println!("{}", text);
        }
        GetCommand::Html { selector } => {
            let result = conn.execute_script(&format!(
                "return document.querySelector({}).innerHTML",
                serde_json::to_string(&selector)?
            ))?;
            let html = result.get("value").and_then(|v| v.as_str()).unwrap_or("");
            println!("{}", html);
        }
        GetCommand::Value { selector } => {
            let element = conn.find_element(&selector)?;
            let elem_id = element
                .get("value")
                .and_then(|v| v.as_object())
                .and_then(|o| o.values().next())
                .and_then(|v| v.as_str())
                .context("Element not found")?;
            let result = conn.send(
                "WebDriver:GetElementProperty",
                serde_json::json!({
                    "id": elem_id,
                    "name": "value"
                }),
            )?;
            let value = result.get("value").and_then(|v| v.as_str()).unwrap_or("");
            println!("{}", value);
        }
        GetCommand::Attr { selector, name } => {
            let element = conn.find_element(&selector)?;
            let elem_id = element
                .get("value")
                .and_then(|v| v.as_object())
                .and_then(|o| o.values().next())
                .and_then(|v| v.as_str())
                .context("Element not found")?;
            let result = conn.send(
                "WebDriver:GetElementAttribute",
                serde_json::json!({
                    "id": elem_id,
                    "name": name
                }),
            )?;
            let attr = result.get("value").and_then(|v| v.as_str()).unwrap_or("");
            println!("{}", attr);
        }
        GetCommand::Count { selector } => {
            let result = conn.find_elements(&selector)?;
            let count = result
                .get("value")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            println!("{}", count);
        }
    }

    Ok(())
}

fn handle_tabs(action: TabsCommand, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;

    match action {
        TabsCommand::List => {
            let result = conn.send("WebDriver:GetWindowHandles", serde_json::json!({}))?;
            let handles = result
                .get("value")
                .and_then(|v| v.as_array())
                .context("Failed to get window handles")?;

            let mut tabs = Vec::new();
            let current = conn.send("WebDriver:GetWindowHandle", serde_json::json!({}))?;

            for (i, handle) in handles.iter().enumerate() {
                if let Some(h) = handle.as_str() {
                    conn.send(
                        "WebDriver:SwitchToWindow",
                        serde_json::json!({ "handle": h }),
                    )?;
                    let title = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
                    let url =
                        conn.send("WebDriver:GetCurrentURL", serde_json::json!({}))?;
                    tabs.push(serde_json::json!({
                        "index": i,
                        "title": title.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                        "url": url.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                        "handle": h
                    }));
                }
            }

            // Switch back to original
            if let Some(h) = current.get("value").and_then(|v| v.as_str()) {
                conn.send(
                    "WebDriver:SwitchToWindow",
                    serde_json::json!({ "handle": h }),
                )?;
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&tabs)?);
            } else {
                for tab in &tabs {
                    println!(
                        "{}: {} - {}",
                        tab.get("index").and_then(|i| i.as_u64()).unwrap_or(0),
                        tab.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                        tab.get("url").and_then(|u| u.as_str()).unwrap_or("")
                    );
                }
            }
        }
        TabsCommand::New { url } => {
            let url = url.unwrap_or_else(|| "about:blank".to_string());
            conn.send("WebDriver:NewWindow", serde_json::json!({ "type": "tab" }))?;
            if url != "about:blank" {
                conn.send("WebDriver:Navigate", serde_json::json!({ "url": url }))?;
            }
            println!("✓ New tab created");
        }
        TabsCommand::Close { index } => {
            if let Some(idx) = index {
                let result =
                    conn.send("WebDriver:GetWindowHandles", serde_json::json!({}))?;
                let handles = result
                    .get("value")
                    .and_then(|v| v.as_array())
                    .context("Failed to get window handles")?;

                if let Some(handle) = handles.get(idx).and_then(|h| h.as_str()) {
                    conn.send(
                        "WebDriver:SwitchToWindow",
                        serde_json::json!({ "handle": handle }),
                    )?;
                }
            }
            conn.send("WebDriver:CloseWindow", serde_json::json!({}))?;
            println!("✓ Tab closed");
        }
        TabsCommand::Switch { index } => {
            let result = conn.send("WebDriver:GetWindowHandles", serde_json::json!({}))?;
            let handles = result
                .get("value")
                .and_then(|v| v.as_array())
                .context("Failed to get window handles")?;

            let handle = handles
                .get(index)
                .and_then(|h| h.as_str())
                .context("Tab index out of range")?;

            conn.send(
                "WebDriver:SwitchToWindow",
                serde_json::json!({ "handle": handle }),
            )?;

            let title = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
            println!(
                "✓ Switched to tab {}: {}",
                index,
                title.get("value").and_then(|v| v.as_str()).unwrap_or("")
            );
        }
    }

    Ok(())
}

fn wait_for(target: Option<String>, url: Option<String>, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;

    if let Some(ms) = target.as_ref().and_then(|s| s.parse::<u64>().ok()) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        println!("✓ Waited {}ms", ms);
    } else if let Some(selector) = target {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);

        loop {
            if start.elapsed() > timeout {
                bail!("Timeout waiting for element: {}", selector);
            }

            if conn.find_element(&selector).is_ok() {
                println!("✓ Element found");
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    } else if let Some(url_pattern) = url {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30);

        loop {
            if start.elapsed() > timeout {
                bail!("Timeout waiting for URL: {}", url_pattern);
            }

            let result = conn.send("WebDriver:GetCurrentURL", serde_json::json!({}))?;
            let current_url = result.get("value").and_then(|v| v.as_str()).unwrap_or("");

            if current_url.contains(&url_pattern) {
                println!("✓ URL matched");
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    Ok(())
}

// --- Main ---

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

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

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    // Handle data URL prefix if present
    let data = if let Some(pos) = input.find(',') {
        &input[pos + 1..]
    } else {
        input
    };

    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .context("Failed to decode base64")
}
