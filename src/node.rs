//! Prompt node data. Pure logic only, with no egui dependency.

use crate::llm::ContextBlock;
use crate::schedule::NodeSched;

/// One prompt node on the canvas. `pos` is in canvas coordinates (persisted).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PromptNode {
    pub id: usize,
    pub title: String,
    pub prompt: String,
    pub output: String,
    pub enabled: bool,
    pub run_count: u32,
    pub pos: [f32; 2],
    #[serde(default)]
    pub context: Vec<ContextBlock>,
    /// Source node ids feeding this node's input. Used for multi-link connections.
    #[serde(default)]
    pub inputs: Vec<usize>,
    /// Legacy single input from old saves. Migrated into `inputs` on load, never saved.
    #[serde(default, rename = "input", skip_serializing)]
    pub input_legacy: Option<usize>,
    /// Per-node model override. Falls back to the global model when None or empty.
    #[serde(default)]
    pub model: Option<String>,
    /// Detail options: uses defaults when None.
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub system: String,
    /// Appearance settings.
    #[serde(default)]
    pub accent: NodeAccent,
    #[serde(default)]
    pub radius: NodeRadius,
    /// Run mode plus agent max steps.
    #[serde(default)]
    pub mode: NodeMode,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// Show title only (collapsed).
    #[serde(default)]
    pub title_only: bool,
    /// Representative image file path. None when absent.
    #[serde(default)]
    pub image_path: Option<String>,
    /// Generated audio file path. None when absent.
    #[serde(default)]
    pub audio_path: Option<String>,
    /// Last run duration in seconds. None before the first run.
    #[serde(default)]
    pub last_secs: Option<f32>,
    /// Keeps one generation of pre-overwrite output.
    #[serde(default)]
    pub prev_output: Option<String>,
    /// Last completion time (unix seconds).
    #[serde(default)]
    pub last_done: Option<u64>,
    /// Card width scale (bottom-handle resize, min 0.05, no max).
    #[serde(default = "default_width_scale")]
    pub width_scale: f32,
    /// Card height scale (bottom-handle vertical drag, min 0.3, no max).
    #[serde(default = "default_height_scale")]
    pub height_scale: f32,
    /// Per-node scheduled runs. Does nothing when disabled.
    #[serde(default)]
    pub sched: NodeSched,
    /// Previous frame's screen rect. Memory-only, for drawing links.
    #[serde(skip)]
    pub last_rect: Option<[f32; 4]>,
}

fn default_max_steps() -> u32 {
    5
}

fn default_width_scale() -> f32 {
    1.0
}

fn default_height_scale() -> f32 {
    1.0
}

impl PromptNode {
    pub fn new(id: usize, title: String, prompt: String, enabled: bool, pos: [f32; 2]) -> Self {
        Self {
            id,
            title,
            prompt,
            output: String::new(),
            enabled,
            run_count: 0,
            pos,
            context: Vec::new(),
            inputs: Vec::new(),
            input_legacy: None,
            model: None,
            temperature: None,
            max_tokens: None,
            system: String::new(),
            accent: NodeAccent::default(),
            radius: NodeRadius::default(),
            mode: NodeMode::default(),
            max_steps: default_max_steps(),
            title_only: false,
            image_path: None,
            audio_path: None,
            last_secs: None,
            prev_output: None,
            last_done: None,
            width_scale: default_width_scale(),
            height_scale: default_height_scale(),
            sched: NodeSched::default(),
            last_rect: None,
        }
    }

    /// Clone: takes a new id, resets output and run count. Keeps input links.
    pub fn cloned_with_id(&self, new_id: usize, pos: [f32; 2]) -> Self {
        Self {
            id: new_id,
            title: format!("{} 복사", self.title),
            prompt: self.prompt.clone(),
            output: String::new(),
            enabled: self.enabled,
            run_count: 0,
            pos,
            context: self.context.clone(),
            inputs: self.inputs.clone(),
            input_legacy: None,
            model: self.model.clone(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            system: self.system.clone(),
            accent: self.accent,
            radius: self.radius,
            mode: self.mode,
            max_steps: self.max_steps,
            title_only: self.title_only,
            image_path: self.image_path.clone(),
            audio_path: self.audio_path.clone(),
            last_secs: None,
            prev_output: None,
            last_done: None,
            width_scale: self.width_scale.max(0.05),
            height_scale: self.height_scale.max(0.3),
            // Start with the schedule off so the copy does not fire on the original's schedule.
            sched: NodeSched::default(),
            last_rect: None,
        }
    }

    pub fn record_run(&mut self) {
        self.run_count = self.run_count.saturating_add(1);
    }

    /// Links one input source. Returns false on self-reference or duplicate.
    pub fn add_input(&mut self, src: usize) -> bool {
        if src == self.id || self.inputs.contains(&src) {
            return false;
        }
        self.inputs.push(src);
        true
    }

    /// Unlinks an input source. Returns true if it existed.
    pub fn remove_input(&mut self, src: usize) -> bool {
        let before = self.inputs.len();
        self.inputs.retain(|c| *c != src);
        self.inputs.len() != before
    }

    /// Unlinks all input sources. Returns true if any were removed.
    pub fn clear_inputs(&mut self) -> bool {
        if self.inputs.is_empty() {
            return false;
        }
        self.inputs.clear();
        true
    }

    /// Model name for the actual call. Falls back to global when the node override is empty.
    pub fn effective_model(&self, global: &str) -> String {
        match self.model.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => global.to_string(),
        }
    }
}

/// For click-to-complete: finds the nearest node input port in screen coordinates.
/// Excludes itself. rect is a screen-space [x0, y0, x1, y1].
pub fn input_port_target(nodes: &[PromptNode], src_id: usize, p: [f32; 2]) -> Option<usize> {
    let rad = PORT_HIT + 6.0;
    nodes
        .iter()
        .filter(|n| n.id != src_id)
        .filter_map(|n| {
            n.last_rect.map(|r| {
                let ip = in_port(r);
                let dx = p[0] - ip[0];
                let dy = p[1] - ip[1];
                (n.id, dx * dx + dy * dy)
            })
        })
        .filter(|(_, d2)| *d2 < rad * rad)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(id, _)| id)
}

/// For L-key batch linking: builds neighbor pairs in ascending id order.
pub fn chain_pairs(mut ids: Vec<usize>) -> Vec<(usize, usize)> {
    ids.sort_unstable();
    ids.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Node accent color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeAccent {
    #[default]
    Blue,
    Green,
    Orange,
    Gray,
    /// User-picked custom color.
    Custom([u8; 3]),
}

impl NodeAccent {
    pub fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Blue => (0, 100, 224),
            Self::Green => (14, 148, 96),
            Self::Orange => (212, 108, 0),
            Self::Gray => (110, 118, 128),
            Self::Custom([r, g, b]) => (*r, *g, *b),
        }
    }

    pub fn label(&self, lang: crate::i18n::Lang) -> &'static str {
        use crate::i18n::t;
        match self {
            Self::Blue => t(lang, "파랑"),
            Self::Green => t(lang, "초록"),
            Self::Orange => t(lang, "주황"),
            Self::Gray => t(lang, "회색"),
            Self::Custom(_) => t(lang, "직접"),
        }
    }
}

