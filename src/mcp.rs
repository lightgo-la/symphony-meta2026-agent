//! MCP (Model Context Protocol) stdio client.
//!
//! Spawns an MCP server (a separate process) as a child and talks JSON-RPC with it.
//! Supports: `initialize` → `tools/list` → `tools/call`.
//! Only server settings (command/args) are stored on disk; secrets are never handled.

use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// MCP server definition stored in the config file.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// One MCP tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpTool {
    pub server: String,
    pub name: String,
    pub description: String,
}

/// MCP failure reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpError {
    Spawn(String),
    Io(String),
    Timeout,
    Protocol(String),
    Server(i64, String),
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(msg) => write!(f, "서버 실행 실패: {msg}"),
            Self::Io(msg) => write!(f, "입출력 오류: {msg}"),
            Self::Timeout => write!(f, "MCP 응답 시간 초과"),
            Self::Protocol(msg) => write!(f, "프로토콜 오류: {msg}"),
            Self::Server(code, msg) => write!(f, "서버 오류({code}): {msg}"),
        }
    }
}

impl std::error::Error for McpError {}

/// Builds a single JSON-RPC request.
pub fn build_request(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

/// Extracts the tool list from a `tools/list` response.
pub fn parse_tools(server: &str, resp: &serde_json::Value) -> Result<Vec<McpTool>, McpError> {
    let result = resp
        .get("result")
        .ok_or_else(|| McpError::Protocol("result 필드 없음".to_string()))?;
    let tools = result
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| McpError::Protocol("tools 배열 없음".to_string()))?;
    let mut out = Vec::with_capacity(tools.len());
    for t in tools {
        let name = t
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| McpError::Protocol("tool.name 없음".to_string()))?;
        out.push(McpTool {
            server: server.to_string(),
            name: name.to_string(),
            description: t
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(out)
}

/// Joins the text from a `tools/call` response. Returns Err when `isError` is set.
pub fn parse_tool_result(resp: &serde_json::Value) -> Result<String, McpError> {
    let result = resp
        .get("result")
        .ok_or_else(|| McpError::Protocol("result 필드 없음".to_string()))?;
    let mut text = String::new();
    if let Some(contents) = result.get("content").and_then(|c| c.as_array()) {
        for item in contents {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(s) = item.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(s);
                }
            }
        }
    }
    if text.is_empty() {
        // Without text blocks, the raw JSON is returned as-is.
        text = result.to_string();
    }
    if result.get("isError").and_then(|v| v.as_bool()) == Some(true) {
        return Err(McpError::Server(-1, text));
    }
    Ok(text)
}

fn check_rpc_error(resp: &serde_json::Value) -> Result<(), McpError> {
    if let Some(err) = resp.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();
        return Err(McpError::Server(code, msg));
    }
    Ok(())
}

struct Incoming {
    id: Option<u64>,
    body: serde_json::Value,
}

