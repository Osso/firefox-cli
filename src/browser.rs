use crate::cli::{GetCommand, TabsCommand};
use crate::connection::MarionetteConnection;
use anyhow::{Context, Result, bail};

pub fn open_url(url: String, port: u16, json: bool) -> Result<()> {
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
            title
                .get("value")
                .and_then(|value| value.as_str())
                .unwrap_or("")
        );
        println!(
            "  {}",
            final_url
                .get("value")
                .and_then(|value| value.as_str())
                .unwrap_or("")
        );
    }

    Ok(())
}

pub fn go_back(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Back", serde_json::json!({}))?;
    println!("✓ Back");
    Ok(())
}

pub fn go_forward(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Forward", serde_json::json!({}))?;
    println!("✓ Forward");
    Ok(())
}

pub fn reload_page(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:Refresh", serde_json::json!({}))?;
    println!("✓ Reloaded");
    Ok(())
}

pub fn close_browser(port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    conn.send("WebDriver:CloseWindow", serde_json::json!({}))?;
    println!("✓ Closed");
    Ok(())
}

pub fn click_element(selector: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let element = conn.find_element(&selector)?;
    let element_id = extract_element_id(&element)?;

    conn.send(
        "WebDriver:ElementClick",
        serde_json::json!({
            "id": element_id
        }),
    )?;
    println!("✓ Clicked");
    Ok(())
}

pub fn type_into_element(selector: String, text: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let element = conn.find_element(&selector)?;
    let element_id = extract_element_id(&element)?;

    conn.send(
        "WebDriver:ElementSendKeys",
        serde_json::json!({
            "id": element_id,
            "text": text
        }),
    )?;
    println!("✓ Typed");
    Ok(())
}

pub fn fill_element(selector: String, text: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    let element = conn.find_element(&selector)?;
    let element_id = extract_element_id(&element)?;

    conn.send(
        "WebDriver:ElementClear",
        serde_json::json!({
            "id": element_id
        }),
    )?;
    conn.send(
        "WebDriver:ElementSendKeys",
        serde_json::json!({
            "id": element_id,
            "text": text
        }),
    )?;
    println!("✓ Filled");
    Ok(())
}

