use clap::{Parser, Subcommand};

pub const DEFAULT_MARIONETTE_PORT: u16 = 2828;

#[derive(Parser)]
#[command(name = "firefox-cli")]
#[command(about = "Firefox automation CLI using Marionette protocol")]
pub struct Cli {
    /// Marionette port to connect to
    #[arg(long, default_value_t = DEFAULT_MARIONETTE_PORT)]
    pub port: u16,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
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
pub enum GetCommand {
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
pub enum TabsCommand {
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
pub enum SessionCommand {
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
