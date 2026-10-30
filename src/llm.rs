//! LLM abstraction: Mock + Meta Model API (OpenAI-compatible chat/completions).
//!
//! The API key is saved to disk only when 'save key on this PC' is on (stored in plain text).
//! Precedence: session-entered key (memory) > saved key > `MODEL_API_KEY` env var.

use std::fmt;
use std::time::Duration;

/// Context block sent along with the prompt (MCP tool results, etc.).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextBlock {
    pub source: String,
    pub text: String,
}

impl ContextBlock {
    pub fn new(source: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            text: text.into(),
        }
    }
}

/// Generation request.
#[derive(Clone, Debug)]
pub struct LlmRequest {
    pub prompt: String,
    pub context: Vec<ContextBlock>,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system: String,
}

impl LlmRequest {
    pub fn new(prompt: String, context: Vec<ContextBlock>, model: String) -> Self {
        Self {
            prompt,
            context,
            model,
            temperature: None,
            max_tokens: None,
            system: String::new(),
        }
    }

    pub fn with_options(
        mut self,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        system: String,
    ) -> Self {
        self.temperature = temperature;
        self.max_tokens = max_tokens;
        self.system = system;
        self
    }

    /// Builds the chat/completions `messages` payload.
    /// Order: system message → context (system) → prompt (user).
    pub fn messages(&self) -> Vec<ChatMessage> {
        let mut out = Vec::with_capacity(self.context.len() + 2);
        if !self.system.trim().is_empty() {
            out.push(ChatMessage {
                role: "system".to_string(),
                content: self.system.clone(),
            });
        }
        for block in &self.context {
            out.push(ChatMessage {
                role: "system".to_string(),
                content: format!("[context:{}]\n{}", block.source, block.text),
            });
        }
        out.push(ChatMessage {
            role: "user".to_string(),
            content: self.prompt.clone(),
        });
        out
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Reason an LLM call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LlmError {
    EmptyPrompt,
    MissingKey,
    Timeout,
    Transport(String),
    Api(u16, String),
    BadResponse(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPrompt => write!(f, "프롬프트가 비어 있습니다"),
            Self::MissingKey => write!(
                f,
                "API 키가 없습니다. 설정에서 키를 입력하거나 MODEL_API_KEY 환경 변수를 지정하세요"
            ),
            Self::Timeout => write!(f, "요청 시간이 초과되었습니다"),
            Self::Transport(msg) => write!(f, "네트워크 오류: {msg}"),
            Self::Api(status, msg) => write!(f, "API 오류({status}): {msg}"),
            Self::BadResponse(msg) => write!(f, "응답 해석 실패: {msg}"),
        }
    }
}

impl std::error::Error for LlmError {}

/// Trait every LLM backend implements. Called from worker threads, hence `Send + Sync`.
pub trait LlmClient: Send + Sync {
    fn name(&self) -> &str;
    fn generate(&self, req: &LlmRequest) -> Result<String, LlmError>;
}

/// Local demo client. Uses no network.
pub struct MockClient {
    /// Adds a slight delay in real runs (not tests) so the spinner is visible.
    pub delay_ms: u64,
}

impl MockClient {
    pub fn new(delay_ms: u64) -> Self {
        Self { delay_ms }
    }
}

impl LlmClient for MockClient {
    fn name(&self) -> &str {
        "mock"
    }

    fn generate(&self, req: &LlmRequest) -> Result<String, LlmError> {
        if req.prompt.trim().is_empty() {
            return Err(LlmError::EmptyPrompt);
        }
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        Ok(mock_generate(&req.prompt))
    }
}

/// Meta Model API client (OpenAI-compatible chat/completions).
///
/// * endpoint example: `https://api.meta.ai/v1/chat/completions`
/// * auth: `Authorization: Bearer <key>`
pub struct MetaClient {
    pub endpoint: String,
    /// Uses the `MODEL_API_KEY` env var when `None`.
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl MetaClient {
    pub fn new(endpoint: impl Into<String>, api_key: Option<String>, timeout: Duration) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_key,
            timeout,
        }
    }

    fn resolved_key(&self) -> Result<String, LlmError> {
        if let Some(key) = self
            .api_key
            .as_ref()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
        {
            return Ok(key);
        }
        std::env::var("MODEL_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .ok_or(LlmError::MissingKey)
    }
}

#[derive(serde::Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    #[serde(default)]
    message: Option<ResponseMessage>,
}

#[derive(serde::Deserialize, Default)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

impl LlmClient for MetaClient {
    fn name(&self) -> &str {
        "meta"
    }

    fn generate(&self, req: &LlmRequest) -> Result<String, LlmError> {
        if req.prompt.trim().is_empty() {
            return Err(LlmError::EmptyPrompt);
        }
        let key = self.resolved_key()?;
        let client = crate::http::shared_client(self.timeout).map_err(LlmError::Transport)?;
        let payload = ChatRequest {
            model: &req.model,
            messages: req.messages(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        };
        let resp = client
            .post(&self.endpoint)
            .bearer_auth(key)
            .json(&payload)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Transport(trim_error(&e.to_string()))
                }
            })?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().unwrap_or_default();
            return Err(LlmError::Api(status, trim_error(&body)));
        }
        let parsed: ChatResponse = resp
            .json()
            .map_err(|e| LlmError::BadResponse(trim_error(&e.to_string())))?;
        parse_chat_content(&parsed)
    }
}

