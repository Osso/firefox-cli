use crate::connection::MarionetteConnection;
use anyhow::{Context, Result, bail};

pub use crate::get_handlers::handle_get;
pub use crate::tabs_handlers::handle_tabs;

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

fn extract_element_id(element: &serde_json::Value) -> Result<&str> {
    element
        .get("value")
        .and_then(|value| value.as_object())
        .and_then(|object| object.values().next())
        .and_then(|value| value.as_str())
        .context("Element not found")
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
