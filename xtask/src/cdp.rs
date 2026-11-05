//! A tiny Chrome DevTools Protocol client for the browser smoke test, without any
//! dependency: it speaks HTTP (to find the page's WebSocket) and just enough WebSocket
//! (one page, text frames) to navigate and evaluate a promise.
//!
//! The alternative — `chromium --dump-dom` — dumps the page before asynchronous work
//! (fetching the wasm module, loading an AudioWorklet) has finished, so it cannot check
//! that the editor really started.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A running headless browser with one page.
pub(crate) struct Browser {
    child: Child,
    port: u16,
    page: Page,
}

/// The page's WebSocket and the next request id.
struct Page {
    socket: TcpStream,
    next_id: u64,
    /// Console messages and exceptions, for a failure report.
    events: Vec<String>,
}

impl Browser {
    /// Starts a headless browser on `port` and connects to its first page.
    pub(crate) fn start(program: &std::ffi::OsString, port: u16) -> Result<Self, String> {
        let child = Command::new(program)
            .args([
                "--headless=new",
                "--no-sandbox",
                "--disable-dev-shm-usage",
                "--enable-unsafe-swiftshader",
                "--use-angle=swiftshader",
                "--autoplay-policy=no-user-gesture-required",
                "--window-size=1280,900",
                &format!("--remote-debugging-port={port}"),
                "about:blank",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start the browser: {e}"))?;
        let mut page = wait_for_page(port, Duration::from_secs(20))
            .map_err(|e| format!("the browser did not open its debug port: {e}"))?;
        let _ = page.call("Runtime.enable", "{}");
        let _ = page.call("Log.enable", "{}");
        let _ = page.call("Page.enable", "{}");
        Ok(Browser { child, port, page })
    }

    /// Opens `url` and evaluates `expression` until its promise resolves, returning the
    /// promise's string value.
    pub(crate) fn evaluate(&mut self, url: &str, expression: &str) -> Result<String, String> {
        self.page.navigate(self.port, url)?;
        self.page.evaluate(expression)
    }

    /// Evaluates `expression` in the current page (no navigation).
    pub(crate) fn eval(&mut self, expression: &str) -> Result<String, String> {
        self.page.evaluate(expression)
    }

    /// The browser's console messages and exceptions, for a failure report.
    pub(crate) fn events(&self) -> &[String] {
        &self.page.events
    }

    /// Saves a PNG of the page.
    pub(crate) fn screenshot(&mut self, path: &std::path::Path) -> Result<(), String> {
        let response = self
            .page
            .call("Page.captureScreenshot", "{\"format\":\"png\"}")?;
        let data = json_string_field(&response, "\"data\":")
            .ok_or_else(|| "the browser sent no screenshot".to_owned())?;
        let bytes =
            base64_decode(&data).ok_or_else(|| "the screenshot is not base64".to_owned())?;
        std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Page {
    fn navigate(&mut self, port: u16, url: &str) -> Result<(), String> {
        let params = format!("{{\"url\":\"{}\"}}", escape_json(url));
        let response = self.call("Page.navigate", &params)?;
        if response.contains("\"error\":") {
            return Err(format!("the browser could not navigate: {response}"));
        }
        // Page.navigate returns when the navigation starts. The new document needs a
        // moment before `Runtime.evaluate` runs in it; the expression's promise then waits
        // for the app's own marker.
        let _ = port;
        std::thread::sleep(Duration::from_millis(500));
        Ok(())
    }

    fn evaluate(&mut self, expression: &str) -> Result<String, String> {
        let params = format!(
            "{{\"expression\":\"{}\",\"awaitPromise\":true,\"returnByValue\":true}}",
            escape_json(expression)
        );
        let response = self.call("Runtime.evaluate", &params)?;
        if let Some(error) = json_string_field(&response, "\"exceptionDetails\":") {
            let _ = error;
        }
        if response.contains("\"exceptionDetails\"") {
            return Err(format!("the page threw: {}", first_string(&response)));
        }
        json_string_field(&response, "\"value\":")
            .ok_or_else(|| format!("the page returned no value: {response}"))
    }

    /// Sends one CDP command and waits for its response, skipping events.
    fn call(&mut self, method: &str, params: &str) -> Result<String, String> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let message = format!("{{\"id\":{id},\"method\":\"{method}\",\"params\":{params}}}");
        self.send_text(&message)?;
        let expected = format!("\"id\":{id}");
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            let text = self.receive_text()?;
            if text.contains(&expected) {
                return Ok(text);
            }
            if text.contains("\"method\":\"Log.entryAdded\"")
                || text.contains("\"method\":\"Runtime.consoleAPICalled\"")
                || text.contains("\"method\":\"Runtime.exceptionThrown\"")
            {
                self.events.push(text.chars().take(2000).collect());
            }
        }
        Err(format!("the browser did not answer {method}"))
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        let payload = text.as_bytes();
        let mask = [0x11_u8, 0x22, 0x33, 0x44];
        let mut frame = Vec::with_capacity(payload.len().saturating_add(14));
        frame.push(0x81);
        let length = payload.len();
        if length < 126 {
            frame.push(0x80 | u8::try_from(length).unwrap_or(126));
        } else if length <= 0xFFFF {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&u16::try_from(length).unwrap_or(0).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        for (index, byte) in payload.iter().enumerate() {
            let key = mask.get(index % 4).copied().unwrap_or(0);
            frame.push(byte ^ key);
        }
        self.socket
            .write_all(&frame)
            .map_err(|e| format!("cannot write to the browser: {e}"))
    }

    fn receive_text(&mut self) -> Result<String, String> {
        loop {
            let (opcode, payload) = self.receive_frame()?;
            match opcode {
                0x1 => {
                    return String::from_utf8(payload)
                        .map_err(|e| format!("the browser sent invalid text: {e}"));
                }
                0x8 => return Err("the browser closed the debug connection".to_owned()),
                0x9 => {
                    // Ping: answer with a pong.
                    let mut frame = vec![0x8A, 0x80];
                    frame.extend_from_slice(&[0, 0, 0, 0]);
                    self.socket
                        .write_all(&frame)
                        .map_err(|e| format!("cannot answer the browser: {e}"))?;
                }
                _ => {}
            }
        }
    }

    fn receive_frame(&mut self) -> Result<(u8, Vec<u8>), String> {
        let mut header = [0_u8; 2];
        self.socket
            .read_exact(&mut header)
            .map_err(|e| format!("cannot read from the browser: {e}"))?;
        let opcode = header[0] & 0x0F;
        let masked = header[1] & 0x80 != 0;
        let mut length = u64::from(header[1] & 0x7F);
        if length == 126 {
            let mut extended = [0_u8; 2];
            self.socket
                .read_exact(&mut extended)
                .map_err(|e| format!("cannot read from the browser: {e}"))?;
            length = u64::from(u16::from_be_bytes(extended));
        } else if length == 127 {
            let mut extended = [0_u8; 8];
            self.socket
                .read_exact(&mut extended)
                .map_err(|e| format!("cannot read from the browser: {e}"))?;
            length = u64::from_be_bytes(extended);
        }
        let mask = if masked {
            let mut key = [0_u8; 4];
            self.socket
                .read_exact(&mut key)
                .map_err(|e| format!("cannot read from the browser: {e}"))?;
            Some(key)
        } else {
            None
        };
        let length =
            usize::try_from(length).map_err(|_| "the browser sent a huge frame".to_owned())?;
        if length > 64 * 1024 * 1024 {
            return Err("the browser sent a huge frame".to_owned());
        }
        let mut payload = vec![0_u8; length];
        self.socket
            .read_exact(&mut payload)
            .map_err(|e| format!("cannot read from the browser: {e}"))?;
        if let Some(mask) = mask {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask.get(index % 4).copied().unwrap_or(0);
            }
        }
        Ok((opcode, payload))
    }
}

/// Finds the first page's WebSocket endpoint through the browser's HTTP API.
fn wait_for_page(port: u16, timeout: Duration) -> Result<Page, String> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < timeout {
        match http_get(port, "/json/list") {
            Ok(list) => {
                if let Some(url) = page_socket(&list) {
                    return connect_websocket(port, &url);
                }
                last = list;
            }
            Err(error) => last = error,
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(last)
}

fn connect_websocket(port: u16, url: &str) -> Result<Page, String> {
    let path = url
        .strip_prefix("ws://")
        .and_then(|rest| rest.find('/').map(|at| rest.get(at..).unwrap_or("/")))
        .unwrap_or("/");
    let mut socket = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("cannot connect to the browser: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| format!("cannot set a browser timeout: {e}"))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    socket
        .write_all(request.as_bytes())
        .map_err(|e| format!("cannot ask the browser to upgrade: {e}"))?;
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while !response.ends_with(b"\r\n\r\n") && response.len() < 8192 {
        socket
            .read_exact(&mut byte)
            .map_err(|e| format!("cannot read the browser's handshake: {e}"))?;
        response.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&response);
    if !head.starts_with("HTTP/1.1 101") {
        return Err(format!("the browser refused the debug connection: {head}"));
    }
    Ok(Page {
        socket,
        next_id: 1,
        events: Vec::new(),
    })
}

fn http_get(port: u16, path: &str) -> Result<String, String> {
    let mut socket = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("cannot connect to the browser: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("cannot set a browser timeout: {e}"))?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    socket
        .write_all(request.as_bytes())
        .map_err(|e| format!("cannot ask the browser: {e}"))?;
    // Read until the browser closes; some versions keep the socket open until the read
    // timeout, so a timeout with bytes in hand is still a complete answer.
    let mut response = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match socket.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => response.extend_from_slice(chunk.get(..read).unwrap_or_default()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && !response.is_empty() =>
            {
                break;
            }
            Err(error) => return Err(format!("cannot read the browser's answer: {error}")),
        }
        if response.len() > 1024 * 1024 {
            break;
        }
    }
    let response = String::from_utf8_lossy(&response);
    let body = response
        .split_once("\r\n\r\n")
        .map_or(response.as_ref(), |(_, body)| body);
    Ok(body.to_owned())
}

/// The WebSocket URL of the first real page (not an extension's background page).
fn page_socket(list: &str) -> Option<String> {
    let mut from = 0;
    while from < list.len() {
        let Some(found) = list.get(from..).and_then(|rest| rest.find("\"type\"")) else {
            break;
        };
        let Some(at) = from.checked_add(found) else {
            break;
        };
        let Some(after) = list.get(at..) else {
            break;
        };
        let is_page =
            after.starts_with("\"type\": \"page\"") || after.starts_with("\"type\":\"page\"");
        if is_page && let Some(url) = json_string_field(after, "\"webSocketDebuggerUrl\":") {
            return Some(url);
        }
        let Some(next) = at.checked_add(6) else {
            break;
        };
        if next <= from {
            break;
        }
        from = next;
    }
    json_string_field(list, "\"webSocketDebuggerUrl\":")
}

/// The string value after `key` (`"key":"value"`), with the escapes we send (quotes and
/// backslashes) decoded. Enough for CDP values; not a general JSON parser.
fn json_string_field(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)?.checked_add(key.len())?;
    let rest = text.get(start..)?;
    // JSON may put spaces after the colon (`"value": "ok"`).
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                other => value.push(other),
            },
            c => value.push(c),
        }
    }
    None
}

