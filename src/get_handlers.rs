use crate::cli::GetCommand;
use crate::connection::MarionetteConnection;
use anyhow::{Context, Result};

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

fn execute_get_string_field(
    conn: &mut MarionetteConnection,
    webdriver_command: &str,
    output_key: &str,
    json: bool,
) -> Result<()> {
    let result = conn.send(webdriver_command, serde_json::json!({}))?;
    print_named_get_value(output_key, response_value_str(&result), json);
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

struct NamedGetAction {
    webdriver_command: &'static str,
    output_key: &'static str,
}

enum SelectorGetAction {
    Html(String),
    Value(String),
    Attr { selector: String, name: String },
    Count(String),
}

enum GetAction {
    Named(NamedGetAction),
    Text(Option<String>),
    Selector(SelectorGetAction),
}

fn classify_get_command(what: GetCommand) -> GetAction {
    match what {
        GetCommand::Title => GetAction::Named(NamedGetAction {
            webdriver_command: "WebDriver:GetTitle",
            output_key: "title",
        }),
        GetCommand::Url => GetAction::Named(NamedGetAction {
            webdriver_command: "WebDriver:GetCurrentURL",
            output_key: "url",
        }),
        GetCommand::Text { selector } => GetAction::Text(selector),
        GetCommand::Html { selector } => GetAction::Selector(SelectorGetAction::Html(selector)),
        GetCommand::Value { selector } => GetAction::Selector(SelectorGetAction::Value(selector)),
        GetCommand::Attr { selector, name } => {
            GetAction::Selector(SelectorGetAction::Attr { selector, name })
        }
        GetCommand::Count { selector } => GetAction::Selector(SelectorGetAction::Count(selector)),
    }
}

fn execute_selector_get_action(
    conn: &mut MarionetteConnection,
    action: SelectorGetAction,
) -> Result<()> {
    match action {
        SelectorGetAction::Html(selector) => execute_get_html(conn, &selector),
        SelectorGetAction::Value(selector) => execute_get_value(conn, &selector),
        SelectorGetAction::Attr { selector, name } => execute_get_attr(conn, &selector, &name),
        SelectorGetAction::Count(selector) => execute_get_count(conn, &selector),
    }
}

fn execute_get_action(
    conn: &mut MarionetteConnection,
    action: GetAction,
    json: bool,
) -> Result<()> {
    match action {
        GetAction::Named(action) => {
            execute_get_string_field(conn, action.webdriver_command, action.output_key, json)
        }
        GetAction::Text(selector) => execute_get_text(conn, selector),
        GetAction::Selector(action) => execute_selector_get_action(conn, action),
    }
}

fn execute_get_command(
    conn: &mut MarionetteConnection,
    what: GetCommand,
    json: bool,
) -> Result<()> {
    let action = classify_get_command(what);
    execute_get_action(conn, action, json)
}

pub fn handle_get(what: GetCommand, port: u16, json: bool) -> Result<()> {
    let mut conn = MarionetteConnection::connect(port)?;
    execute_get_command(&mut conn, what, json)
}

#[cfg(test)]
mod tests {
    use crate::cli::GetCommand;

    use super::{
        GetAction, NamedGetAction, classify_get_command, extract_element_id, named_get_output,
        response_value_str,
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
    fn classify_get_command_maps_title_to_named_action() {
        let action = classify_get_command(GetCommand::Title);
        match action {
            GetAction::Named(NamedGetAction {
                webdriver_command,
                output_key,
            }) => {
                assert_eq!(webdriver_command, "WebDriver:GetTitle");
                assert_eq!(output_key, "title");
            }
            _ => panic!("expected named action"),
        }
    }
}