pub fn press_key(key: String, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
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

pub fn take_screenshot(path: String, full: bool, port: u16) -> Result<()> {
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
        .and_then(|value| value.as_str())
        .context("No screenshot data")?;

    use std::io::Write;
    let bytes = base64_decode(data)?;
    let mut file = std::fs::File::create(&path)?;
    file.write_all(&bytes)?;
    println!("✓ Screenshot saved to {}", path);
    Ok(())
}

pub fn eval_script(script: String, port: u16, json: bool) -> Result<()> {
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

fn response_value_str(result: &serde_json::Value) -> &str {
    result
        .get("value")
        .and_then(|value| value.as_str())
        .unwrap_or("")
}

fn named_get_output(key: &str, value: &str, json: bool) -> String {
    if json {
        serde_json::json!({ key: value }).to_string()
    } else {
        value.to_string()
    }
}

fn print_named_get_value(key: &str, value: &str, json: bool) {
    println!("{}", named_get_output(key, value, json));
}

fn extract_element_id(element: &serde_json::Value) -> Result<&str> {
    element
        .get("value")
        .and_then(|value| value.as_object())
        .and_then(|object| object.values().next())
        .and_then(|value| value.as_str())
        .context("Element not found")
}

fn get_element_named_value(
    conn: &mut MarionetteConnection,
    selector: &str,
    command: &str,
    name: &str,
) -> Result<String> {
    let element = conn.find_element(selector)?;
    let element_id = extract_element_id(&element)?;
    let result = conn.send(
        command,
        serde_json::json!({
            "id": element_id,
            "name": name
        }),
    )?;
    Ok(response_value_str(&result).to_string())
}

fn get_text_by_selector(conn: &mut MarionetteConnection, selector: &str) -> Result<String> {
    let element = conn.find_element(selector)?;
    let element_id = extract_element_id(&element)?;
    let result = conn.send(
        "WebDriver:GetElementText",
        serde_json::json!({
            "id": element_id
        }),
    )?;
    Ok(response_value_str(&result).to_string())
}

fn get_body_text(conn: &mut MarionetteConnection) -> Result<String> {
    let result = conn.execute_script("return document.body.innerText")?;
    Ok(response_value_str(&result).to_string())
}

fn get_text(conn: &mut MarionetteConnection, selector: Option<String>) -> Result<String> {
    match selector {
        Some(selector) => get_text_by_selector(conn, &selector),
        None => get_body_text(conn),
    }
}

fn get_inner_html(conn: &mut MarionetteConnection, selector: &str) -> Result<String> {
    let script = format!(
        "return document.querySelector({}).innerHTML",
        serde_json::to_string(selector)?
    );
    let result = conn.execute_script(&script)?;
    Ok(response_value_str(&result).to_string())
}

fn get_matching_elements_count(conn: &mut MarionetteConnection, selector: &str) -> Result<usize> {
    let result = conn.find_elements(selector)?;
    Ok(result
        .get("value")
        .and_then(|value| value.as_array())
        .map(|elements| elements.len())
        .unwrap_or(0))
}

fn execute_get_title(conn: &mut MarionetteConnection, json: bool) -> Result<()> {
    let result = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
    print_named_get_value("title", response_value_str(&result), json);
    Ok(())
}

fn execute_get_url(conn: &mut MarionetteConnection, json: bool) -> Result<()> {
    let result = conn.send("WebDriver:GetCurrentURL", serde_json::json!({}))?;
    print_named_get_value("url", response_value_str(&result), json);
    Ok(())
}

fn execute_get_text(conn: &mut MarionetteConnection, selector: Option<String>) -> Result<()> {
    let text = get_text(conn, selector)?;
    println!("{}", text);
    Ok(())
}

fn execute_get_html(conn: &mut MarionetteConnection, selector: &str) -> Result<()> {
    let html = get_inner_html(conn, selector)?;
    println!("{}", html);
    Ok(())
}

fn execute_get_value(conn: &mut MarionetteConnection, selector: &str) -> Result<()> {
    let value = get_element_named_value(conn, selector, "WebDriver:GetElementProperty", "value")?;
    println!("{}", value);
    Ok(())
}

fn execute_get_attr(conn: &mut MarionetteConnection, selector: &str, name: &str) -> Result<()> {
    let attr = get_element_named_value(conn, selector, "WebDriver:GetElementAttribute", name)?;
    println!("{}", attr);
    Ok(())
}

fn execute_get_count(conn: &mut MarionetteConnection, selector: &str) -> Result<()> {
    let count = get_matching_elements_count(conn, selector)?;
    println!("{}", count);
    Ok(())
}

fn execute_get_command(
    conn: &mut MarionetteConnection,
    what: GetCommand,
    json: bool,
) -> Result<()> {
    match what {
        GetCommand::Title => execute_get_title(conn, json)?,
        GetCommand::Url => execute_get_url(conn, json)?,
        GetCommand::Text { selector } => execute_get_text(conn, selector)?,
        GetCommand::Html { selector } => execute_get_html(conn, &selector)?,
        GetCommand::Value { selector } => execute_get_value(conn, &selector)?,
        GetCommand::Attr { selector, name } => execute_get_attr(conn, &selector, &name)?,
        GetCommand::Count { selector } => execute_get_count(conn, &selector)?,
    }

    Ok(())
}

pub fn handle_get(what: GetCommand, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    execute_get_command(&mut conn, what, json)
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

fn current_window_handle(conn: &mut MarionetteConnection) -> Result<Option<String>> {
    let current = conn.send("WebDriver:GetWindowHandle", serde_json::json!({}))?;
    Ok(current
        .get("value")
        .and_then(|value| value.as_str())
        .map(ToString::to_string))
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

    if let Some(handle) = original {
        switch_to_window(conn, &handle)?;
    }

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
    if let Some(index) = index {
        let handles = window_handles(conn)?;
        if let Some(handle) = handles.get(index).and_then(|value| value.as_str()) {
            switch_to_window(conn, handle)?;
        }
    }

    conn.send("WebDriver:CloseWindow", serde_json::json!({}))?;
    println!("✓ Tab closed");
    Ok(())
}

fn handle_tabs_switch(conn: &mut MarionetteConnection, index: usize) -> Result<()> {
    let handles = window_handles(conn)?;
    let handle = handles
        .get(index)
        .and_then(|value| value.as_str())
        .context("Tab index out of range")?;
    switch_to_window(conn, handle)?;

    let title = conn.send("WebDriver:GetTitle", serde_json::json!({}))?;
    println!(
        "✓ Switched to tab {}: {}",
        index,
        response_value_str(&title)
    );
    Ok(())
}

fn execute_tabs_command(
    conn: &mut MarionetteConnection,
    action: TabsCommand,
    json: bool,
) -> Result<()> {
    match action {
        TabsCommand::List => handle_tabs_list(conn, json),
        TabsCommand::New { url } => handle_tabs_new(conn, url),
        TabsCommand::Close { index } => handle_tabs_close(conn, index),
        TabsCommand::Switch { index } => handle_tabs_switch(conn, index),
    }
}

pub fn handle_tabs(action: TabsCommand, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    execute_tabs_command(&mut conn, action, json)
}

pub fn wait_for(target: Option<String>, url: Option<String>, port: u16) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;

    if let Some(ms) = target.as_ref().and_then(|value| value.parse::<u64>().ok()) {
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
            let current_url = result
                .get("value")
                .and_then(|value| value.as_str())
                .unwrap_or("");

            if current_url.contains(&url_pattern) {
                println!("✓ URL matched");
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    Ok(())
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let data = if let Some(position) = input.find(',') {
        &input[position + 1..]
    } else {
        input
    };

    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .context("Failed to decode base64")
}

#[cfg(test)]
mod tests {
    use super::{
        extract_element_id, named_get_output, response_value_str, tab_json, tab_output_line,
    };

    #[test]
    fn extract_element_id_reads_webdriver_value() {
        let element = serde_json::json!({
            "value": {
                "element-6066-11e4-a52e-4f735466cecf": "node-1"
            }
        });
        let element_id = extract_element_id(&element).expect("element id should parse");
        assert_eq!(element_id, "node-1");
    }

    #[test]
    fn response_value_str_returns_empty_when_not_string() {
        let scalar = serde_json::json!({ "value": 42 });
        let missing = serde_json::json!({});
        assert_eq!(response_value_str(&scalar), "");
        assert_eq!(response_value_str(&missing), "");
    }

    #[test]
    fn named_get_output_formats_json_and_plain_modes() {
        assert_eq!(named_get_output("title", "Example", false), "Example");
        assert_eq!(
            named_get_output("title", "Example", true),
            r#"{"title":"Example"}"#
        );
    }

    #[test]
    fn tab_output_line_formats_tab_summary() {
        let tab = tab_json(3, "Docs", "https://docs.example", "window-3");
        assert_eq!(tab_output_line(&tab), "3: Docs - https://docs.example");
    }
}