/// Node corner radius.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRadius {
    #[default]
    Round,
    Soft,
    Sharp,
}

impl NodeRadius {
    pub fn px(&self) -> u8 {
        match self {
            Self::Round => 12,
            Self::Soft => 6,
            Self::Sharp => 0,
        }
    }

    pub fn label(&self, lang: crate::i18n::Lang) -> &'static str {
        use crate::i18n::t;
        match self {
            Self::Round => t(lang, "둥글게"),
            Self::Soft => t(lang, "약간"),
            Self::Sharp => t(lang, "각지게"),
        }
    }
}

/// Node run mode. Agent mode hops across MCP tools on its own to build an answer.
/// Image/audio modes generate media from the prompt and attach it to the result,
/// and the rest are instruction presets prepended to the prompt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeMode {
    #[default]
    Normal,
    Agent,
    Image,
    Audio,
    Python,
    Summarize,
    TranslateEn,
    TranslateKo,
    Polish,
    Bullets,
    Table,
    Expand,
    Critique,
    Questions,
    ExplainCode,
    WriteCode,
    Email,
    Sns,
    Title,
    Keywords,
    Plan,
    ProsCons,
    Eli5,
}

impl NodeMode {
    /// All 23 modes, in combobox order.
    pub const ALL: [NodeMode; 23] = [
        NodeMode::Normal,
        NodeMode::Agent,
        NodeMode::Image,
        NodeMode::Audio,
        NodeMode::Python,
        NodeMode::Summarize,
        NodeMode::TranslateEn,
        NodeMode::TranslateKo,
        NodeMode::Polish,
        NodeMode::Bullets,
        NodeMode::Table,
        NodeMode::Expand,
        NodeMode::Critique,
        NodeMode::Questions,
        NodeMode::ExplainCode,
        NodeMode::WriteCode,
        NodeMode::Email,
        NodeMode::Sns,
        NodeMode::Title,
        NodeMode::Keywords,
        NodeMode::Plan,
        NodeMode::ProsCons,
        NodeMode::Eli5,
    ];

    /// Canonical Korean label. Wrap with t() for display.
    pub fn label(&self) -> &'static str {
        match self {
            NodeMode::Normal => "일반",
            NodeMode::Agent => "에이전트",
            NodeMode::Image => "이미지",
            NodeMode::Audio => "오디오",
            NodeMode::Python => "파이썬",
            NodeMode::Summarize => "요약",
            NodeMode::TranslateEn => "영어로 번역",
            NodeMode::TranslateKo => "한국어로 번역",
            NodeMode::Polish => "문장 다듬기",
            NodeMode::Bullets => "불릿 정리",
            NodeMode::Table => "표로 정리",
            NodeMode::Expand => "아이디어 확장",
            NodeMode::Critique => "반박·비판",
            NodeMode::Questions => "질문 만들기",
            NodeMode::ExplainCode => "코드 설명",
            NodeMode::WriteCode => "코드 작성",
            NodeMode::Email => "이메일 작성",
            NodeMode::Sns => "SNS 문구",
            NodeMode::Title => "제목 짓기",
            NodeMode::Keywords => "키워드 추출",
            NodeMode::Plan => "실행 계획",
            NodeMode::ProsCons => "장단점 분석",
            NodeMode::Eli5 => "쉽게 설명",
        }
    }

    /// Prompt input label key. Python mode labels it as code input.
    pub fn prompt_label_key(&self) -> &'static str {
        match self {
            NodeMode::Python => "파이썬 코드:",
            _ => "프롬프트 입력:",
        }
    }

    /// Prompt input hint key.
    pub fn prompt_hint_key(&self) -> &'static str {
        match self {
            NodeMode::Python => "파이썬 코드를 입력하세요…",
            _ => "여기에 프롬프트를 입력하세요…",
        }
    }

    /// Instruction prepended to the prompt. A handwritten system message wins over it.
    pub fn system_preset(&self) -> Option<&'static str> {
        match self {
            NodeMode::Summarize => Some("다음 글을 핵심만 3줄로 요약하세요."),
            NodeMode::TranslateEn => {
                Some("다음 글을 자연스러운 영어로 번역하세요. 번역문만 출력하세요.")
            }
            NodeMode::TranslateKo => {
                Some("다음 글을 자연스러운 한국어로 번역하세요. 번역문만 출력하세요.")
            }
            NodeMode::Polish => Some("다음 글의 맞춤법과 문장을 다듬으세요. 의미는 바꾸지 마세요."),
            NodeMode::Bullets => Some("다음 내용을 불릿 5개로 정리하세요."),
            NodeMode::Table => Some("다음 내용을 마크다운 표로 정리하세요."),
            NodeMode::Expand => Some("다음 아이디어를 10가지로 변형·확장하세요."),
            NodeMode::Critique => Some("다음 주장의 약점 3가지와 개선안을 말하세요."),
            NodeMode::Questions => Some("다음 내용을 이해했는지 확인할 질문 5개를 만드세요."),
            NodeMode::ExplainCode => Some("다음 코드를 한 줄씩 쉽게 설명하세요."),
            NodeMode::WriteCode => {
                Some("다음 요구사항을 만족하는 코드를 작성하세요. 코드와 짧은 설명만 출력하세요.")
            }
            NodeMode::Email => {
                Some("다음 내용을 정중한 이메일로 작성하세요. 제목과 본문을 출력하세요.")
            }
            NodeMode::Sns => Some("다음 내용을 인스타그램·스레드용 짧은 문구 3개로 만드세요."),
            NodeMode::Title => Some("다음 글의 제목을 5개 지으세요."),
            NodeMode::Keywords => Some("다음 글의 핵심 키워드 10개를 쉼표로 나열하세요."),
            NodeMode::Plan => Some("다음 목표의 단계별 실행 계획을 세우세요."),
            NodeMode::ProsCons => Some("다음 안의 장점과 단점을 각각 3개씩 말하세요."),
            NodeMode::Eli5 => Some("다음 내용을 중학생도 이해하게 쉽게 설명하세요."),
            _ => None,
        }
    }
}

/// Tests whether two rects overlap. Inflates by margin so gaps count as overlap.
pub fn rects_overlap(a: [f32; 4], b: [f32; 4], margin: f32) -> bool {
    let m = margin.max(0.0);
    a[0] < b[2] + m && b[0] < a[2] + m && a[1] < b[3] + m && b[1] < a[3] + m
}

