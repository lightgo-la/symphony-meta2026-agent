//! Per-node agent mode: the LLM hops across MCP tools to build an answer.
//!
//! Protocol: each turn the model returns one of the JSON shapes below.
//!   {"action":"tool","server":"...","tool":"...","arguments":{...}}
//!   {"action":"done","answer":"..."}
//! Prose or code fences wrapping the JSON are allowed; the object is found inside.

use std::sync::Arc;
use std::time::Duration;

use crate::llm::{ContextBlock, LlmClient, LlmRequest};
use crate::mcp::McpClient;

/// One tool exposed to the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTool {
    pub server: String,
    pub name: String,
    pub description: String,
}

/// Tool-execution backend. Production uses MCP, tests use a fake.
pub trait ToolBackend: Send + Sync {
    fn catalog(&self) -> Vec<AgentTool>;
    fn call(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        timeout: Duration,
    ) -> Result<String, String>;
}

/// Backend that uses the connected MCP servers as-is.
pub struct McpBackend {
    clients: Vec<(String, Arc<McpClient>)>,
    tools: Vec<AgentTool>,
}

impl McpBackend {
    pub fn new(clients: Vec<(String, Arc<McpClient>)>, tools: Vec<AgentTool>) -> Self {
        Self { clients, tools }
    }

    fn pick(&self, server: &str) -> Option<Arc<McpClient>> {
        if !server.is_empty() {
            if let Some((_, client)) = self.clients.iter().find(|(name, _)| name == server) {
                return Some(client.clone());
            }
        }
        if self.clients.len() == 1 {
            return Some(self.clients[0].1.clone());
        }
        None
    }
}

impl ToolBackend for McpBackend {
    fn catalog(&self) -> Vec<AgentTool> {
        self.tools.clone()
    }

    fn call(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        timeout: Duration,
    ) -> Result<String, String> {
        let Some(client) = self.pick(server) else {
            return Err(format!("no connected server for '{server}'"));
        };
        client
            .call_tool(tool, args, timeout)
            .map_err(|e| e.to_string())
    }
}

/// Extracts the action from a model response.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentAction {
    ToolCall {
        server: String,
        tool: String,
        args: serde_json::Value,
    },
    Done {
        answer: String,
    },
    Invalid,
}

pub fn parse_action(text: &str) -> AgentAction {
    let Some(body) = extract_json(text) else {
        return AgentAction::Invalid;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return AgentAction::Invalid;
    };
    match value.get("action").and_then(|a| a.as_str()) {
        Some("tool") => {
            let server = value
                .get("server")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let Some(tool) = value.get("tool").and_then(|v| v.as_str()) else {
                return AgentAction::Invalid;
            };
            AgentAction::ToolCall {
                server,
                tool: tool.to_string(),
                args: value
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            }
        }
        Some("done") => AgentAction::Done {
            answer: value
                .get("answer")
                .and_then(|a| a.as_str())
                .unwrap_or("")
                .to_string(),
        },
        _ => AgentAction::Invalid,
    }
}

/// Finds the JSON object inside code fences or surrounding prose.
fn extract_json(text: &str) -> Option<&str> {
    let fenced = text
        .split("```")
        .nth(1)
        .map(|s| s.trim().trim_start_matches("json").trim());
    if let Some(body) = fenced {
        if body.starts_with('{') {
            return Some(body.trim_end_matches('`').trim());
        }
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(text[start..=end].trim())
    } else {
        None
    }
}

/// Builds the system prompt for the agent.
pub fn agent_system_prompt(tools: &[AgentTool], lang: &str) -> String {
    let mut out = String::from(
        "You are a tool-using agent. Reply with exactly one JSON object per turn, no other text.\n",
    );
    out.push_str("To call a tool: {\"action\":\"tool\",\"server\":\"<server>\",\"tool\":\"<name>\",\"arguments\":{...}}.\n");
    out.push_str("When done: {\"action\":\"done\",\"answer\":\"<final answer>\"}.\n");
    if tools.is_empty() {
        out.push_str("No tools are available; answer directly with a done action.\n");
    } else {
        out.push_str("Available tools:\n");
        for tool in tools {
            out.push_str(&format!(
                "- {}/{}: {}\n",
                tool.server, tool.name, tool.description
            ));
        }
    }
    out.push_str(&format!("Write the final answer in {lang}.\n"));
    out
}

pub struct AgentResult {
    pub answer: String,
    pub steps: u32,
}

