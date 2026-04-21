use crate::cli::TabsCommand;
use crate::connection::MarionetteConnection;
use anyhow::{Context, Result};

fn response_value_str(result: &serde_json::Value) -> &str {
    result
        .get("value")
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

fn window_handles(conn: &mut MarionetteConnection) -> Result<Vec<serde_json::Value>> {
    let result = conn.send("WebDriver:GetWindowHandles", serde_json::json!({}))?;
    let handles = result
        .get("value")
        .and_then(|value| value.as_array())
        .context("Failed to get window handles")?;
    Ok(handles.clone())
}

fn switch_to_window(conn: &mut MarionetteConnection, handle: &str) -> Result<()> {
    conn.send(
        "WebDriver:SwitchToWindow",
        serde_json::json!({ "handle": handle }),
    )?;
    Ok(())
}

fn tab_json(index: usize, title: &str, url: &str, handle: &str) -> serde_json::Value {
    serde_json::json!({
        "index": index,
        "title": title,
        "url": url,
        "handle": handle
    })
}

fn tab_output_line(tab: &serde_json::Value) -> String {
    format!(
        "{}: {} - {}",
        tab.get("index")
            .and_then(|index| index.as_u64())
            .unwrap_or(0),
        tab.get("title")
            .and_then(|title| title.as_str())
            .unwrap_or(""),
        tab.get("url").and_then(|url| url.as_str()).unwrap_or("")
    )
}

fn handle_at_index(handles: &[serde_json::Value], index: usize) -> Option<&str> {
    handles.get(index).and_then(|value| value.as_str())
}

fn current_window_handle(conn: &mut MarionetteConnection) -> Result<Option<String>> {
    let current = conn.send("WebDriver:GetWindowHandle", serde_json::json!({}))?;
    Ok(current
        .get("value")
        .and_then(|value| value.as_str())
        .map(ToString::to_string))
}

fn restore_window_handle(conn: &mut MarionetteConnection, original: Option<String>) -> Result<()> {
    if let Some(handle) = original {
        switch_to_window(conn, &handle)?;
    }
    Ok(())
}

fn switch_to_optional_index(
    conn: &mut MarionetteConnection,
    handles: &[serde_json::Value],
    index: Option<usize>,
) -> Result<()> {
    let Some(index) = index else {
        return Ok(());
    };
    if let Some(handle) = handle_at_index(handles, index) {
        switch_to_window(conn, handle)?;
    }
    Ok(())
}

fn collect_tab_details(
    conn: &mut MarionetteConnection,
    handles: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>> {
    let mut tabs = Vec::new();

    for (index, handle) in handles.iter().enumerate() {
        let Some(handle_str) = handle.as_str() else {
            continue;
        };
        switch_to_window(conn, handle_str)?;
        let title = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
        let url = conn.send("WebDriver:GetCurrentURL", serde_json::json!({}))?;
        tabs.push(tab_json(
            index,
            response_value_str(&title),
            response_value_str(&url),
            handle_str,
        ));
    }

    Ok(tabs)
}

fn print_tabs(tabs: &[serde_json::Value], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(tabs)?);
        return Ok(());
    }

    for tab in tabs {
        println!("{}", tab_output_line(tab));
    }
    Ok(())
}

fn handle_tabs_list(conn: &mut MarionetteConnection, json: bool) -> Result<()> {
    let handles = window_handles(conn)?;
    let original = current_window_handle(conn)?;
    let tabs = collect_tab_details(conn, &handles)?;
    restore_window_handle(conn, original)?;
    print_tabs(&tabs, json)
}

fn handle_tabs_new(conn: &mut MarionetteConnection, url: Option<String>) -> Result<()> {
    let url = url.unwrap_or_else(|| "about:blank".to_string());
    conn.send("WebDriver:NewWindow", serde_json::json!({ "type": "tab" }))?;
    if url != "about:blank" {
        conn.send("WebDriver:Navigate", serde_json::json!({ "url": url }))?;
    }
    println!("✓ New tab created");
    Ok(())
}

fn handle_tabs_close(conn: &mut MarionetteConnection, index: Option<usize>) -> Result<()> {
    let handles = window_handles(conn)?;
    switch_to_optional_index(conn, &handles, index)?;
    conn.send("WebDriver:CloseWindow", serde_json::json!({}))?;
    println!("✓ Tab closed");
    Ok(())
}

fn handle_tabs_switch(conn: &mut MarionetteConnection, index: usize) -> Result<()> {
    let handles = window_handles(conn)?;
    let handle = handle_at_index(&handles, index).context("Tab index out of range")?;
    switch_to_window(conn, handle)?;

    let title = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
    println!(
        "✓ Switched to tab {}: {}",
        index,
        response_value_str(&title)
    );
    Ok(())
}

enum TabsAction {
    List,
    New { url: Option<String> },
    Close { index: Option<usize> },
    Switch { index: usize },
}

fn classify_tabs_command(action: TabsCommand) -> TabsAction {
    match action {
        TabsCommand::List => TabsAction::List,
        TabsCommand::New { url } => TabsAction::New { url },
        TabsCommand::Close { index } => TabsAction::Close { index },
        TabsCommand::Switch { index } => TabsAction::Switch { index },
    }
}

fn execute_tabs_action(
    conn: &mut MarionetteConnection,
    action: TabsAction,
    json: bool,
) -> Result<()> {
    match action {
        TabsAction::List => handle_tabs_list(conn, json),
        TabsAction::New { url } => handle_tabs_new(conn, url),
        TabsAction::Close { index } => handle_tabs_close(conn, index),
        TabsAction::Switch { index } => handle_tabs_switch(conn, index),
    }
}

fn execute_tabs_command(
    conn: &mut MarionetteConnection,
    action: TabsCommand,
    json: bool,
) -> Result<()> {
    let tabs_action = classify_tabs_command(action);
    execute_tabs_action(conn, tabs_action, json)
}

pub fn handle_tabs(action: TabsCommand, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    execute_tabs_command(&mut conn, action, json)
}

#[cfg(test)]
mod tests {
    use crate::cli::TabsCommand;

    use super::{TabsAction, classify_tabs_command, handle_at_index, tab_json, tab_output_line};

    #[test]
    fn tab_output_line_formats_tab_summary() {
        let tab = tab_json(3, "Docs", "https://docs.example", "window-3");
        assert_eq!(tab_output_line(&tab), "3: Docs - https://docs.example");
    }

    #[test]
    fn handle_at_index_returns_string_handles_only() {
        let handles = vec![serde_json::json!("window-a"), serde_json::json!(42)];
        assert_eq!(handle_at_index(&handles, 0), Some("window-a"));
        assert_eq!(handle_at_index(&handles, 1), None);
        assert_eq!(handle_at_index(&handles, 2), None);
    }

    #[test]
    fn classify_tabs_command_maps_switch_index() {
        let action = classify_tabs_command(TabsCommand::Switch { index: 4 });
        match action {
            TabsAction::Switch { index } => assert_eq!(index, 4),
            _ => panic!("expected switch action"),
        }
    }
}