/// Pushes `cur` out of `hit` by the minimum shift plus margin.
fn push_out_rect(cur: [f32; 4], hit: [f32; 4], margin: f32) -> [f32; 4] {
    let m = margin.max(0.0);
    let push_left = cur[2] + m - hit[0];
    let push_right = hit[2] + m - cur[0];
    let push_up = cur[3] + m - hit[1];
    let push_down = hit[3] + m - cur[1];
    let w = cur[2] - cur[0];
    let h = cur[3] - cur[1];
    let min = push_left.min(push_right).min(push_up).min(push_down);
    if min == push_left {
        [hit[0] - m - w, cur[1], hit[0] - m, cur[3]]
    } else if min == push_right {
        [hit[2] + m, cur[1], hit[2] + m + w, cur[3]]
    } else if min == push_up {
        [cur[0], hit[1] - m - h, cur[2], hit[1] - m]
    } else {
        [cur[0], hit[3] + m, cur[2], hit[3] + m + h]
    }
}

/// Among neighbors overlapping the placed card (`fixed`), pushes aside by minimum
/// shift only those that did not overlap at gesture start. Leaves existing piles
/// (`pre_overlapped`) untouched. Returns only changed (id, pos); ignores sub-0.5px slivers.
pub fn declump(
    rects: &[(usize, [f32; 4])],
    fixed: &[usize],
    pre_overlapped: &[usize],
    margin: f32,
) -> Vec<(usize, [f32; 2])> {
    let fixed_rects: Vec<[f32; 4]> = rects
        .iter()
        .filter(|(id, _)| fixed.contains(id))
        .map(|(_, r)| *r)
        .collect();
    let mut out = Vec::new();
    for (id, r) in rects {
        if fixed.contains(id) || pre_overlapped.contains(id) {
            continue;
        }
        let mut cur = *r;
        for _ in 0..10 {
            let Some(hit) = fixed_rects.iter().find(|p| rects_overlap(cur, **p, margin)) else {
                break;
            };
            cur = push_out_rect(cur, *hit, margin);
        }
        if (cur[0] - r[0]).abs() > 0.5 || (cur[1] - r[1]).abs() > 0.5 {
            out.push((*id, [cur[0], cur[1]]));
        }
    }
    out
}

/// Searches outward in a spiral from `want` for a free spot. Returns `want` if none.
pub fn find_free_spot(want: [f32; 2], size: [f32; 2], taken: &[[f32; 4]], margin: f32) -> [f32; 2] {
    let at = |p: [f32; 2]| [p[0], p[1], p[0] + size[0], p[1] + size[1]];
    if taken.iter().all(|t| !rects_overlap(at(want), *t, margin)) {
        return want;
    }
    for ring in 1..=30 {
        let d = ring as f32 * 40.0;
        let pts = [
            [0.0, -d],
            [d, -d],
            [d, 0.0],
            [d, d],
            [0.0, d],
            [-d, d],
            [-d, 0.0],
            [-d, -d],
        ];
        for [dx, dy] in pts {
            let p = [want[0] + dx, want[1] + dy];
            if taken.iter().all(|t| !rects_overlap(at(p), *t, margin)) {
                return p;
            }
        }
    }
    want
}

/// Snaps the dragged card to neighbor card edges like a magnet.
/// In screen coordinates; picks the nearest alignment within the threshold.
pub fn magnetic_snap(
    origin: [f32; 2],
    size: [f32; 2],
    others: &[[f32; 4]],
    threshold: f32,
) -> [f32; 2] {
    let (w, h) = (size[0], size[1]);
    let mut ox = origin[0];
    let mut oy = origin[1];
    let mut best_dx = threshold;
    let mut best_dy = threshold;
    for r in others {
        // Vertical edges: left-left, left-right, right-left, right-right
        for cand in [r[0], r[2] - w, r[0] - w, r[2]] {
            let d = (ox - cand).abs();
            if d < best_dx {
                best_dx = d;
                ox = cand;
            }
        }
        // Horizontal edges: top-top, top-bottom, bottom-top, bottom-bottom
        for cand in [r[1], r[3] - h, r[1] - h, r[3]] {
            let d = (oy - cand).abs();
            if d < best_dy {
                best_dy = d;
                oy = cand;
            }
        }
    }
    [ox, oy]
}

/// Preserves run artifacts. Undo/redo restores structure only and
/// matches by id so fresh output, counts, and timestamps survive.
pub fn restore_results(current: &[PromptNode], restored: &mut [PromptNode]) {
    for node in restored.iter_mut() {
        if let Some(live) = current.iter().find(|n| n.id == node.id) {
            node.output = live.output.clone();
            node.prev_output = live.prev_output.clone();
            node.run_count = live.run_count;
            node.last_secs = live.last_secs;
            node.last_done = live.last_done;
            // Schedule progress (next due time, run count) is an artifact too, so preserve it.
            node.sched.next_due = live.sched.next_due;
            node.sched.runs_done = live.sched.runs_done;
        }
    }
}

/// Port (dot) centers relative to the card screen rect. Output on the right, input on the left.
pub const PORT_GAP: f32 = 10.0;
/// Port hit radius (px).
pub const PORT_HIT: f32 = 17.0;
/// Side length (px) of the unlink button hit area. Always shown mid-link.
pub const UNLINK_HIT: f32 = 28.0;

pub fn out_port(rect: [f32; 4]) -> [f32; 2] {
    [rect[2] + PORT_GAP, (rect[1] + rect[3]) / 2.0]
}

pub fn in_port(rect: [f32; 4]) -> [f32; 2] {
    [rect[0] - PORT_GAP, (rect[1] + rect[3]) / 2.0]
}

/// Detects whether output text is an error. Checks both Korean and English prefixes.
pub fn is_error_output(s: &str) -> bool {
    s.starts_with("[오류] ") || s.starts_with("[error] ")
}