/// Runs the agent loop. Progress is reported through `progress`.
pub fn run_agent(
    client: &dyn LlmClient,
    backend: &dyn ToolBackend,
    req: &LlmRequest,
    max_steps: u32,
    per_call_timeout: Duration,
    progress: &std::sync::mpsc::Sender<String>,
) -> Result<AgentResult, String> {
    let tools = backend.catalog();
    let mut history: Vec<ContextBlock> = req.context.clone();
    let system = agent_system_prompt(&tools, "the user's language");
    let mut invalid_streak = 0u32;
    let steps = max_steps.clamp(1, 10);
    for step in 1..=steps {
        let _ = progress.send(format!("agent step {step}/{steps}"));
        let call_req = LlmRequest {
            prompt: req.prompt.clone(),
            context: {
                let mut ctx = vec![ContextBlock::new("agent-protocol", system.clone())];
                ctx.extend(history.iter().cloned());
                ctx
            },
            model: req.model.clone(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            system: req.system.clone(),
        };
        let text = client
            .generate(&call_req)
            .map_err(|e| format!("llm: {e}"))?;
        match parse_action(&text) {
            AgentAction::Done { answer } => {
                return Ok(AgentResult {
                    answer: if answer.trim().is_empty() {
                        text
                    } else {
                        answer
                    },
                    steps: step,
                });
            }
            AgentAction::ToolCall { server, tool, args } => {
                invalid_streak = 0;
                let server_name = if server.is_empty() {
                    tools
                        .iter()
                        .find(|t| t.name == tool)
                        .map(|t| t.server.clone())
                        .unwrap_or_default()
                } else {
                    server
                };
                let result = backend
                    .call(&server_name, &tool, args, per_call_timeout)
                    .unwrap_or_else(|e| format!("tool error: {e}"));
                history.push(ContextBlock::new(
                    format!("tool:{server_name}/{tool}"),
                    result,
                ));
            }
            AgentAction::Invalid => {
                invalid_streak += 1;
                if invalid_streak >= 2 {
                    return Ok(AgentResult {
                        answer: text,
                        steps: step,
                    });
                }
                history.push(ContextBlock::new(
                    "agent-note",
                    "Reply with exactly one JSON action object.".to_string(),
                ));
            }
        }
    }
    // Exhausting all steps without a final answer does not count as success.
    // Return Err so batch failure accounting picks it up.
    Err("최대 단계에 도달했으나 최종 답을 받지 못했습니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::MockClient;

    struct FakeBackend;

    impl ToolBackend for FakeBackend {
        fn catalog(&self) -> Vec<AgentTool> {
            vec![AgentTool {
                server: "s".to_string(),
                name: "echo".to_string(),
                description: "echoes".to_string(),
            }]
        }

        fn call(
            &self,
            _server: &str,
            tool: &str,
            _args: serde_json::Value,
            _timeout: Duration,
        ) -> Result<String, String> {
            Ok(format!("called {tool}"))
        }
    }

    #[test]
    fn parses_tool_action() {
        let action = parse_action(
            "```json\n{\"action\":\"tool\",\"server\":\"s\",\"tool\":\"echo\",\"arguments\":{\"x\":1}}\n```",
        );
        assert_eq!(
            action,
            AgentAction::ToolCall {
                server: "s".to_string(),
                tool: "echo".to_string(),
                args: serde_json::json!({"x": 1}),
            }
        );
    }

    #[test]
    fn parses_done_action() {
        let action = parse_action("{\"action\":\"done\",\"answer\":\"hi\"}");
        assert_eq!(
            action,
            AgentAction::Done {
                answer: "hi".to_string()
            }
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_action("just text"), AgentAction::Invalid);
        assert_eq!(parse_action("{\"action\":\"nope\"}"), AgentAction::Invalid);
    }

    #[test]
    fn system_prompt_lists_tools() {
        let tools = FakeBackend.catalog();
        let prompt = agent_system_prompt(&tools, "Korean");
        assert!(prompt.contains("s/echo"));
        assert!(prompt.contains("Korean"));
    }

    /// Live-API agent smoke test: checks that a tool-less loop comes back alive.
    /// MODEL_API_KEY=... cargo test -- --ignored live_agent_loop
    #[test]
    #[ignore]
    fn live_agent_loop() {
        use crate::llm::MetaClient;
        let key = std::env::var("MODEL_API_KEY").expect("MODEL_API_KEY 필요");
        let client = MetaClient::new(
            "https://api.meta.ai/v1/chat/completions",
            Some(key),
            Duration::from_secs(60),
        );
        struct NoTools;
        impl ToolBackend for NoTools {
            fn catalog(&self) -> Vec<AgentTool> {
                vec![]
            }
            fn call(
                &self,
                _server: &str,
                _tool: &str,
                _args: serde_json::Value,
                _timeout: Duration,
            ) -> Result<String, String> {
                Err("no tools".to_string())
            }
        }
        let req = LlmRequest::new(
            "Reply with exactly: done".to_string(),
            vec![],
            "muse-spark-1.3".to_string(),
        );
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = run_agent(&client, &NoTools, &req, 3, Duration::from_secs(60), &tx)
            .expect("에이전트 루프 실패");
        assert!(!result.answer.trim().is_empty());
        println!(
            "live agent: {}단계, {}자",
            result.steps,
            result.answer.chars().count()
        );
    }

    #[test]
    fn mock_answer_ends_the_loop() {
        let client = MockClient::new(0);
        let req = LlmRequest::new("hi".to_string(), vec![], "m".to_string());
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = run_agent(&client, &FakeBackend, &req, 5, Duration::from_secs(5), &tx);
        let result = result.unwrap();
        // Mock output is not a JSON action, so after 2 consecutive invalid verdicts the raw text is returned.
        assert_eq!(result.steps, 2);
        assert!(!result.answer.is_empty());
    }
}
