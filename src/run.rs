//! Background job execution: LLM, agents, MCP, and batches.

use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::agent::{run_agent, AgentTool, McpBackend};
use crate::i18n::{t, Lang};
use crate::llm::{ContextBlock, LlmClient, LlmError, LlmRequest, MetaClient, MockClient};
use crate::mcp::{McpClient, McpError};
use crate::media::MediaKind;
use crate::node::{
    ancestor_context, combined_results, find_free_spot, runnable_ids, runnable_subset,
    tile_positions, topo_order, NodeMode, PromptNode,
};
use crate::state::{App, BatchScope, BatchState};
use crate::store::{Provider, RunMode};

/// Background job failure reason. Categorized by kind instead of raw strings,
/// for display messages and retry decisions.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JobError {
    /// API key is empty or not configured.
    MissingKey,
    /// Network/HTTP failure (may include a status code).
    Network(String),
    /// Server returned an error code.
    ApiStatus(u16, String),
    /// Response parsing failed.
    Parse(String),
    /// Timed out.
    Timeout,
    /// Cancelled by the user.
    Cancelled,
    /// MCP server/tool problem.
    Mcp(String),
    /// Other.
    Other(String),
}

impl JobError {
    /// Converts to a display message.
    pub(crate) fn display(&self, lang: Lang) -> String {
        match self {
            JobError::MissingKey => {
                t(lang, "API 키가 없습니다. 설정에서 키를 입력하세요.").to_string()
            }
            JobError::Network(e) => t(lang, "네트워크 오류: {e}").replace("{e}", e),
            JobError::ApiStatus(code, body) => t(lang, "서버 오류 {code}: {body}")
                .replace("{code}", &code.to_string())
                .replace("{body}", &shorten(body, 200)),
            JobError::Parse(e) => t(lang, "응답 해석 실패: {e}").replace("{e}", e),
            JobError::Timeout => t(lang, "제한 시간 초과. 다시 실행하세요.").to_string(),
            JobError::Cancelled => t(lang, "취소됨.").to_string(),
            JobError::Mcp(e) => t(lang, "MCP 오류: {e}").replace("{e}", e),
            JobError::Other(e) => e.clone(),
        }
    }
    /// Checks whether this is a transient failure that may succeed on retry.
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(
            self,
            JobError::Network(_) | JobError::Timeout | JobError::ApiStatus(500..=599, _)
        )
    }
}

impl From<LlmError> for JobError {
    fn from(e: LlmError) -> Self {
        match e {
            LlmError::EmptyPrompt => JobError::Other("프롬프트가 비어 있습니다".to_string()),
            LlmError::MissingKey => JobError::MissingKey,
            LlmError::Timeout => JobError::Timeout,
            LlmError::Transport(msg) => JobError::Network(msg),
            LlmError::Api(status, msg) => JobError::ApiStatus(status, msg),
            LlmError::BadResponse(msg) => JobError::Parse(msg),
        }
    }
}

impl From<McpError> for JobError {
    fn from(e: McpError) -> Self {
        match e {
            McpError::Timeout => JobError::Timeout,
            other => JobError::Mcp(other.to_string()),
        }
    }
}

/// Current unix time in milliseconds. Avoids media filename collisions.
fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Truncates a long error body from the front.
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

/// Background job result.
pub(crate) enum JobResult {
    Text(String),
    Error(JobError),
}

#[derive(Clone)]
pub(crate) enum JobKind {
    Generate,
    Media {
        media: MediaKind,
    },
    Tool {
        server: String,
        tool: String,
    },
    ConnTest,
    /// External process execution (Python). Result handling matches Generate.
    Exec,
    /// Blueprint planning. Not attached to a node; polled from a separate slot.
    Blueprint,
}

pub(crate) struct PendingJob {
    pub(crate) rx: mpsc::Receiver<JobResult>,
    pub(crate) progress: Option<mpsc::Receiver<String>>,
    pub(crate) started: Instant,
    pub(crate) timeout: Duration,
    pub(crate) kind: JobKind,
}

/// Spawns a background job on a named thread and returns the result receiver.
/// Thread names exist to distinguish jobs in the profiler.
/// Spawn failure is extremely rare; then only the channel closes and the poll side treats it as aborted.
fn spawn_job<F>(name: String, job: F) -> mpsc::Receiver<JobResult>
where
    F: FnOnce() -> JobResult + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    if let Err(e) = std::thread::Builder::new().name(name).spawn(move || {
        let _ = tx.send(job());
    }) {
        eprintln!("job thread spawn failed: {e}");
    }
    rx
}

/// Slow-frame detection. Only when `PC_FRAME_LOG=1`, logs frame intervals over 50ms to stderr.
/// Called at the end of every `update()`. Normally returns right after a one-time env check.
pub(crate) fn log_slow_frame(nodes: usize, pending: usize) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static LAST_MS: AtomicU64 = AtomicU64::new(0);
    if !*ENABLED.get_or_init(|| std::env::var("PC_FRAME_LOG").is_ok_and(|v| v != "0")) {
        return;
    }
    let now = unix_ms() as u64;
    let prev = LAST_MS.swap(now, Ordering::Relaxed);
    if prev == 0 {
        return;
    }
    let dt = now.saturating_sub(prev);
    if dt > 50 {
        eprintln!("slow frame: {dt}ms (nodes={nodes} pending={pending})");
    }
}