/// Short title for header display. Truncates over 16 chars to 15 chars + "…".
pub fn short_title(s: &str) -> String {
    if s.chars().count() > 16 {
        format!("{}…", s.chars().take(15).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Clamps node X so it never slides fully under a panel. Y is unlimited.
/// `pos` and `canvas` are in canvas coordinates; `central` and `card_w` in screen coordinates.
/// A card slightly under a panel stays put while its 60px grab area remains visible.
/// A tight clamp would snap the card to the wall on grab, making it unmovable.
pub fn clamp_panel_x(
    pos: [f32; 2],
    card_w: f32,
    central: Option<[f32; 4]>,
    canvas: [f32; 2],
    zoom: f32,
) -> [f32; 2] {
    let Some(r) = central else {
        return pos;
    };
    let zoom = zoom.max(0.001);
    let w = card_w.max(80.0);
    let ox = pos[0] * zoom + canvas[0];
    let (lo, hi) = (r[0] - w + 60.0, r[2] - 60.0);
    let cx = ox.clamp(lo.min(hi), lo.max(hi));
    [(cx - canvas[0]) / zoom, pos[1]]
}

/// Clamps the node top-left inside the map bounds. Canvas coordinates.
/// When the card exceeds the board, leaves only the 60x40 grab area visible.
pub fn clamp_board(pos: [f32; 2], size: [f32; 2], board: [f32; 2]) -> [f32; 2] {
    let (w, h) = (size[0].max(80.0), size[1].max(80.0));
    let (bw, bh) = (board[0].max(400.0), board[1].max(400.0));
    let x = if w <= bw {
        pos[0].clamp(0.0, bw - w)
    } else {
        pos[0].clamp(bw - w, bw - 60.0)
    };
    let y = if h <= bh {
        pos[1].clamp(0.0, bh - h)
    } else {
        pos[1].clamp(bh - h, bh - 40.0)
    };
    [x, y]
}

/// One-sided guard: blocks only exits from inside, leaves outside cards alone.
/// Pulling an outside card inward would look like teleporting.
pub fn block_exit(old: f32, new: f32, lo: f32, hi: f32) -> f32 {
    if old < lo || old > hi {
        new
    } else {
        new.clamp(lo, hi)
    }
}

/// Board guard for moves. Cards inside cannot cross the boundary,
/// while cards outside move freely (entering is not blocked either).
pub fn clamp_board_step(old: [f32; 2], new: [f32; 2], size: [f32; 2], board: [f32; 2]) -> [f32; 2] {
    let (w, h) = (size[0].max(80.0), size[1].max(80.0));
    let (bw, bh) = (board[0].max(400.0), board[1].max(400.0));
    let (xlo, xhi) = if w <= bw {
        (0.0, bw - w)
    } else {
        (bw - w, bw - 60.0)
    };
    let (ylo, yhi) = if h <= bh {
        (0.0, bh - h)
    } else {
        (bh - h, bh - 40.0)
    };
    [
        block_exit(old[0], new[0], xlo, xhi),
        block_exit(old[1], new[1], ylo, yhi),
    ]
}

/// Panel guard for moves. `old`, `new`, and `canvas` are in canvas coordinates;
/// `central` and `card_w` are in screen coordinates. Outside cards stay free.
pub fn clamp_panel_x_step(
    old: [f32; 2],
    new: [f32; 2],
    card_w: f32,
    central: Option<[f32; 4]>,
    canvas: [f32; 2],
    zoom: f32,
) -> [f32; 2] {
    let Some(r) = central else {
        return new;
    };
    let zoom = zoom.max(0.001);
    let w = card_w.max(80.0);
    let oox = old[0] * zoom + canvas[0];
    let onx = new[0] * zoom + canvas[0];
    let (lo, hi) = (r[0] - w + 60.0, r[2] - 60.0);
    let cx = block_exit(oox, onx, lo.min(hi), lo.max(hi));
    [(cx - canvas[0]) / zoom, new[1]]
}

/// Builds diagonally offset placement coordinates so nodes do not overlap.
pub fn cascade_pos(seed: usize) -> [f32; 2] {
    let k = seed as f32;
    [40.0 + (k * 60.0) % 500.0, 60.0 + (k * 40.0) % 300.0]
}

/// Builds grid coordinates for `count` nodes starting at `origin`, like a go board.
/// Pure function, so it is testable.
pub fn tile_positions(count: usize, origin: [f32; 2]) -> Vec<[f32; 2]> {
    const COLS: usize = 3;
    const W: f32 = 330.0;
    const H: f32 = 470.0;
    (0..count)
        .map(|i| {
            let col = (i % COLS) as f32;
            let row = (i / COLS) as f32;
            [origin[0] + col * W, origin[1] + row * H]
        })
        .collect()
}

/// Spawn position for a new node in the current view (`center`: canvas-space center).
/// Shifts by half width so the card lands centered, and offsets by seed to avoid overlap.
pub fn view_spawn_pos(center: [f32; 2], seed: usize) -> [f32; 2] {
    let k = seed as f32;
    let dx = (k * 37.0) % 160.0 - 80.0;
    let dy = (k * 53.0) % 120.0 - 60.0;
    [center[0] - 160.0 + dx, center[1] - 200.0 + dy]
}

/// Selects batch-run target ids only: enabled, not busy, and with a non-empty prompt.
pub fn runnable_ids(nodes: &[PromptNode], busy: &[usize]) -> Vec<usize> {
    nodes
        .iter()
        .filter(|n| n.enabled && !busy.contains(&n.id) && !n.prompt.trim().is_empty())
        .map(|n| n.id)
        .collect()
}

/// For run-selection: picks runnable ids in `wanted`, in canvas order.
/// Follows screen order rather than selection (HashSet) order for stable results.
pub fn runnable_subset(nodes: &[PromptNode], wanted: &[usize], busy: &[usize]) -> Vec<usize> {
    nodes
        .iter()
        .filter(|n| {
            wanted.contains(&n.id)
                && n.enabled
                && !busy.contains(&n.id)
                && !n.prompt.trim().is_empty()
        })
        .map(|n| n.id)
        .collect()
}

/// Builds context blocks from ancestor outputs, oldest first.
/// Walks multi-inputs in depth-first post-order; cuts cycles and missing sources.
pub fn ancestor_context(node: &PromptNode, nodes: &[PromptNode]) -> Vec<ContextBlock> {
    fn visit<'a>(
        id: usize,
        nodes: &'a [PromptNode],
        seen: &mut Vec<usize>,
        out: &mut Vec<&'a PromptNode>,
    ) {
        if seen.contains(&id) || out.len() >= 16 {
            return;
        }
        seen.push(id);
        let Some(src) = nodes.iter().find(|n| n.id == id) else {
            return;
        };
        for inp in &src.inputs.clone() {
            visit(*inp, nodes, seen, out);
        }
        out.push(src);
    }
    let mut seen = vec![node.id];
    let mut chain: Vec<&PromptNode> = Vec::new();
    for inp in node.inputs.clone() {
        visit(inp, nodes, &mut seen, &mut chain);
    }
    chain
        .into_iter()
        .filter(|src| !src.output.trim().is_empty())
        .map(|src| ContextBlock::new(format!("chain:{}", src.title), src.output.clone()))
        .collect()
}

/// Checks whether adding `src` to `dst` inputs would create a cycle.
/// Follows all multi-inputs to block it early so chain runs never stall.
pub fn creates_cycle(nodes: &[PromptNode], dst: usize, src: usize) -> bool {
    if dst == src {
        return true;
    }
    let mut seen = vec![src];
    let mut stack = vec![src];
    while let Some(id) = stack.pop() {
        if id == dst {
            return true;
        }
        let Some(node) = nodes.iter().find(|n| n.id == id) else {
            continue;
        };
        for inp in &node.inputs {
            if !seen.contains(inp) {
                seen.push(*inp);
                stack.push(*inp);
            }
        }
    }
    false
}

/// Checks whether linking src→dst creates only a direct pair (2-cycle).
/// The reverse edge (dst→src) must exist, and src must be unreachable once it is removed.
/// Returns false when a longer cycle forms too.
pub fn is_mutual_only(nodes: &[PromptNode], dst: usize, src: usize) -> bool {
    if dst == src {
        return false;
    }
    let Some(s) = nodes.iter().find(|n| n.id == src) else {
        return false;
    };
    if !s.inputs.contains(&dst) {
        return false;
    }
    // Ignoring the reverse edge, reaching dst from src ancestors means a long cycle.
    let mut seen = vec![src];
    let mut stack: Vec<usize> = s.inputs.iter().filter(|i| **i != dst).copied().collect();
    while let Some(id) = stack.pop() {
        if id == dst {
            return false;
        }
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if let Some(node) = nodes.iter().find(|n| n.id == id) {
            stack.extend(node.inputs.iter().copied());
        }
    }
    true
}

/// Checks whether an src→dst link forms a cycle that must be blocked.
/// Allows direct (bidirectional) pairs; blocks self-links and long cycles.
pub fn blocked_cycle(nodes: &[PromptNode], dst: usize, src: usize) -> bool {
    creates_cycle(nodes, dst, src) && !is_mutual_only(nodes, dst, src)
}

/// Determines run order: topological sort with sources first. Cuts cycles and proceeds.
pub fn topo_order(nodes: &[PromptNode], targets: &[usize]) -> Vec<usize> {
    let mut ordered = Vec::with_capacity(targets.len());
    let mut visiting: Vec<usize> = Vec::new();
    fn visit(
        id: usize,
        nodes: &[PromptNode],
        targets: &[usize],
        ordered: &mut Vec<usize>,
        visiting: &mut Vec<usize>,
    ) {
        if ordered.contains(&id) || visiting.contains(&id) {
            return;
        }
        visiting.push(id);
        if let Some(node) = nodes.iter().find(|n| n.id == id) {
            for src in node.inputs.clone() {
                if targets.contains(&src) {
                    visit(src, nodes, targets, ordered, visiting);
                }
            }
        }
        visiting.retain(|v| *v != id);
        ordered.push(id);
    }
    for id in targets {
        visit(*id, nodes, targets, &mut ordered, &mut visiting);
    }
    ordered
}

/// Combined results text: joins executed nodes' title+output in canvas order.
pub fn combined_results(nodes: &[PromptNode]) -> String {
    let mut out = String::new();
    for node in nodes.iter().filter(|n| !n.output.trim().is_empty()) {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&format!("## {}\n{}", node.title, node.output));
    }
    if out.is_empty() {
        out.push_str("아직 출력이 없습니다. 노드를 실행하세요.");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: usize, enabled: bool, prompt: &str) -> PromptNode {
        PromptNode::new(
            id,
            format!("n{id}"),
            prompt.to_string(),
            enabled,
            [0.0, 0.0],
        )
    }

    #[test]
    fn zoom_scaled_snap_keeps_small_lowzoom_offset() {
        // Half-zoom scene: node 1 dragged 6px to overlap its neighbor by 6px.
        // Larger than the 10.0*0.5=5px threshold, so it must not snap back.
        let others = [[552.0, 100.0, 724.0, 300.0]];
        let got = magnetic_snap([386.0, 100.0], [172.0, 200.0], &others, 10.0 * 0.5);
        assert_eq!(got, [386.0, 100.0]);
        // With the old fixed 10px threshold, the 6px gap would snap back to 380 and look jumpy.
        let old = magnetic_snap([386.0, 100.0], [172.0, 200.0], &others, 10.0);
        assert_eq!(old, [380.0, 100.0]);
    }

    #[test]
    fn twenty_three_modes_have_labels_and_presets() {
        assert_eq!(NodeMode::ALL.len(), 23);
        for m in NodeMode::ALL {
            assert!(!m.label().is_empty());
        }
        // Text presets carry an instruction; run modes carry none.
        assert!(NodeMode::Summarize.system_preset().is_some());
        assert!(NodeMode::Eli5.system_preset().is_some());
        assert!(NodeMode::Normal.system_preset().is_none());
        assert!(NodeMode::Agent.system_preset().is_none());
        assert!(NodeMode::Image.system_preset().is_none());
        assert!(NodeMode::Audio.system_preset().is_none());
        assert!(NodeMode::Python.system_preset().is_none());
        assert_eq!(NodeMode::Python.label(), "파이썬");
        // Old saves (lowercase) still load as-is.
        let old: NodeMode = serde_json::from_str("\"agent\"").unwrap();
        assert_eq!(old, NodeMode::Agent);
        let img: NodeMode = serde_json::from_str("\"image\"").unwrap();
        assert_eq!(img, NodeMode::Image);
    }

    #[test]
    fn multi_inputs_add_remove() {
        let mut n = node(1, true, "p");
        assert!(n.add_input(2));
        assert!(n.add_input(3));
        assert!(!n.add_input(2));
        assert!(!n.add_input(1));
        assert_eq!(n.inputs, vec![2, 3]);
        assert!(n.remove_input(2));
        assert!(!n.remove_input(2));
        assert_eq!(n.inputs, vec![3]);
    }

    #[test]
    fn effective_model_prefers_node_then_global() {
        let mut n = node(1, true, "p");
        assert_eq!(n.effective_model("g"), "g");
        n.model = Some("   ".to_string());
        assert_eq!(n.effective_model("g"), "g");
        n.model = Some("muse-spark-1.3".to_string());
        assert_eq!(n.effective_model("g"), "muse-spark-1.3");
    }

    #[test]
    fn runnable_skips_off_busy_and_empty() {
        let nodes = vec![
            node(1, true, "실행"),
            node(2, false, "꺼짐"),
            node(3, true, "   "),
            node(4, true, "바쁨"),
        ];
        assert_eq!(runnable_ids(&nodes, &[4]), vec![1]);
    }

    #[test]
    fn runnable_subset_keeps_canvas_order_and_filters() {
        let nodes = vec![
            node(1, true, "a"),
            node(2, false, "b"),
            node(3, true, "   "),
            node(4, true, "d"),
            node(5, true, "e"),
        ];
        // Shuffled selection order still yields canvas order. Excludes OFF, empty-prompt, and busy nodes.
        assert_eq!(runnable_subset(&nodes, &[5, 4, 3, 2, 1], &[4]), vec![1, 5]);
        assert!(runnable_subset(&nodes, &[], &[]).is_empty());
        assert!(runnable_subset(&nodes, &[99], &[]).is_empty());
    }

    #[test]
    fn clear_inputs_reports_change() {
        let mut n = node(1, true, "p");
        assert!(!n.clear_inputs());
        n.add_input(2);
        n.add_input(3);
        assert!(n.clear_inputs());
        assert!(n.inputs.is_empty());
        assert!(!n.clear_inputs());
    }

    #[test]
    fn clone_resets_output_and_count_but_keeps_prompt() {
        let mut src = node(1, true, "원본");
        src.output = "이전 출력".to_string();
        src.run_count = 5;
        let copy = src.cloned_with_id(2, [10.0, 20.0]);
        assert_eq!(copy.id, 2);
        assert_eq!(copy.prompt, "원본");
        assert!(copy.output.is_empty());
        assert_eq!(copy.run_count, 0);
        assert_eq!(copy.pos, [10.0, 20.0]);
    }

    #[test]
    fn tiles_do_not_overlap_and_grow_downward() {
        let tiles = tile_positions(7, [24.0, 24.0]);
        assert_eq!(tiles.len(), 7);
        let mut seen = std::collections::HashSet::new();
        for [x, y] in &tiles {
            let key = (*x as i32, *y as i32);
            assert!(seen.insert(key), "겹치는 타일 좌표");
        }
        // The 4th tile (second row) sits below the first row.
        assert!(tiles[3][1] > tiles[0][1]);
        assert!(tiles[3][0] == tiles[0][0]);
    }

    /// Deterministic PRNG (no external crates). For stress tests.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x.max(1);
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % (n as u64).max(1)) as usize
        }
    }

    fn random_nodes(rng: &mut Rng, n: usize) -> Vec<PromptNode> {
        let mut nodes = Vec::with_capacity(n);
        for id in 1..=n {
            let mut node = PromptNode::new(
                id,
                format!("n{id}"),
                if rng.below(4) == 0 {
                    String::new()
                } else {
                    format!("prompt {id}")
                },
                rng.below(3) != 0,
                [0.0, 0.0],
            );
            if rng.below(2) == 0 {
                node.output = format!("out-{id}");
            }
            // Random multi-input links, including self-references and cycles
            if n > 0 {
                for _ in 0..rng.below(3) {
                    node.add_input(rng.below(n) + 1);
                }
            }
            nodes.push(node);
        }
        nodes
    }

    /// QC 10,000 runs: pure functions must never break on random graphs.
    #[test]
    fn qc_ten_thousand_random_graphs() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        for _ in 0..10_000 {
            let n = rng.below(6) + 1;
            let nodes = random_nodes(&mut rng, n);
            let busy: Vec<usize> = (1..=n).filter(|_| rng.below(5) == 0).collect();
            let runnable = runnable_ids(&nodes, &busy);
            // Run targets are enabled, have a prompt, and are not busy.
            for id in &runnable {
                let node = nodes.iter().find(|x| x.id == *id).unwrap();
                assert!(node.enabled && !node.prompt.trim().is_empty());
                assert!(!busy.contains(id));
            }
            // Topo order is a permutation of the targets (terminates even with cycles).
            let order = topo_order(&nodes, &runnable);
            let mut a = order.clone();
            let mut b = runnable.clone();
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b);
            // Ancestor search always terminates, and the combined result is non-empty.
            for node in &nodes {
                let _ = ancestor_context(node, &nodes);
            }
            let text = combined_results(&nodes);
            assert!(!text.is_empty());
            // Tile coordinates never overlap each other.
            let tiles = tile_positions(n, [24.0, 24.0]);
            let mut seen = std::collections::HashSet::new();
            for [x, y] in &tiles {
                assert!(seen.insert((*x as i32, *y as i32)));
            }
            // serde round-trip
            let idx = rng.below(n);
            let json = serde_json::to_string(&nodes[idx]).unwrap();
            let back: PromptNode = serde_json::from_str(&json).unwrap();
            assert_eq!(back.id, nodes[idx].id);
            assert_eq!(back.title, nodes[idx].title);
            assert_eq!(back.inputs, nodes[idx].inputs);
        }
    }

    /// QA 1,000 runs: drives batch scenarios to completion with no deadlock or omission.
    #[test]
    fn qa_one_thousand_batch_scenarios() {
        let mut rng = Rng(0x0BAD_F00D_CAFE_1234);
        for _ in 0..1_000 {
            let n = rng.below(5) + 1;
            let nodes = random_nodes(&mut rng, n);
            let targets = runnable_ids(&nodes, &[]);
            let ordered = topo_order(&nodes, &targets);
            // Cascade simulation: from ready nodes through completion
            let mut done: Vec<usize> = Vec::new();
            let mut guard = 0usize;
            loop {
                let mut progressed = false;
                for id in &ordered {
                    if done.contains(id) {
                        continue;
                    }
                    let node = nodes.iter().find(|x| x.id == *id).unwrap();
                    let ready = node
                        .inputs
                        .iter()
                        .all(|src| !targets.contains(src) || done.contains(src));
                    if ready {
                        done.push(*id);
                        progressed = true;
                    }
                }
                guard += 1;
                if !progressed || guard > 10_000 {
                    break;
                }
            }
            assert!(guard <= 10_000, "cascade stalled");
            // Only cycle-stuck nodes remain; everything else finishes.
            for id in &ordered {
                if !done.contains(id) {
                    let node = nodes.iter().find(|x| x.id == *id).unwrap();
                    assert!(!node.inputs.is_empty(), "unfinished node without input");
                }
            }
        }
    }

    #[test]
    fn magnetic_snap_aligns_to_neighbor_edges() {
        let others = [[500.0, 100.0, 800.0, 500.0]];
        // Snaps when the left edge is 6px off the neighbor's left side.
        let p = magnetic_snap([506.0, 200.0], [300.0, 400.0], &others, 10.0);
        assert_eq!(p[0], 500.0);
        assert_eq!(p[1], 200.0);
        // Aligns the right edge to the neighbor's right side: ox = 800 - 300.
        let p = magnetic_snap([495.0, 200.0], [300.0, 400.0], &others, 10.0);
        assert_eq!(p[0], 500.0);
        // Leaves the position alone outside the threshold.
        let p = magnetic_snap([100.0, 200.0], [300.0, 400.0], &others, 10.0);
        assert_eq!(p, [100.0, 200.0]);
    }

    #[test]
    fn declump_pushes_overlap_out_on_min_axis() {
        // Neighbor overlapping by 100px: pushed right to x=300+margin 16.
        let rects = vec![
            (1, [0.0, 0.0, 300.0, 400.0]),
            (2, [200.0, 0.0, 500.0, 400.0]),
        ];
        let moves = declump(&rects, &[1], &[], 16.0);
        assert_eq!(moves, vec![(2, [316.0, 0.0])]);
    }

    #[test]
    fn declump_leaves_fixed_and_clear_alone() {
        let rects = vec![
            (1, [0.0, 0.0, 300.0, 400.0]),
            (2, [500.0, 0.0, 800.0, 400.0]),
        ];
        assert!(declump(&rects, &[1], &[], 16.0).is_empty());
    }

    #[test]
    fn declump_clears_fixed_only_and_keeps_prepile() {
        let rects = vec![
            (1, [0.0, 0.0, 300.0, 400.0]),
            (2, [0.0, 0.0, 300.0, 400.0]),
            (3, [0.0, 0.0, 300.0, 400.0]),
        ];
        // Leaves piles that already overlapped before the gesture untouched.
        assert!(declump(&rects, &[1], &[2, 3], 16.0).is_empty());
        // Pushes only newly overlapped cards off the fixed one (mutual overlap allowed).
        let moves = declump(&rects, &[1], &[], 16.0);
        assert_eq!(moves.len(), 2);
        for (id, pos) in &moves {
            let r = [pos[0], pos[1], pos[0] + 300.0, pos[1] + 400.0];
            assert!(
                !rects_overlap(r, rects[0].1, 16.0),
                "node {id} must clear fixed: {r:?}"
            );
        }
    }

    #[test]
    fn free_spot_returns_want_when_clear_steps_out_when_taken() {
        let taken = [[0.0, 0.0, 300.0, 400.0]];
        assert_eq!(
            find_free_spot([500.0, 0.0], [300.0, 400.0], &taken, 16.0),
            [500.0, 0.0]
        );
        // When `want` is blocked, searches from the top of the first ring: y=-40.
        let p = find_free_spot([0.0, 0.0], [300.0, 400.0], &taken, 16.0);
        assert!(!rects_overlap(
            [p[0], p[1], p[0] + 300.0, p[1] + 400.0],
            taken[0],
            16.0
        ));
    }

    #[test]
    fn view_spawn_stays_near_center_and_varies_by_seed() {
        // Spawns near the screen center, offset per seed.
        let a = view_spawn_pos([1000.0, 800.0], 1);
        let b = view_spawn_pos([1000.0, 800.0], 2);
        for p in [a, b] {
            assert!((p[0] - (1000.0 - 160.0)).abs() <= 80.0);
            assert!((p[1] - (800.0 - 200.0)).abs() <= 60.0);
        }
        assert_ne!(a, b);
        assert_eq!(view_spawn_pos([0.0, 0.0], 0), [-240.0, -260.0]);
    }

    #[test]
    fn clamp_panel_x_keeps_grip_visible() {
        let central = Some([100.0, 100.0, 900.0, 700.0]);
        // Under the left panel, stops with only the 60px grip visible. Y untouched.
        assert_eq!(
            clamp_panel_x([-500.0, 50.0], 300.0, central, [0.0, 0.0], 1.0),
            [-140.0, 50.0]
        );
        // Inside stays as-is (even when Y is off-screen).
        assert_eq!(
            clamp_panel_x([200.0, -9999.0], 300.0, central, [0.0, 0.0], 1.0),
            [200.0, -9999.0]
        );
        // Stays as-is at the right boundary while the grip remains visible.
        assert_eq!(
            clamp_panel_x([800.0, 600.0], 300.0, central, [0.0, 0.0], 1.0),
            [800.0, 600.0]
        );
        // Leaves the position alone when rect info is missing.
        assert_eq!(
            clamp_panel_x([-500.0, 1.0], 300.0, None, [0.0, 0.0], 1.0),
            [-500.0, 1.0]
        );
        // The 60px grip rule holds even when the card is wider than the window.
        assert_eq!(
            clamp_panel_x([-2000.0, 5.0], 1200.0, central, [0.0, 0.0], 1.0),
            [-1040.0, 5.0]
        );
    }

    #[test]
    fn step_clamps_block_exit_but_never_force_entry() {
        let board = [2400.0, 1400.0];
        // Inside: blocks only exits.
        assert_eq!(
            clamp_board_step([100.0, 100.0], [-50.0, 100.0], [300.0, 300.0], board),
            [0.0, 100.0]
        );
        // Outside: never pulls inward.
        assert_eq!(
            clamp_board_step([5000.0, 3000.0], [5030.0, 3000.0], [300.0, 300.0], board),
            [5030.0, 3000.0]
        );
        // Does not block outside cards from entering either.
        assert_eq!(
            clamp_board_step([5000.0, 100.0], [2000.0, 100.0], [300.0, 300.0], board),
            [2000.0, 100.0]
        );
        let central = Some([100.0, 100.0, 900.0, 700.0]);
        // Same for panels: a wall inside, free outside.
        assert_eq!(
            clamp_panel_x_step([200.0, 0.0], [-500.0, 0.0], 300.0, central, [0.0, 0.0], 1.0),
            [-140.0, 0.0]
        );
        assert_eq!(
            clamp_panel_x_step(
                [-500.0, 0.0],
                [-470.0, 0.0],
                300.0,
                central,
                [0.0, 0.0],
                1.0
            ),
            [-470.0, 0.0]
        );
    }

    #[test]
    fn clamp_board_keeps_nodes_on_board() {
        let board = [2400.0, 1600.0];
        // Leaves inside positions alone.
        assert_eq!(
            clamp_board([100.0, 100.0], [300.0, 400.0], board),
            [100.0, 100.0]
        );
        // Pulls left/top overflow back to 0.
        assert_eq!(
            clamp_board([-50.0, -30.0], [300.0, 400.0], board),
            [0.0, 0.0]
        );
        // Pulls right/bottom overflow in so the card fits.
        assert_eq!(
            clamp_board([2300.0, 1500.0], [300.0, 400.0], board),
            [2100.0, 1200.0]
        );
        // For cards larger than the board, leaves only the grab area.
        assert_eq!(
            clamp_board([-5000.0, -5000.0], [3000.0, 2000.0], board),
            [-600.0, -400.0]
        );
    }

    #[test]
    fn tiles_start_at_given_origin() {
        let tiles = tile_positions(4, [500.0, 300.0]);
        assert_eq!(tiles[0], [500.0, 300.0]);
        assert_eq!(tiles[1], [830.0, 300.0]);
        assert_eq!(tiles[3], [500.0, 770.0]);
    }

    #[test]
    fn undo_keeps_fresh_outputs() {
        // Restores structure while preserving run artifacts.
        let mut live = PromptNode::new(1, "a".to_string(), "p".to_string(), true, [10.0, 10.0]);
        live.output = "새 출력".to_string();
        live.prev_output = Some("옛 출력".to_string());
        live.run_count = 3;
        live.last_secs = Some(1.5);
        live.last_done = Some(99);
        let current = vec![live];
        let mut restored = vec![PromptNode::new(
            1,
            "a".to_string(),
            "p".to_string(),
            true,
            [500.0, 500.0],
        )];
        restore_results(&current, &mut restored);
        assert_eq!(restored[0].output, "새 출력");
        assert_eq!(restored[0].prev_output.as_deref(), Some("옛 출력"));
        assert_eq!(restored[0].run_count, 3);
        // Keeps the restored value for structure (position).
        assert_eq!(restored[0].pos, [500.0, 500.0]);
    }

    #[test]
    fn click_completes_link_on_input_port() {
        // Clicking the output port then the input port completes the link.
        let mut a = PromptNode::new(1, "a".into(), "p".into(), true, [0.0, 0.0]);
        let mut b = PromptNode::new(2, "b".into(), "p".into(), true, [400.0, 0.0]);
        a.last_rect = Some([0.0, 0.0, 300.0, 200.0]);
        b.last_rect = Some([400.0, 0.0, 700.0, 200.0]);
        let nodes = vec![a, b];
        // Input port center of b: [390, 100].
        assert_eq!(input_port_target(&nodes, 1, [390.0, 100.0]), Some(2));
        // Excludes itself.
        assert_eq!(input_port_target(&nodes, 2, [390.0, 100.0]), None);
        // Far-away points match nothing.
        assert_eq!(input_port_target(&nodes, 1, [0.0, 600.0]), None);
        // Within overlapping radii, distance wins over order.
        let mut c = PromptNode::new(3, "c".into(), "p".into(), true, [400.0, 16.0]);
        c.last_rect = Some([400.0, 16.0, 700.0, 216.0]);
        let nodes3 = vec![nodes[0].clone(), nodes[1].clone(), c];
        // 10 to b port [390,100], 6 to c port [390,116] → c.
        assert_eq!(input_port_target(&nodes3, 1, [390.0, 110.0]), Some(3));
    }

    #[test]
    fn link_key_chains_selection_in_id_order() {
        assert_eq!(chain_pairs(vec![3, 1, 2]), vec![(1, 2), (2, 3)]);
        assert_eq!(chain_pairs(vec![5]), vec![]);
        assert_eq!(chain_pairs(vec![]), vec![]);
    }

    #[test]
    fn run_count_saturates_at_max() {
        let mut n = node(1, true, "x");
        n.run_count = u32::MAX;
        n.record_run();
        assert_eq!(n.run_count, u32::MAX);
    }

    fn chained() -> Vec<PromptNode> {
        let mut a = node(1, true, "pA");
        a.title = "A".to_string();
        a.output = "out-A".to_string();
        let mut b = node(2, true, "pB");
        b.title = "B".to_string();
        b.output = "out-B".to_string();
        b.add_input(1);
        let mut c = node(3, true, "pC");
        c.title = "C".to_string();
        c.add_input(2);
        vec![a, b, c]
    }

    #[test]
    fn ancestors_come_oldest_first() {
        let nodes = chained();
        let ctx = ancestor_context(&nodes[2], &nodes);
        assert_eq!(ctx.len(), 2);
        assert!(ctx[0].source.contains('A'));
        assert_eq!(ctx[0].text, "out-A");
        assert!(ctx[1].source.contains('B'));
    }

    #[test]
    fn ancestors_skip_empty_and_missing() {
        let mut nodes = chained();
        nodes[0].output.clear();
        nodes[2].inputs = vec![99];
        let ctx = ancestor_context(&nodes[2], &nodes);
        assert!(ctx.is_empty());
    }

    #[test]
    fn ancestors_survive_cycles_and_self_loops() {
        let mut nodes = chained();
        nodes[0].add_input(3); // A→C→B→A cycle
        let ctx = ancestor_context(&nodes[2], &nodes);
        assert!(ctx.len() <= 3);
        let mut solo = node(7, true, "S");
        solo.inputs = vec![7];
        let pool = vec![solo.clone()];
        assert!(ancestor_context(&solo, &pool).is_empty());
    }

    #[test]
    fn topo_runs_sources_first() {
        let nodes = chained();
        let order = topo_order(&nodes, &[3, 2, 1]);
        assert_eq!(order, vec![1, 2, 3]);
    }

    #[test]
    fn topo_ignores_inputs_outside_targets() {
        let nodes = chained();
        let order = topo_order(&nodes, &[3]);
        assert_eq!(order, vec![3]);
    }

    #[test]
    fn combined_joins_outputs_in_canvas_order() {
        let nodes = chained();
        let text = combined_results(&nodes);
        assert!(text.contains("## A"));
        assert!(text.contains("out-B"));
        assert!(text.find("## A").unwrap() < text.find("## B").unwrap());
    }

    #[test]
    fn ports_sit_outside_card_edges() {
        let r = [100.0, 50.0, 300.0, 250.0];
        let o = out_port(r);
        let i = in_port(r);
        assert!(o[0] > r[2]);
        assert!(i[0] < r[0]);
        assert!((o[1] - 150.0).abs() < f32::EPSILON);
        assert!((i[1] - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn error_prefix_detected_in_both_langs() {
        assert!(is_error_output("[오류] 터짐"));
        assert!(is_error_output("[error] boom"));
        assert!(!is_error_output("정상 출력"));
        assert!(!is_error_output(""));
    }

    #[test]
    fn new_node_defaults_fold_image_secs() {
        let n = PromptNode::new(1, "t".into(), "p".into(), true, [0.0, 0.0]);
        assert!(!n.title_only);
        assert!(n.image_path.is_none());
        assert!(n.last_secs.is_none());
        assert!((n.width_scale - 1.0).abs() < f32::EPSILON);
        assert!((n.height_scale - 1.0).abs() < f32::EPSILON);
        let c = n.cloned_with_id(2, [0.0, 0.0]);
        assert!(!c.title_only);
        assert!(c.last_secs.is_none());
        assert!((c.width_scale - 1.0).abs() < f32::EPSILON);
        assert!((c.height_scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cycle_links_are_detected() {
        // 1 <- 2 <- 3 chain: linking 3 as a source of 1 or 2 forms a cycle.
        let mut ns: Vec<PromptNode> = (1..=3)
            .map(|i| PromptNode::new(i, format!("N{i}"), "p".into(), true, [0.0, 0.0]))
            .collect();
        ns[1].add_input(1);
        ns[2].add_input(2);
        assert!(creates_cycle(&ns, 1, 3));
        assert!(creates_cycle(&ns, 2, 3));
        assert!(!creates_cycle(&ns, 3, 1));
        assert!(!creates_cycle(&ns, 3, 2));
        assert!(creates_cycle(&ns, 1, 1));
        assert!(!creates_cycle(&ns, 3, 99));
    }

    #[test]
    fn mutual_only_allows_direct_pair_and_blocks_longer() {
        // After A→B, B→A is a direct pair and thus allowed.
        let mut ns = vec![node(1, true, "a"), node(2, true, "b"), node(3, true, "c")];
        ns[1].add_input(1);
        assert!(is_mutual_only(&ns, 1, 2));
        assert!(!blocked_cycle(&ns, 1, 2));
        // With X→Y and X→Z→Y present, Y→X is blocked by the long cycle involved.
        let mut ms = vec![node(1, true, "x"), node(2, true, "y"), node(3, true, "z")];
        ms[1].add_input(1);
        ms[1].add_input(3);
        ms[2].add_input(1);
        assert!(!is_mutual_only(&ms, 1, 2));
        assert!(blocked_cycle(&ms, 1, 2));
        // Cycles without a reverse edge and self-links are blocked.
        assert!(!is_mutual_only(&ns, 1, 1));
        assert!(blocked_cycle(&ns, 1, 1));
        // Blocks nothing when there is no cycle.
        assert!(!blocked_cycle(&ns, 3, 1));
    }

    #[test]
    fn custom_accent_roundtrips() {
        let a = NodeAccent::Custom([10, 20, 30]);
        assert_eq!(a.rgb(), (10, 20, 30));
        let s = serde_json::to_string(&a).unwrap();
        let back: NodeAccent = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
        // Old saves (string form) still load as-is.
        let old: NodeAccent = serde_json::from_str("\"blue\"").unwrap();
        assert_eq!(old, NodeAccent::Blue);
    }

    #[test]
    fn old_save_without_new_fields_loads() {
        let json = r#"{"id":7,"title":"old","prompt":"p","output":"","enabled":true,"run_count":0,"pos":[0.0,0.0]}"#;
        let n: PromptNode = serde_json::from_str(json).unwrap();
        assert!(!n.title_only);
        assert!(n.image_path.is_none());
    }
}