fn first_string(text: &str) -> String {
    json_string_field(text, "\"description\":")
        .or_else(|| json_string_field(text, "\"text\":"))
        .unwrap_or_else(|| text.chars().take(200).collect())
}

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Decodes base64 (screenshots); `None` for invalid input.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len().saturating_mul(3).saturating_div(4));
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | value;
        bits = bits.saturating_add(6);
        if bits >= 8 {
            bits = bits.saturating_sub(8);
            let shift = bits;
            out.push(u8::try_from((buffer >> shift) & 0xFF).unwrap_or(0));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_socket_skips_background_and_ui_targets() {
        let list = r#"[
          { "type": "background_page", "url": "chrome-extension://x",
            "webSocketDebuggerUrl": "ws://127.0.0.1:1/devtools/page/BG" },
          { "type": "page", "url": "about:blank",
            "webSocketDebuggerUrl": "ws://127.0.0.1:1/devtools/page/PAGE" },
          { "type": "browser_ui", "url": "chrome://omnibox-popup",
            "webSocketDebuggerUrl": "ws://127.0.0.1:1/devtools/page/UI" }
        ]"#;
        assert_eq!(
            page_socket(list).as_deref(),
            Some("ws://127.0.0.1:1/devtools/page/PAGE")
        );
    }

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64_decode("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64_decode("").as_deref(), Some(&b""[..]));
        assert_eq!(base64_decode("not base64!"), None);
    }
}