/// One live MCP server connection.
pub struct McpClient {
    server_name: String,
    child: Child,
    stdin: Mutex<ChildStdin>,
    /// Sends only one request at a time.
    call_lock: Mutex<()>,
    inbox: Mutex<mpsc::Receiver<Incoming>>,
    next_id: AtomicU64,
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Kill the child process on disconnect/replace so it never stays a zombie.
        // Closing stdout also lets the receiver thread exit.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl McpClient {
    /// Spawns the process and completes the handshake.
    pub fn connect(config: &McpServerConfig, timeout: Duration) -> Result<Self, McpError> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| McpError::Spawn(format!("{}: {e}", config.command)))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Spawn("stdin 파이프 실패".to_string()))?;
        let stdout: ChildStdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Spawn("stdout 파이프 실패".to_string()))?;

        let (tx, rx) = mpsc::channel();
        let reader_name = format!("mcp-{}", config.name);
        std::thread::Builder::new()
            .name(reader_name)
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(body) = serde_json::from_str::<serde_json::Value>(line) {
                        let id = body.get("id").and_then(|v| v.as_u64());
                        if tx.send(Incoming { id, body }).is_err() {
                            break;
                        }
                    }
                }
            })
            .map_err(|e| McpError::Spawn(format!("수신 스레드 실패: {e}")))?;

        let client = Self {
            server_name: config.name.clone(),
            child,
            stdin: Mutex::new(stdin),
            call_lock: Mutex::new(()),
            inbox: Mutex::new(rx),
            next_id: AtomicU64::new(1),
        };
        client.initialize(timeout)?;
        Ok(client)
    }

    fn initialize(&self, timeout: Duration) -> Result<(), McpError> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "prompt-canvas", "version": env!("CARGO_PKG_VERSION")},
        });
        let resp = self.call("initialize", params, timeout)?;
        if resp.get("result").is_none() {
            return Err(McpError::Protocol("initialize 실패".to_string()));
        }
        // A notification; no response is awaited.
        let note = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        });
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| McpError::Protocol("stdin 잠금 실패".to_string()))?;
        writeln!(stdin, "{}", note).map_err(|e| McpError::Io(e.to_string()))?;
        stdin.flush().map_err(|e| McpError::Io(e.to_string()))?;
        Ok(())
    }

    fn call(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, McpError> {
        let _guard = self
            .call_lock
            .lock()
            .map_err(|_| McpError::Protocol("호출 잠금 실패".to_string()))?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = build_request(id, method, params);
        {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|_| McpError::Protocol("stdin 잠금 실패".to_string()))?;
            writeln!(stdin, "{req}").map_err(|e| McpError::Io(e.to_string()))?;
            stdin.flush().map_err(|e| McpError::Io(e.to_string()))?;
        }
        let inbox = self
            .inbox
            .lock()
            .map_err(|_| McpError::Protocol("수신함 잠금 실패".to_string()))?;
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(McpError::Timeout);
            }
            match inbox.recv_timeout(deadline - now) {
                Ok(msg) if msg.id == Some(id) => {
                    check_rpc_error(&msg.body)?;
                    return Ok(msg.body);
                }
                Ok(_) => continue, // ignore unrelated notifications/responses
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(McpError::Timeout),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(McpError::Io("서버 출력 종료".to_string()));
                }
            }
        }
    }

    pub fn list_tools(&self, timeout: Duration) -> Result<Vec<McpTool>, McpError> {
        let resp = self.call("tools/list", serde_json::json!({}), timeout)?;
        parse_tools(&self.server_name, &resp)
    }

    /// Calls a tool and returns the text result. `arguments` must be a JSON object.
    pub fn call_tool(
        &self,
        tool: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<String, McpError> {
        let resp = self.call(
            "tools/call",
            serde_json::json!({"name": tool, "arguments": arguments}),
            timeout,
        )?;
        parse_tool_result(&resp)
    }
}

/// Manager holding multiple MCP servers' configs. Only connected clients are stored.
#[derive(Default)]
pub struct McpManager {
    clients: HashMap<String, Arc<McpClient>>,
}

impl McpManager {
    pub fn is_connected(&self, name: &str) -> bool {
        self.clients.contains_key(name)
    }

    pub fn connect(&mut self, config: &McpServerConfig, timeout: Duration) -> Result<(), McpError> {
        let client = McpClient::connect(config, timeout)?;
        self.clients.insert(config.name.clone(), Arc::new(client));
        Ok(())
    }

    pub fn disconnect(&mut self, name: &str) {
        self.clients.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<Arc<McpClient>> {
        self.clients.get(name).cloned()
    }

    pub fn connected_names(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_jsonrpc_shape() {
        let req = build_request(7, "tools/list", serde_json::json!({}));
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["id"], 7);
        assert_eq!(req["method"], "tools/list");
    }

    #[test]
    fn tools_list_parses_names() {
        let resp = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {"tools": [
                {"name": "search", "description": "웹 검색"},
                {"name": "fetch"}
            ]}
        });
        let tools = parse_tools("srv", &resp).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "search");
        assert_eq!(tools[1].description, "");
        assert_eq!(tools[0].server, "srv");
    }

    #[test]
    fn tool_result_joins_text_blocks() {
        let resp = serde_json::json!({
            "jsonrpc": "2.0", "id": 2,
            "result": {"content": [
                {"type": "text", "text": "첫째"},
                {"type": "text", "text": "둘째"}
            ], "isError": false}
        });
        assert_eq!(parse_tool_result(&resp).unwrap(), "첫째\n둘째");
    }

    #[test]
    fn tool_result_error_becomes_err() {
        let resp = serde_json::json!({
            "jsonrpc": "2.0", "id": 2,
            "result": {"content": [{"type": "text", "text": "폭발"}], "isError": true}
        });
        assert!(matches!(
            parse_tool_result(&resp),
            Err(McpError::Server(_, _))
        ));
    }

    #[test]
    fn rpc_error_object_maps_to_server_error() {
        let resp = serde_json::json!({
            "jsonrpc": "2.0", "id": 3,
            "error": {"code": -32601, "message": "Method not found"}
        });
        assert!(matches!(
            check_rpc_error(&resp),
            Err(McpError::Server(-32601, _))
        ));
    }
}