/// Runs Python code in a separate process.
/// Looks up `python` → `python3` → `py` in order and returns combined stdout/stderr.
fn run_python(code: &str, timeout: Duration) -> JobResult {
    let mut child_opt = None;
    let mut last_err = String::new();
    for exe in ["python", "python3", "py"] {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("-c")
            .arg(code)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            // Keeps the console window from flashing.
            cmd.creation_flags(0x0800_0000);
        }
        match cmd.spawn() {
            Ok(child) => {
                child_opt = Some(child);
                break;
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    let Some(mut child) = child_opt else {
        return JobResult::Error(JobError::Other(format!(
            "파이썬을 찾을 수 없습니다({last_err}). python을 설치하고 PATH에 등록하세요."
        )));
    };
    // Reads output on separate threads so a full pipe never blocks the child.
    // On spawn failure, leaves just that stream empty.
    let out_reader = child.stdout.take().and_then(|mut h| {
        std::thread::Builder::new()
            .name("py-stdout".to_string())
            .spawn(move || {
                use std::io::Read as _;
                let mut buf = Vec::new();
                let _ = h.read_to_end(&mut buf);
                buf
            })
            .ok()
    });
    let err_reader = child.stderr.take().and_then(|mut h| {
        std::thread::Builder::new()
            .name("py-stderr".to_string())
            .spawn(move || {
                use std::io::Read as _;
                let mut buf = Vec::new();
                let _ = h.read_to_end(&mut buf);
                buf
            })
            .ok()
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = out_reader.and_then(|t| t.join().ok()).unwrap_or_default();
                let err = err_reader.and_then(|t| t.join().ok()).unwrap_or_default();
                let stdout = String::from_utf8_lossy(&out);
                let stderr = String::from_utf8_lossy(&err);
                return combine_exec_output(status.success(), &stdout, &stderr);
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return JobResult::Error(JobError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return JobResult::Error(JobError::Other(e.to_string())),
        }
    }
}

/// Combines a process run result. On success returns stdout (+stderr) as text,
/// on failure returns the combined output as an error. Truncates from the front when too long.
pub(crate) fn combine_exec_output(success: bool, stdout: &str, stderr: &str) -> JobResult {
    let mut text = stdout.trim_end().to_string();
    if !stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push_str("\n[stderr]\n");
        }
        text.push_str(stderr.trim_end());
    }
    let text = shorten(&text, 50_000);
    if success {
        JobResult::Text(text)
    } else if text.is_empty() {
        JobResult::Error(JobError::Other("종료 코드가 0이 아님".to_string()))
    } else {
        JobResult::Error(JobError::Other(text))
    }
}

impl App {
    // ── LLM execution ────────────────────────────────────────────

    pub(crate) fn timeout(&self) -> Duration {
        let _lang = self.config.lang;
        Duration::from_secs(self.config.timeout_secs.max(10))
    }

    pub(crate) fn build_client(&self) -> Arc<dyn LlmClient> {
        let _lang = self.config.lang;
        match self.config.provider {
            Provider::Mock => Arc::new(MockClient::new(600)),
            Provider::Meta => Arc::new(MetaClient::new(
                self.config.endpoint.clone(),
                self.api_key_value(),
                self.timeout(),
            )),
        }
    }

    /// Returns Some when the session key is non-empty, None otherwise (falls back to env vars).
    /// Effective API key: session input > saved key. Empty means env vars are used.
    pub(crate) fn api_key_value(&self) -> Option<String> {
        let _lang = self.config.lang;
        let session = self.session_key.trim();
        if !session.is_empty() {
            return Some(session.to_string());
        }
        let saved = self.config.saved_key.trim();
        if !saved.is_empty() {
            return Some(saved.to_string());
        }
        None
    }

    pub fn key_status_text(lang: Lang) -> (&'static str, bool) {
        let env_ok = std::env::var("MODEL_API_KEY")
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false);
        if env_ok {
            (t(lang, "환경변수 키: 설정됨"), true)
        } else {
            (t(lang, "환경변수 키: 미설정"), false)
        }
    }

    /// Records the model name used for a run (latest 10, deduplicated).
    pub(crate) fn note_model(&mut self, name: String) {
        let _lang = self.config.lang;
        if name.trim().is_empty() {
            return;
        }
        self.model_history.retain(|m| m != &name);
        self.model_history.insert(0, name);
        self.model_history.truncate(10);
    }

    pub(crate) fn spawn_generate(&mut self, node_id: usize) {
        let lang = self.config.lang;
        if self.pending.contains_key(&node_id) {
            return;
        }
        let Some(node) = self.nodes.iter().find(|n| n.id == node_id) else {
            return;
        };
        if node.prompt.trim().is_empty() {
            self.status =
                t(lang, "노드 '{}': 프롬프트가 비어 있습니다.").replace("{}", &node.title);
            return;
        }
        if node.mode == NodeMode::Agent {
            self.spawn_agent(node_id);
            return;
        }
        if node.mode == NodeMode::Image {
            self.spawn_media(node_id, MediaKind::Image);
            return;
        }
        if node.mode == NodeMode::Audio {
            self.spawn_media(node_id, MediaKind::Audio);
            return;
        }
        if node.mode == NodeMode::Python {
            self.spawn_python(node_id);
            return;
        }
        let client = self.build_client();
        // Prepends the outputs of connected source nodes.
        let mut context = ancestor_context(node, &self.nodes);
        context.extend(node.context.iter().cloned());
        // Falls back to the mode preset instructions when no custom system message exists.
        let system = if node.system.trim().is_empty() {
            node.mode.system_preset().unwrap_or("").to_string()
        } else {
            node.system.clone()
        };
        let model_name = node.effective_model(&self.config.model);
        let req = LlmRequest::new(node.prompt.clone(), context, model_name.clone()).with_options(
            node.temperature,
            node.max_tokens,
            system,
        );
        let title = node.title.clone();
        let client_name = client.name().to_string();
        self.note_model(model_name);
        let rx = spawn_job(format!("job-generate-{node_id}"), move || {
            match client.generate(&req) {
                Ok(text) => JobResult::Text(text),
                Err(e) => JobResult::Error(JobError::from(e)),
            }
        });
        self.pending.insert(
            node_id,
            PendingJob {
                rx,
                progress: None,
                started: Instant::now(),
                timeout: self.timeout(),
                kind: JobKind::Generate,
            },
        );
        self.status = t(lang, "노드 '{title}' 생성 중…({client_name})")
            .replace("{title}", &title)
            .replace("{client_name}", &client_name);
    }

    /// Image/audio generation. Returns the resulting file path as text.
    pub(crate) fn spawn_media(&mut self, node_id: usize, media: MediaKind) {
        let lang = self.config.lang;
        if self.pending.contains_key(&node_id) {
            return;
        }
        let Some(node) = self.nodes.iter().find(|n| n.id == node_id).cloned() else {
            return;
        };
        if node.prompt.trim().is_empty() {
            self.status =
                t(lang, "노드 '{}': 프롬프트가 비어 있습니다.").replace("{}", &node.title);
            return;
        }
        let mut parts = vec![node.prompt.clone()];
        parts.extend(
            ancestor_context(&node, &self.nodes)
                .into_iter()
                .map(|c| c.text),
        );
        let text = parts.join("\n");
        let stem = format!("node_{}_{}_{}", node_id, node.run_count + 1, unix_ms());
        let timeout = Duration::from_secs((self.config.timeout_secs.max(10) * 3).max(120));
        let rx = spawn_job(format!("job-media-{node_id}"), move || {
            let result = match media {
                MediaKind::Image => {
                    let prompt: String = text.chars().take(500).collect();
                    let url = crate::media::image_url(&prompt, node_id as u32);
                    match crate::media::fetch_bytes(&url, timeout) {
                        Ok(bytes) => match crate::media::sniff_image_ext(&bytes) {
                            Some(ext) => match crate::media::save_media(&stem, ext, &bytes) {
                                Ok(p) => JobResult::Text(p.display().to_string()),
                                Err(e) => JobResult::Error(JobError::Other(e)),
                            },
                            None => JobResult::Error(JobError::Parse(
                                "이미지 형식을 알 수 없음".to_string(),
                            )),
                        },
                        Err(e) => JobResult::Error(JobError::Network(e)),
                    }
                }
                MediaKind::Audio => {
                    let lang_id = crate::media::detect_lang(&text);
                    let mut all: Vec<u8> = Vec::new();
                    let mut ok = true;
                    let mut err = String::new();
                    for chunk in crate::media::tts_chunks(&text, 180) {
                        let url = crate::media::tts_url(&chunk, lang_id);
                        match crate::media::fetch_bytes(&url, timeout) {
                            Ok(mut b) => all.append(&mut b),
                            Err(e) => {
                                ok = false;
                                err = e;
                                break;
                            }
                        }
                    }
                    if ok {
                        match crate::media::save_media(&stem, media.ext(), &all) {
                            Ok(p) => JobResult::Text(p.display().to_string()),
                            Err(e) => JobResult::Error(JobError::Other(e)),
                        }
                    } else {
                        JobResult::Error(JobError::Network(err))
                    }
                }
            };
            result
        });
        self.pending.insert(
            node_id,
            PendingJob {
                rx,
                progress: None,
                started: Instant::now(),
                timeout,
                kind: JobKind::Media { media },
            },
        );
        let kind_label = match media {
            MediaKind::Image => t(lang, "이미지"),
            MediaKind::Audio => t(lang, "오디오"),
        };
        self.status = t(lang, "노드 '{title}' {kind} 생성 중…")
            .replace("{title}", &node.title)
            .replace("{kind}", kind_label);
    }

    /// Python node: runs the prompt as code and uses stdout as the result.
    pub(crate) fn spawn_python(&mut self, node_id: usize) {
        let lang = self.config.lang;
        if self.pending.contains_key(&node_id) {
            return;
        }
        let Some(node) = self.nodes.iter().find(|n| n.id == node_id).cloned() else {
            return;
        };
        if node.prompt.trim().is_empty() {
            self.status =
                t(lang, "노드 '{}': 프롬프트가 비어 있습니다.").replace("{}", &node.title);
            return;
        }
        let code = node.prompt.clone();
        let timeout = self.timeout();
        let rx = spawn_job(format!("job-python-{node_id}"), move || {
            run_python(&code, timeout)
        });
        self.pending.insert(
            node_id,
            PendingJob {
                rx,
                progress: None,
                started: Instant::now(),
                timeout,
                kind: JobKind::Exec,
            },
        );
        self.status = t(lang, "노드 '{title}' 파이썬 실행 중…").replace("{title}", &node.title);
    }

    pub(crate) fn spawn_agent(&mut self, node_id: usize) {
        let lang = self.config.lang;
        let Some(node) = self.nodes.iter().find(|n| n.id == node_id).cloned() else {
            return;
        };
        let connected = self.mcp.connected_names();
        if connected.is_empty() {
            self.status = t(lang, "에이전트는 연결된 MCP 서버가 필요합니다.").to_string();
            return;
        }
        let pairs: Vec<(String, Arc<McpClient>)> = connected
            .iter()
            .filter_map(|name| self.mcp.get(name).map(|c| (name.clone(), c)))
            .collect();
        let tools: Vec<AgentTool> = self
            .mcp_tools
            .iter()
            .filter(|tool| connected.contains(&tool.server))
            .map(|tool| AgentTool {
                server: tool.server.clone(),
                name: tool.name.clone(),
                description: tool.description.clone(),
            })
            .collect();
        let client = self.build_client();
        let mut context = ancestor_context(&node, &self.nodes);
        context.extend(node.context.iter().cloned());
        let req = LlmRequest::new(
            node.prompt.clone(),
            context,
            node.effective_model(&self.config.model),
        )
        .with_options(node.temperature, node.max_tokens, node.system.clone());
        let title = node.title.clone();
        let max_steps = node.max_steps.clamp(1, 10);
        let timeout = self.timeout();
        let (ptx, prx) = mpsc::channel();
        let rx = spawn_job(format!("job-agent-{node_id}"), move || {
            let backend = McpBackend::new(pairs, tools);
            match run_agent(&*client, &backend, &req, max_steps, timeout, &ptx) {
                Ok(done) => {
                    JobResult::Text(format!("{}\n[에이전트 {}단계]", done.answer, done.steps))
                }
                Err(e) => JobResult::Error(JobError::Other(e)),
            }
        });
        self.pending.insert(
            node_id,
            PendingJob {
                rx,
                progress: Some(prx),
                started: Instant::now(),
                timeout: timeout * (max_steps + 1),
                kind: JobKind::Generate,
            },
        );
        self.status = t(lang, "노드 '{title}' 생성 중…({client_name})")
            .replace("{title}", &title)
            .replace("{client_name}", "agent");
    }

    pub(crate) fn run_batch(&mut self) {
        let busy: Vec<usize> = self.pending.keys().copied().collect();
        let targets = runnable_ids(&self.nodes, &busy);
        let pool: Vec<usize> = self.nodes.iter().map(|n| n.id).collect();
        let (skipped_off, skipped_empty) = Self::skipped_counts(&self.nodes, &pool);
        self.start_batch(targets, BatchScope::All, skipped_off, skipped_empty);
    }

    /// Run selected: runs only the chosen nodes through the same batch machinery (mode, progress, cancel).
    /// In chain order mode, inputs outside the selection are treated as already finished.
    pub(crate) fn run_selected(&mut self) {
        let lang = self.config.lang;
        let wanted: Vec<usize> = self.selected.iter().copied().collect();
        if wanted.is_empty() {
            self.status = t(lang, "먼저 노드를 선택하세요 (Shift+클릭·Shift+드래그).").to_string();
            return;
        }
        let busy: Vec<usize> = self.pending.keys().copied().collect();
        let targets = runnable_subset(&self.nodes, &wanted, &busy);
        if targets.is_empty() {
            self.status = t(
                lang,
                "선택한 노드는 실행할 게 없습니다 (OFF·빈 프롬프트·실행 중 제외).",
            )
            .to_string();
            return;
        }
        let (skipped_off, skipped_empty) = Self::skipped_counts(&self.nodes, &wanted);
        self.start_batch(targets, BatchScope::Selected, skipped_off, skipped_empty);
    }

    /// Per-node schedules: runs each due node one at a time.
    /// Left untouched while a batch runs (fires in turn once it ends);
    /// busy, OFF, or empty-prompt nodes are deferred to the next tick.
    pub(crate) fn tick_node_schedules(&mut self) {
        if matches!(self.batch, BatchState::Running { .. }) {
            return;
        }
        let now = chrono::Local::now();
        // Collects only ids and titles first to end borrows before firing.
        let mut fired: Vec<(usize, String)> = Vec::new();
        for node in self.nodes.iter_mut() {
            if !node.sched.enabled {
                continue;
            }
            node.sched.arm(now);
            if !node.sched.due(now) {
                continue;
            }
            if !node.enabled || node.prompt.trim().is_empty() {
                continue;
            }
            if self.pending.contains_key(&node.id) {
                continue;
            }
            if node.sched.fire(now, &node.output) {
                fired.push((node.id, node.title.clone()));
            }
        }
        for (id, title) in fired {
            self.spawn_generate(id);
            if self.pending.contains_key(&id) {
                let lang = self.config.lang;
                self.status = t(lang, "예약 실행: 노드 '{title}'").replace("{title}", &title);
            }
        }
    }

    /// Counts OFF and empty-prompt nodes in the candidate pool. For the start message.
    fn skipped_counts(nodes: &[PromptNode], pool: &[usize]) -> (usize, usize) {
        let off = pool
            .iter()
            .filter(|id| nodes.iter().any(|n| &n.id == *id && !n.enabled))
            .count();
        let empty = pool
            .iter()
            .filter(|id| {
                nodes
                    .iter()
                    .any(|n| &n.id == *id && n.enabled && n.prompt.trim().is_empty())
            })
            .count();
        (off, empty)
    }

    /// Batch start body. Run-all and run-selected share mode, progress, and finish handling.
    fn start_batch(
        &mut self,
        targets: Vec<usize>,
        scope: BatchScope,
        skipped_off: usize,
        skipped_empty: usize,
    ) {
        let lang = self.config.lang;
        // Per-mode order: parallel/sequential use canvas order, chain uses topological order.
        let ordered = match self.config.mode {
            RunMode::Parallel | RunMode::Sequential => targets.clone(),
            RunMode::Chain => topo_order(&self.nodes, &targets),
        };
        self.batch = BatchState::Running {
            queue: ordered.clone(),
            done: Vec::new(),
            failed: Vec::new(),
            scope,
        };
        let mut started = 0;
        if self.config.mode == RunMode::Parallel {
            // Parallel: runs everything at once. Counts only jobs that actually launched.
            for id in &ordered {
                self.spawn_generate(*id);
                if self.pending.contains_key(id) {
                    started += 1;
                }
            }
        } else {
            // Sequential/chain: starts the first ready one, then one more per completion.
            for id in &ordered {
                if self.cascade_ready(*id) {
                    self.spawn_generate(*id);
                    if self.pending.contains_key(id) {
                        started += 1;
                    }
                    break;
                }
            }
        }
        // Finishes immediately instead of stalling when nothing launched (e.g. cyclic links).
        self.finish_batch_if_idle();
        let mode_label = self.config.mode.label(lang);
        let mut msg = t(lang, "{mode_label} {started}개 실행 중 (OFF {skipped_off}개·빈 프롬프트 {skipped_empty}개 제외)")
            .replace("{mode_label}", mode_label)
            .replace("{started}", &started.to_string())
            .replace("{skipped_off}", &skipped_off.to_string())
            .replace("{skipped_empty}", &skipped_empty.to_string());
        if scope == BatchScope::Selected {
            msg = format!("{}: {msg}", t(lang, "선택 실행"));
        }
        self.status = msg;
    }

    /// Checks whether a node may run now within the batch.
    /// Proceeds when every input is either outside the batch or already finished (success or failure alike).
    /// Nodes waiting on a failed input are skipped and recorded as failed.
    pub(crate) fn cascade_ready(&self, id: usize) -> bool {
        let _lang = self.config.lang;
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return false;
        };
        if node.inputs.is_empty() {
            return true;
        }
        node.inputs.iter().all(|src| {
            if !self.batch.queue().contains(src) {
                return true;
            }
            // Mutual (bidirectional) edges ignore ordering: they run in canvas order.
            // The remaining waits form a DAG, so no deadlock occurs.
            if self
                .nodes
                .iter()
                .any(|n| n.id == *src && n.inputs.contains(&id))
            {
                return true;
            }
            match &self.batch {
                BatchState::Idle => true,
                BatchState::Running { done, .. } => done.contains(src),
            }
        })
    }

    /// Advances the batch one step: records the finished node in done and runs waiters.
    /// With `ok=false` also records it in failed, and waiters fed by a failed input
    /// are skipped as failed without running.
    pub(crate) fn cascade_advance(&mut self, finished: usize, ok: bool) {
        let lang = self.config.lang;
        let batch = match &mut self.batch {
            BatchState::Idle => return,
            BatchState::Running { .. } => &mut self.batch,
        };
        if let BatchState::Running { done, failed, .. } = batch {
            if !done.contains(&finished) {
                done.push(finished);
            }
            if !ok && !failed.contains(&finished) {
                failed.push(finished);
            }
        }
        // Skips waiters fed by a failed input and records them as failed.
        let failed_inputs: Vec<usize> = match &self.batch {
            BatchState::Idle => Vec::new(),
            BatchState::Running { queue, failed, .. } => queue
                .iter()
                .copied()
                .filter(|id| {
                    !self.pending.contains_key(id)
                        && self
                            .nodes
                            .iter()
                            .find(|n| n.id == *id)
                            .is_some_and(|n| n.inputs.iter().any(|src| failed.contains(src)))
                })
                .collect(),
        };
        if let BatchState::Running { done, failed, .. } = &mut self.batch {
            for id in failed_inputs {
                if !done.contains(&id) {
                    done.push(id);
                }
                if !failed.contains(&id) {
                    failed.push(id);
                }
                if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                    Self::stash_output(node);
                    node.output = t(lang, "[건너뜀] 연결된 이전 노드가 실패했습니다.").to_string();
                    self.status =
                        t(lang, "노드 '{}' 건너뜀 (이전 실패).").replace("{}", &node.title);
                }
            }
        }
        // Parallel mode continues with all; sequential/chain continue one at a time.
        let limit = match self.config.mode {
            RunMode::Parallel => usize::MAX,
            RunMode::Sequential | RunMode::Chain => 1,
        };
        let next: Vec<usize> = self
            .batch
            .queue()
            .iter()
            .copied()
            .filter(|id| {
                let done = match &self.batch {
                    BatchState::Idle => return false,
                    BatchState::Running { done, .. } => done.contains(id),
                };
                !done && !self.pending.contains_key(id) && self.cascade_ready(*id)
            })
            .take(limit)
            .collect();
        for id in next {
            self.spawn_generate(id);
        }
        self.finish_batch_if_idle();
    }

    /// Finishes the batch when empty. Called whenever a job leaves.
    /// Also called from poll so the batch never stalls when a tool job finishes last.
    pub(crate) fn finish_batch_if_idle(&mut self) {
        let lang = self.config.lang;
        if !matches!(self.batch, BatchState::Running { .. }) {
            return;
        }
        if self.pending.is_empty() {
            let failed = self.batch.failed_count();
            let scope = self.batch.scope();
            self.batch = BatchState::Idle;
            let (ok_key, fail_key) = match scope {
                BatchScope::All => (
                    "전체 실행 완료. 전체 결과 버튼으로 모아보세요.",
                    "전체 실행 완료 (실패 {failed}개). 전체 결과 버튼으로 모아보세요.",
                ),
                BatchScope::Selected => (
                    "선택 실행 완료. 전체 결과 버튼으로 모아보세요.",
                    "선택 실행 완료 (실패 {failed}개). 전체 결과 버튼으로 모아보세요.",
                ),
            };
            self.status = if failed > 0 {
                t(lang, fail_key).replace("{failed}", &failed.to_string())
            } else {
                t(lang, ok_key).to_string()
            };
            // Batch completion alarm: summary + the head of the combined results.
            let body = crate::alarm::cap_text(&combined_results(&self.nodes), 1200);
            self.launch_alarm(format!("{}\n\n{}", self.status, body));
        }
    }

    pub(crate) fn poll_jobs(&mut self) {
        let lang = self.config.lang;
        // Flushes agent progress first. Fixes the order so the completion message comes later
        // and is never overwritten by a stale progress message.
        let mut latest: Option<String> = None;
        for job in self.pending.values() {
            if let Some(prx) = &job.progress {
                while let Ok(msg) = prx.try_recv() {
                    latest = Some(msg);
                }
            }
        }
        if let Some(msg) = latest {
            self.status = msg;
        }
        let ids: Vec<usize> = self.pending.keys().copied().collect();
        for id in ids {
            let done = self.poll_one(id);
            if done {
                self.pending.remove(&id);
            }
        }
        // Finishes so the batch never stalls when a tool job finishes last.
        self.finish_batch_if_idle();
        // Also polls the connection test job.
        if let Some(job) = self.test_job.take() {
            match job.rx.try_recv() {
                Ok(JobResult::Text(_)) => {
                    self.test_status = t(lang, "연결 성공: 모델이 응답했습니다.").to_string();
                }
                Ok(JobResult::Error(e)) => {
                    self.test_status = t(lang, "연결 실패: {e}").replace("{e}", &e.display(lang));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if job.started.elapsed() > job.timeout {
                        self.test_status = t(lang, "연결 실패: 시간 초과").to_string();
                    } else {
                        self.test_job = Some(job);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.test_status = t(lang, "연결 실패: 작업이 중단됨").to_string();
                }
            }
        }
        // Also polls the alarm delivery job.
        if let Some(job) = self.alarm_job.take() {
            match job.rx.try_recv() {
                Ok(JobResult::Text(sent)) => {
                    self.alarm_status = t(lang, "알람 전송됨: {sent}").replace("{sent}", &sent);
                }
                Ok(JobResult::Error(e)) => {
                    self.alarm_status = t(lang, "알람 실패: {e}").replace("{e}", &e.display(lang));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if job.started.elapsed() > job.timeout {
                        self.alarm_status = t(lang, "알람 실패: 시간 초과").to_string();
                    } else {
                        self.alarm_job = Some(job);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.alarm_status = t(lang, "알람 실패: 작업이 중단됨").to_string();
                }
            }
        }
        // Also polls the blueprint planning job.
        if let Some(job) = self.blueprint_job.take() {
            match job.rx.try_recv() {
                Ok(JobResult::Text(text)) => match crate::blueprint::parse_plan(&text) {
                    Ok(plan) => {
                        let n = self.apply_blueprint(plan);
                        self.blueprint_status =
                            t(lang, "{n}개 노드를 만들었습니다.").replace("{n}", &n.to_string());
                        self.status = self.blueprint_status.clone();
                    }
                    Err(e) => {
                        self.blueprint_status =
                            t(lang, "블루프린트 해석 실패: {e}").replace("{e}", &e);
                    }
                },
                Ok(JobResult::Error(e)) => {
                    self.blueprint_status =
                        t(lang, "블루프린트 실패: {e}").replace("{e}", &e.display(lang));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if job.started.elapsed() > job.timeout {
                        self.blueprint_status =
                            t(lang, "블루프린트 시간 초과. 다시 시도하세요.").to_string();
                    } else {
                        self.blueprint_job = Some(job);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.blueprint_status = t(lang, "블루프린트 작업이 중단됨").to_string();
                }
            }
        }
    }

    /// Stashes one generation of output before overwriting and stamps the completion time.
    pub(crate) fn stash_output(node: &mut PromptNode) {
        if !node.output.is_empty() {
            node.prev_output = Some(std::mem::take(&mut node.output));
        }
        node.last_done = Some(crate::state::unix_now());
    }

    /// Polls one job. Returns true when finished.
    pub(crate) fn poll_one(&mut self, id: usize) -> bool {
        let lang = self.config.lang;
        // Fast path: this runs every frame, so it only borrows without cloning.
        // Clones kind only when actual handling (completion/timeout) is needed.
        let (timed_out, elapsed) = match self.pending.get(&id) {
            None => return true,
            Some(job) => {
                let span = job.started.elapsed();
                (span > job.timeout, span.as_secs_f32())
            }
        };
        let result = match self.pending.get(&id) {
            None => return true,
            Some(job) => match job.rx.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Empty) if !timed_out => return false,
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(JobResult::Error(JobError::Other(
                    t(lang, "작업 중단됨").to_string(),
                ))),
            },
        };
        // Actual handling starts here: drops the borrow and clones only kind.
        let Some(kind) = self.pending.get(&id).map(|job| job.kind.clone()) else {
            return true;
        };
        if timed_out {
            match kind {
                JobKind::Generate | JobKind::Media { .. } | JobKind::Exec => {
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                        Self::stash_output(node);
                        node.output =
                            t(lang, "[오류] {e}").replace("{e}", &JobError::Timeout.display(lang));
                        node.last_secs = Some(elapsed);
                    }
                }
                JobKind::Tool { .. } => {
                    self.status = JobError::Timeout.display(lang);
                }
                JobKind::ConnTest | JobKind::Blueprint => {}
            }
            self.cascade_advance(id, false);
            return true;
        }
        // A timeout already returned true above, so a result must exist here.
        let Some(result) = result else {
            return true;
        };
        match (kind, result) {
            (JobKind::Generate | JobKind::Exec, JobResult::Text(text)) => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                    Self::stash_output(node);
                    node.output = text;
                    node.record_run();
                    node.last_secs = Some(elapsed);
                    self.status = t(lang, "노드 '{}' 실행 완료.").replace("{}", &node.title);
                    self.mark_dirty();
                }
                self.cascade_advance(id, true);
            }
            (JobKind::Media { media }, JobResult::Text(path)) => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                    Self::stash_output(node);
                    match media {
                        MediaKind::Image => {
                            node.image_path = Some(path.clone());
                            node.output =
                                t(lang, "[이미지 저장됨] {path}").replace("{path}", &path);
                        }
                        MediaKind::Audio => {
                            node.audio_path = Some(path.clone());
                            node.output =
                                t(lang, "[오디오 저장됨] {path}").replace("{path}", &path);
                        }
                    }
                    node.record_run();
                    node.last_secs = Some(elapsed);
                    self.status = t(lang, "노드 '{}' 실행 완료.").replace("{}", &node.title);
                    self.mark_dirty();
                }
                self.cascade_advance(id, true);
            }
            (JobKind::Generate | JobKind::Media { .. } | JobKind::Exec, JobResult::Error(e)) => {
                let retry_hint = if e.is_retryable() {
                    t(lang, " 다시 실행해 보세요.").to_string()
                } else {
                    String::new()
                };
                if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                    Self::stash_output(node);
                    node.output = t(lang, "[오류] {e}{hint}")
                        .replace("{e}", &e.display(lang))
                        .replace("{hint}", &retry_hint);
                    node.last_secs = Some(elapsed);
                    self.status = t(lang, "노드 '{}' 실행 실패.").replace("{}", &node.title);
                    self.mark_dirty();
                }
                self.cascade_advance(id, false);
            }
            (JobKind::Tool { server, tool }, JobResult::Text(text)) => {
                if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                    node.context
                        .push(ContextBlock::new(format!("mcp:{server}/{tool}"), text));
                    self.status = t(lang, "도구 '{tool}' 결과를 컨텍스트에 추가했습니다.")
                        .replace("{tool}", &tool);
                    self.mark_dirty();
                }
            }
            (JobKind::Tool { tool, .. }, JobResult::Error(e)) => {
                self.status = t(lang, "도구 '{tool}' 호출 실패: {e}")
                    .replace("{tool}", &tool)
                    .replace("{e}", &e.display(lang));
            }
            (JobKind::ConnTest | JobKind::Blueprint, _) => {}
        }
        true
    }

    // ── MCP ─────────────────────────────────────────────────

    pub(crate) fn mcp_timeout(&self) -> Duration {
        let _lang = self.config.lang;
        Duration::from_secs(30)
    }

    pub(crate) fn mcp_connect(&mut self, name: &str) {
        let lang = self.config.lang;
        let Some(cfg) = self.servers.iter().find(|s| s.name == name).cloned() else {
            return;
        };
        match self.mcp.connect(&cfg, self.mcp_timeout()) {
            Ok(()) => {
                self.mcp_status = t(lang, "MCP '{name}' 연결됨.").replace("{name}", name);
                self.mcp_errors.remove(name);
                self.refresh_tools();
            }
            Err(e) => {
                let msg = t(lang, "MCP '{name}' 연결 실패: {e}")
                    .replace("{name}", name)
                    .replace("{e}", &e.to_string());
                self.mcp_status = msg.clone();
                self.mcp_errors.insert(name.to_string(), msg);
            }
        }
    }

    pub(crate) fn refresh_tools(&mut self) {
        let lang = self.config.lang;
        let mut all = Vec::new();
        for name in self.mcp.connected_names() {
            // Briefly borrows a connected client to fetch the tool list.
            if let Some(client) = self.mcp.get(&name) {
                match client.list_tools(self.mcp_timeout()) {
                    Ok(tools) => all.extend(tools),
                    Err(e) => {
                        self.mcp_status = t(lang, "도구 목록 실패({name}): {e}")
                            .replace("{name}", name.as_str())
                            .replace("{e}", &e.to_string());
                    }
                }
            }
        }
        all.sort_by(|a, b| (&a.server, &a.name).cmp(&(&b.server, &b.name)));
        self.mcp_tools = all;
    }

    /// Starts a tool call and returns success (no per-language string branching).
    pub(crate) fn spawn_tool_call(
        &mut self,
        node_id: usize,
        server: String,
        tool: String,
        args: String,
    ) -> bool {
        let lang = self.config.lang;
        if self.pending.contains_key(&node_id) {
            self.status = t(lang, "이미 실행 중인 작업이 있습니다.").to_string();
            return false;
        }
        let parsed: serde_json::Value = match serde_json::from_str::<serde_json::Value>(&args) {
            Ok(v) if v.is_object() => v,
            _ => {
                self.status = t(lang, "도구 인자는 JSON 객체여야 합니다. 예: {}").to_string();
                return false;
            }
        };
        let Some(client) = self.mcp.get(&server) else {
            self.status =
                t(lang, "MCP '{server}'에 연결되어 있지 않습니다.").replace("{server}", &server);
            return false;
        };
        let timeout = self.mcp_timeout();
        let tool_for_thread = tool.clone();
        let rx = spawn_job(
            format!("job-tool-{server}/{tool_for_thread}"),
            move || match client.call_tool(&tool_for_thread, parsed, timeout) {
                Ok(text) => JobResult::Text(text),
                Err(e) => JobResult::Error(JobError::from(e)),
            },
        );
        self.pending.insert(
            node_id,
            PendingJob {
                rx,
                progress: None,
                started: Instant::now(),
                timeout,
                kind: JobKind::Tool { server, tool },
            },
        );
        self.status = t(lang, "도구 호출 중…").to_string();
        true
    }

    pub(crate) fn spawn_conn_test(&mut self) {
        let lang = self.config.lang;
        if self.test_job.is_some() {
            return;
        }
        let client = self.build_client();
        let req = LlmRequest::new(
            "Reply with exactly: OK".to_string(),
            vec![],
            self.config.model.clone(),
        );
        let rx = spawn_job("job-conn-test".to_string(), move || {
            match client.generate(&req) {
                Ok(text) => JobResult::Text(text),
                Err(e) => JobResult::Error(JobError::from(e)),
            }
        });
        self.test_job = Some(PendingJob {
            rx,
            progress: None,
            started: Instant::now(),
            timeout: self.timeout(),
            kind: JobKind::ConnTest,
        });
        self.test_status = t(lang, "연결 테스트 중…").to_string();
    }

    /// Sends a blueprint planning request to the background.
    /// Mock cannot plan, so this only starts with the Meta API.
    pub(crate) fn spawn_blueprint(&mut self) {
        let lang = self.config.lang;
        if self.blueprint_job.is_some() {
            self.blueprint_status = t(lang, "이미 생성 중입니다.").to_string();
            return;
        }
        if self.config.provider == Provider::Mock {
            self.blueprint_status = t(
                lang,
                "Mock에서는 블루프린트를 만들 수 없습니다. Meta API를 선택하세요.",
            )
            .to_string();
            return;
        }
        let desc = self.blueprint_text.trim().to_string();
        if desc.is_empty() {
            self.blueprint_status = t(lang, "블루프린트 설명을 입력하세요.").to_string();
            return;
        }
        let client = self.build_client();
        let req = LlmRequest::new(desc, vec![], self.config.model.clone()).with_options(
            Some(0.2),
            Some(8000),
            crate::blueprint::plan_system_prompt(),
        );
        let rx = spawn_job("job-blueprint".to_string(), move || {
            match client.generate(&req) {
                Ok(text) => JobResult::Text(text),
                Err(e) => JobResult::Error(JobError::from(e)),
            }
        });
        self.blueprint_job = Some(PendingJob {
            rx,
            progress: None,
            started: Instant::now(),
            timeout: self.timeout(),
            kind: JobKind::Blueprint,
        });
        self.blueprint_status = t(lang, "블루프린트 생성 중…").to_string();
    }

    /// Lays a validated plan onto the canvas. Returns the number of nodes created.
    /// Tiles around the current view center, pushing overlaps to free spots.
    pub(crate) fn apply_blueprint(&mut self, plan: crate::blueprint::BlueprintPlan) -> usize {
        let _lang = self.config.lang;
        if plan.nodes.is_empty() {
            return 0;
        }
        self.push_undo();
        let z = self.zoom.max(0.001);
        let origin = match self.central_rect {
            Some(r) => [
                (r[0] + r[2]) / 2.0 / z - self.canvas[0] / z - 160.0,
                (r[1] + r[3]) / 2.0 / z - self.canvas[1] / z - 200.0,
            ],
            None => [0.0, 0.0],
        };
        let spots = tile_positions(plan.nodes.len(), origin);
        let mut taken: Vec<[f32; 4]> = self
            .nodes
            .iter()
            .map(|n| [n.pos[0], n.pos[1], n.pos[0] + 330.0, n.pos[1] + 470.0])
            .collect();
        let mut ids = Vec::with_capacity(plan.nodes.len());
        for (i, spec) in plan.nodes.iter().enumerate() {
            let id = self.next_id;
            self.next_id += 1;
            let want = spots.get(i).copied().unwrap_or(origin);
            let pos = find_free_spot(want, [330.0, 470.0], &taken, 16.0);
            taken.push([pos[0], pos[1], pos[0] + 330.0, pos[1] + 470.0]);
            let mut node = PromptNode::new(id, spec.title.clone(), spec.prompt.clone(), true, pos);
            node.mode = spec.mode;
            node.system = spec.system.clone();
            node.model = spec.model.clone();
            node.temperature = spec.temperature;
            node.max_tokens = spec.max_tokens;
            self.nodes.push(node);
            ids.push(id);
        }
        // Input links: converts plan indexes to real ids. Only earlier nodes are referenced, so no cycles.
        for (i, spec) in plan.nodes.iter().enumerate() {
            let dst = ids[i];
            for &src_idx in &spec.inputs {
                let src = ids[src_idx];
                if let Some(node) = self.nodes.iter_mut().find(|n| n.id == dst) {
                    node.add_input(src);
                }
            }
        }
        self.selected = ids.iter().copied().collect();
        self.mark_dirty();
        ids.len()
    }

    /// Sends the result alarm in the background. Silently skips when no channel is ready.
    pub(crate) fn launch_alarm(&mut self, text: String) {
        let lang = self.config.lang;
        if self.alarm_job.is_some() {
            return;
        }
        let cfg = self.config.alarm.clone();
        if !cfg.any_ready() {
            return;
        }
        let rx = spawn_job(
            "job-alarm".to_string(),
            move || match crate::alarm::send_all(&cfg, &text) {
                Ok(sent) => JobResult::Text(sent),
                Err(e) => JobResult::Error(JobError::Other(e)),
            },
        );
        self.alarm_job = Some(PendingJob {
            rx,
            progress: None,
            started: Instant::now(),
            timeout: Duration::from_secs(90),
            kind: JobKind::ConnTest,
        });
        self.alarm_status = t(lang, "알람 전송 중…").to_string();
    }

    /// For the settings test button. Records the reason in status when no channel is ready.
    pub(crate) fn send_alarm_test(&mut self) {
        let lang = self.config.lang;
        if !self.config.alarm.any_ready() {
            self.alarm_status = t(
                lang,
                "켜진 알람 채널이 없습니다. 채널을 켜고 값을 입력하세요.",
            )
            .to_string();
            return;
        }
        self.launch_alarm(t(lang, "Symphony Canvas 테스트 알람입니다.").to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_job_delivers_result() {
        let rx = spawn_job("test-job".to_string(), || JobResult::Text("hi".to_string()));
        let got = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(matches!(got, JobResult::Text(s) if s == "hi"));
    }
}
