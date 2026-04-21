use anyhow::{Context, Result, bail};
use std::io::{Read as IoRead, Write as IoWrite};
use std::net::TcpStream;

pub struct MarionetteConnection {
    stream: TcpStream,
    message_id: u32,
}

impl MarionetteConnection {
    pub fn connect(port: u16) -> Result<Self> {
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

    pub fn send(&mut self, command: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.message_id += 1;

        let msg = serde_json::json!([0, self.message_id, command, params]);
        let msg_str = msg.to_string();
        let packet = format!("{}:{}", msg_str.len(), msg_str);

        self.stream.write_all(packet.as_bytes())?;
        self.stream.flush()?;

        let response = self.read_message()?;

        // Response format: [1, messageId, error, result]
        if let Some(arr) = response.as_array()
            && arr.len() >= 4
        {
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

        Ok(serde_json::json!(null))
    }

    pub fn execute_script(&mut self, script: &str) -> Result<serde_json::Value> {
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

    pub fn find_element(&mut self, selector: &str) -> Result<serde_json::Value> {
        self.find_by_css("WebDriver:FindElement", selector)
    }

    pub fn find_elements(&mut self, selector: &str) -> Result<serde_json::Value> {
        self.find_by_css("WebDriver:FindElements", selector)
    }
}