/// Extracts the first text from a chat/completions response.
fn parse_chat_content(resp: &ChatResponse) -> Result<String, LlmError> {
    resp.choices
        .first()
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.content.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LlmError::BadResponse("choices가 비어 있습니다".to_string()))
}

/// Truncates error strings so they cannot grow unbounded (e.g. HTML error pages).
fn trim_error(s: &str) -> String {
    const LIMIT: usize = 300;
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > LIMIT {
        flat.chars().take(LIMIT).collect::<String>() + "…"
    } else {
        flat
    }
}

/// Local demo output generator.
pub fn mock_generate(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return "프롬프트가 비어 있습니다. 위 입력창에 텍스트를 입력하세요.".to_string();
    }
    let chars = trimmed.chars().count();
    let lines = trimmed.lines().count();
    let preview: String = trimmed.chars().take(120).collect();
    let ellipsis = if chars > 120 { "…" } else { "" };
    format!(
        "[개별 출력]\n- 입력: {chars}자 / {lines}줄\n- 요약: \"{preview}{ellipsis}\"\n- 결과 1: 위 요청을 3단계로 나누어 처리했습니다.\n- 결과 2: 핵심 키워드 3개를 뽑아 확장 문장을 만들었습니다.\n(데모 출력 — 설정에서 Meta API로 전환하세요)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prompt_is_rejected_before_network() {
        let client = MockClient::new(0);
        let req = LlmRequest::new("   \n ".to_string(), vec![], "m".to_string());
        assert_eq!(client.generate(&req), Err(LlmError::EmptyPrompt));
    }

    #[test]
    fn messages_put_context_first_then_prompt() {
        let req = LlmRequest::new(
            "질문".to_string(),
            vec![ContextBlock::new("file", "내용")],
            "m".to_string(),
        );
        let msgs = req.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("file"));
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "질문");
    }

    #[test]
    fn chat_content_parses_first_choice() {
        let resp: ChatResponse = serde_json::from_str(
            r#"{"choices":[{"message":{"role":"assistant","content":"  OK  "}}]}"#,
        )
        .unwrap();
        assert_eq!(parse_chat_content(&resp).unwrap(), "OK");
    }

    #[test]
    fn chat_content_rejects_empty_choices() {
        let resp: ChatResponse = serde_json::from_str(r#"{"choices":[]}"#).unwrap();
        assert!(matches!(
            parse_chat_content(&resp),
            Err(LlmError::BadResponse(_))
        ));
    }

    #[test]
    fn session_key_beats_env_and_blank_is_missing() {
        let client = MetaClient::new(
            "http://localhost:9",
            Some("  ".to_string()),
            Duration::from_secs(1),
        );
        // A blank session key falls through to the env var. The test env has
        // no MODEL_API_KEY, so this must be MissingKey.
        let req = LlmRequest::new("hi".to_string(), vec![], "m".to_string());
        std::env::remove_var("MODEL_API_KEY");
        assert_eq!(client.generate(&req), Err(LlmError::MissingKey));
    }

    #[test]
    fn error_text_is_bounded() {
        let long = "x".repeat(2000);
        assert!(trim_error(&long).chars().count() <= 301);
    }

    /// Live API smoke test: actually calls 10 short prompts.
    /// Skipped in the default suite; run explicitly only when passing a key:
    /// MODEL_API_KEY=... cargo test -- --ignored live_meta_ten_outputs
    /// The key is taken from the env var only and never written to disk.
    #[test]
    #[ignore]
    fn live_meta_ten_outputs() {
        let key = std::env::var("MODEL_API_KEY").expect("MODEL_API_KEY 필요");
        let client = MetaClient::new(
            "https://api.meta.ai/v1/chat/completions",
            Some(key),
            Duration::from_secs(60),
        );
        let prompts = [
            "Reply with exactly: one",
            "Reply with exactly: two",
            "1+1은? 숫자로만 답하세요.",
            "Reply with exactly: four",
            "하늘 색은? 한 단어로 답하세요.",
            "Reply with exactly: six",
            "2*3은? 숫자로만 답하세요.",
            "Reply with exactly: eight",
            "물의 화학식은? 그것만 답하세요.",
            "Reply with exactly: ten",
        ];
        for (i, p) in prompts.iter().enumerate() {
            let req = LlmRequest::new(p.to_string(), vec![], "muse-spark-1.3".to_string());
            let out = client
                .generate(&req)
                .unwrap_or_else(|e| panic!("{i}번째 호출 실패: {e}"));
            assert!(!out.trim().is_empty(), "{i}번째 출력이 비었음");
            println!(
                "live[{i}] {}자: {}",
                out.chars().count(),
                out.chars().take(60).collect::<String>()
            );
        }
    }
}
