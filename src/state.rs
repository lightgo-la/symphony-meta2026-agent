//! App state, persistence, undo, node operations, and view helpers.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::i18n::{t, Lang};
use crate::mcp::{McpManager, McpServerConfig, McpTool};
use crate::node::{
    blocked_cycle, cascade_pos, clamp_board, clamp_panel_x, find_free_spot, is_error_output,
    is_mutual_only, restore_results, short_title, tile_positions, view_spawn_pos, PromptNode,
};
use crate::run::PendingJob;
use crate::store::{load_from, save_to, store_path, AppConfig, StoredDoc};
use crate::zoom::clamp_zoom;

/// Snapshot for undoing structural changes (excludes text typing).
#[derive(Clone)]
pub(crate) struct UndoSnap {
    pub(crate) nodes: Vec<PromptNode>,
    pub(crate) next_id: usize,
}

/// Batch run state machine. Tracks the start-progress-finish flow of the whole run
/// in one place, regardless of individual job success or failure.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum BatchScope {
    /// All ON nodes.
    All,
    /// Only the selected nodes.
    Selected,
}

/// One context-menu action on a link. Relative to the src→dst edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkMenuAction {
    /// Unlinks this edge.
    Unlink { src: usize, dst: usize },
    /// Links the reverse direction to make it bidirectional.
    MakeMutual { src: usize, dst: usize },
    /// Unlinks the reverse direction to restore one-way.
    MakeSingle { src: usize, dst: usize },
    /// Runs the node at one end.
    Run(usize),
}

/// One menu item row. The view renders it; `apply_link_menu` executes it.
#[derive(Clone, Debug)]
pub(crate) struct LinkMenuItem {
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) tip: Option<String>,
    pub(crate) action: LinkMenuAction,
}

/// Menu row: item or separator.
#[derive(Clone, Debug)]
pub(crate) enum LinkMenuRow {
    Item(LinkMenuItem),
    Sep,
}

#[derive(Clone, PartialEq)]
pub(crate) enum BatchState {
    /// Not running.
    Idle,
    /// Running. queue=all targets, done=finished (success+failure), failed=failed ones.
    Running {
        queue: Vec<usize>,
        done: Vec<usize>,
        failed: Vec<usize>,
        scope: BatchScope,
    },
}

impl BatchState {
    pub(crate) fn is_active(&self) -> bool {
        matches!(self, BatchState::Running { .. })
    }
    pub(crate) fn queue(&self) -> &[usize] {
        match self {
            BatchState::Idle => &[],
            BatchState::Running { queue, .. } => queue,
        }
    }
    pub(crate) fn done_count(&self) -> usize {
        match self {
            BatchState::Idle => 0,
            BatchState::Running { done, .. } => done.len(),
        }
    }
    pub(crate) fn failed_count(&self) -> usize {
        match self {
            BatchState::Idle => 0,
            BatchState::Running { failed, .. } => failed.len(),
        }
    }
    pub(crate) fn scope(&self) -> BatchScope {
        match self {
            BatchState::Idle => BatchScope::All,
            BatchState::Running { scope, .. } => *scope,
        }
    }
}

/// Current unix time in seconds. For timestamp records.
pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// View mode of the standalone live-results window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ResultsWinMode {
    /// Per-node output list.
    #[default]
    PerNode,
    /// Combined full output.
    Combined,
}

/// Per-node filter of the standalone live-results window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ResultsWinFilter {
    /// All.
    #[default]
    All,
    /// Only running ones.
    Running,
    /// Only errors.
    Errors,
}

pub struct App {
    pub(crate) nodes: Vec<PromptNode>,
    pub(crate) next_id: usize,
    pub(crate) status: String,
    pub(crate) canvas: [f32; 2],
    pub(crate) config: AppConfig,
    /// Session key. Kept in memory only. Never save or serialize it.
    pub(crate) session_key: String,
    pub(crate) show_settings: bool,
    pub(crate) settings_is_mcp: bool,
    pub(crate) pending: HashMap<usize, PendingJob>,
    pub(crate) linking: Option<usize>,
    pub(crate) was_down: bool,
    /// Point pressed on a port (for drag detection).
    pub(crate) press_pos: Option<egui::Pos2>,
    /// Two-step delete confirm: (node id, first-click time).
    pub(crate) confirm_delete: Option<(usize, Instant)>,
    /// Id of the node whose title is being edited.
    pub(crate) editing_title: Option<usize>,
    /// Requests focus once right after entering title edit.
    pub(crate) title_focus_pending: bool,
    /// Set of node ids with unsaved changes.
    pub(crate) touched: std::collections::HashSet<usize>,
    /// Node thumbnail texture cache: id -> (path, handle).
    pub(crate) node_textures: HashMap<usize, (String, egui::TextureHandle)>,
    /// Expanded context blocks: (node id, block idx).
    pub(crate) open_ctx: std::collections::HashSet<(usize, usize)>,
    /// Set of node ids showing their previous output.
    pub(crate) prev_open: std::collections::HashSet<usize>,
    /// Two-step output-clear confirm time.
    pub(crate) confirm_clear: Option<Instant>,
    /// Two-step API-reset confirm time.
    pub(crate) confirm_api_reset: Option<Instant>,
    /// Structural-change undo/redo stacks.
    pub(crate) undo: Vec<UndoSnap>,
    pub(crate) redo: Vec<UndoSnap>,
    /// Node search query.
    pub(crate) search: String,
    /// Shows the about window.
    pub(crate) show_about: bool,
    /// Shows the minimap.
    pub(crate) show_minimap: bool,
    /// Last error per MCP server.
    pub(crate) mcp_errors: HashMap<String, String>,
    /// Screen rect of the central canvas. Refreshed every frame to keep nodes inside.
    pub(crate) central_rect: Option<[f32; 4]>,
    /// Canvas zoom factor + pan offset (screen px).
    pub(crate) zoom: f32,
    /// Screen rects of in-card scroll areas (prompt input/output fields): node id -> rect list.
    /// Based on the previous frame, so it lags one frame like `last_rect`.
    /// Used for wheel-zoom hit testing; excluded from save/undo.
    pub(crate) scroll_zones: HashMap<usize, Vec<[f32; 4]>>,
    /// Screen rects of per-node output rows in the results window: node id -> rect. For click-to-jump.
    /// Based on the previous frame, so it lags one frame like `last_rect`.
    pub(crate) result_row_rects: HashMap<usize, [f32; 4]>,
    /// Batch run state machine (Idle=waiting, Running=running).
    pub(crate) batch: BatchState,
    pub(crate) results_open: bool,
    /// Fresh-start confirm modal.
    pub(crate) new_confirm_open: bool,
    /// Full-text window for the combined results.
    pub(crate) show_combined_zoom: bool,
    /// Standalone live-results window (OS window).
    pub(crate) show_results_window: bool,
    /// View mode of the standalone window.
    pub(crate) results_win_mode: ResultsWinMode,
    /// Per-node filter of the standalone window.
    pub(crate) results_win_filter: ResultsWinFilter,
    /// Auto-scroll of the standalone window.
    pub(crate) results_win_follow: bool,
    /// Word wrap of the standalone window.
    pub(crate) results_win_wrap: bool,
    /// Node id shown in the node-output standalone window (OS window).
    pub(crate) output_window: Option<usize>,
    /// Drag/resize gesture start: (pinned ids, already-overlapping neighbor ids).
    /// Kept in memory only so drop resolves just the newly created overlaps.
    pub(crate) drop_baseline: Option<(Vec<usize>, Vec<usize>)>,
    /// Selected nodes shown in the right detail panel. View state, excluded from save/undo.
    pub(crate) selected: HashSet<usize>,
    /// Screen rects of overlays (modals, minimap) that block wheel zoom. Previous-frame basis.
    pub(crate) overlay_rects: Vec<[f32; 4]>,
    /// Overlay rects collected this frame. Moved to overlay_rects at frame end.
    pub(crate) overlay_staging: Vec<[f32; 4]>,
    /// Box-selection start point (screen coords). Some only during Shift+drag.
    pub(crate) band_start: Option<[f32; 2]>,
    /// Whether Shift was held when box selection started. Used for additivity on release.
    pub(crate) band_additive: bool,
    /// Two-step multi-delete confirm: (selected id list, first Delete time).
    pub(crate) confirm_multi_delete: Option<(Vec<usize>, Instant)>,
    pub(crate) test_job: Option<PendingJob>,
    pub(crate) test_status: String,
    /// Blueprint (prompt-to-nodes) input + job status. Not saved.
    pub(crate) blueprint_open: bool,
    pub(crate) blueprint_text: String,
    pub(crate) blueprint_job: Option<PendingJob>,
    pub(crate) blueprint_status: String,
    /// Alarm delivery job + last result. View state, not saved.
    pub(crate) alarm_job: Option<PendingJob>,
    pub(crate) alarm_status: String,
    pub(crate) mcp: McpManager,
    pub(crate) servers: Vec<McpServerConfig>,
    pub(crate) mcp_tools: Vec<McpTool>,
    pub(crate) mcp_status: String,
    pub(crate) new_srv_name: String,
    pub(crate) new_srv_cmd: String,
    pub(crate) new_srv_args: String,
    pub(crate) tool_menu_for: Option<usize>,
    pub(crate) tool_server: String,
    pub(crate) tool_name: String,
    pub(crate) tool_args: String,
    /// Set of node ids with the input-link "＋ 추가" list expanded. View state, not saved.
    pub(crate) link_add_open: HashSet<usize>,
    /// Model names used for runs (latest 10). Session-only.
    pub(crate) model_history: Vec<String>,
    pub(crate) dirty: bool,
    pub(crate) last_save: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let lang = Lang::default();
        let mut app = Self {
            nodes: Vec::new(),
            next_id: 1,
            status: String::from(t(lang, "노드를 추가하고 실행을 눌러보세요.")),
            canvas: [0.0, 0.0],
            config: AppConfig::default(),
            session_key: String::new(),
            show_settings: false,
            settings_is_mcp: false,
            pending: HashMap::new(),
            linking: None,
            was_down: false,
            press_pos: None,
            confirm_delete: None,
            editing_title: None,
            title_focus_pending: false,
            touched: std::collections::HashSet::new(),
            node_textures: HashMap::new(),
            open_ctx: std::collections::HashSet::new(),
            prev_open: std::collections::HashSet::new(),
            confirm_clear: None,
            confirm_api_reset: None,
            undo: Vec::new(),
            redo: Vec::new(),
            search: String::new(),
            show_about: false,
            show_minimap: true,
            mcp_errors: HashMap::new(),
            central_rect: None,
            zoom: 1.0,
            scroll_zones: HashMap::new(),
            result_row_rects: HashMap::new(),
            batch: BatchState::Idle,
            results_open: true,
            new_confirm_open: false,
            show_combined_zoom: false,
            show_results_window: false,
            results_win_mode: ResultsWinMode::default(),
            results_win_filter: ResultsWinFilter::default(),
            results_win_follow: true,
            results_win_wrap: true,
            output_window: None,
            drop_baseline: None,
            selected: HashSet::new(),
            overlay_rects: Vec::new(),
            overlay_staging: Vec::new(),
            band_start: None,
            band_additive: false,
            confirm_multi_delete: None,
            test_job: None,
            test_status: String::new(),
            blueprint_open: false,
            blueprint_text: String::new(),
            blueprint_job: None,
            blueprint_status: String::new(),
            alarm_job: None,
            alarm_status: String::new(),
            mcp: McpManager::default(),
            servers: Vec::new(),
            mcp_tools: Vec::new(),
            mcp_status: String::new(),
            new_srv_name: String::new(),
            new_srv_cmd: String::new(),
            new_srv_args: String::new(),
            tool_menu_for: None,
            tool_server: String::new(),
            tool_name: String::new(),
            tool_args: String::from("{}"),
            link_add_open: HashSet::new(),
            model_history: Vec::new(),
            dirty: false,
            last_save: Instant::now(),
        };
        crate::theme::apply_meta_theme(&cc.egui_ctx, app.config.theme);
        if !app.restore() {
            app.seed_sample_nodes();
            app.dirty = true;
        }
        app
    }
    // ── Persistence ──────────────────────────────────────────────

    pub(crate) fn snapshot(&self) -> StoredDoc {
        let _lang = self.config.lang;
        StoredDoc {
            version: 1,
            nodes: self.nodes.clone(),
            next_id: self.next_id,
            config: self.config.clone(),
            canvas: self.canvas,
            zoom: self.zoom,
            mcp_servers: self.servers.clone(),
        }
    }

    pub(crate) fn restore(&mut self) -> bool {
        let lang = self.config.lang;
        let Some(path) = store_path() else {
            return false;
        };
        let Ok(doc) = load_from(&path) else {
            return false;
        };
        if doc.nodes.is_empty() && doc.next_id == 0 {
            return false;
        }
        self.nodes = doc.nodes;
        self.zoom = doc.zoom.max(0.001);
        self.next_id = doc.next_id.max(1);
        self.config = doc.config;
        self.canvas = doc.canvas;
        self.servers = doc.mcp_servers;
        self.prune_stale_refs();
        self.status = t(lang, "이전 작업을 불러왔습니다.").to_string();
        true
    }

    pub(crate) fn autosave(&mut self) {
        let lang = self.config.lang;
        // Saves with a 0.5s debounce only when there are changes.
        // Never touches the disk when clean.
        if !self.dirty {
            return;
        }
        if self.last_save.elapsed() < Duration::from_millis(500) {
            return;
        }
        if let Some(path) = store_path() {
            if let Err(e) = save_to(&path, &self.snapshot()) {
                self.status = t(lang, "자동 저장 실패: {e}").replace("{e}", &e.to_string());
            } else {
                self.dirty = false;
                self.touched.clear();
            }
            // Updates the timestamp even on failure so retries never hammer the disk every frame.
            self.last_save = Instant::now();
        }
    }

    pub(crate) fn mark_dirty(&mut self) {
        let _lang = self.config.lang;
        self.dirty = true;
    }

    /// Resets API settings to the first-run state. Clears connection values, saved key, and session key,
    /// and marks for autosave. Not an undo target.
    pub(crate) fn reset_api_settings(&mut self) {
        let _lang = self.config.lang;
        self.config.reset_api();
        self.session_key.clear();
        self.test_status.clear();
        self.mark_dirty();
    }

    /// Saves right now (Ctrl+S).
    /// Syncs the saved key with the remember setting. The settings view calls this every frame.
    pub(crate) fn sync_saved_key(&mut self) {
        if self.config.remember_key {
            let session = self.session_key.trim().to_string();
            if !session.is_empty() && self.config.saved_key != session {
                self.config.saved_key = session;
            }
        } else if !self.config.saved_key.is_empty() {
            self.config.saved_key.clear();
        }
    }

    /// Returns node ids matching the standalone results-window filter, in canvas order.
    pub(crate) fn filter_result_nodes(&self, filter: ResultsWinFilter) -> Vec<usize> {
        self.nodes
            .iter()
            .filter(|n| match filter {
                ResultsWinFilter::All => true,
                ResultsWinFilter::Running => self.pending.contains_key(&n.id),
                ResultsWinFilter::Errors => is_error_output(&n.output),
            })
            .map(|n| n.id)
            .collect()
    }

    pub(crate) fn save_now(&mut self) {
        if let Some(path) = store_path() {
            self.save_now_to(&path);
        }
    }

    /// Saves to the given path. Returns true on success. Modals/tests pick the path.
    pub(crate) fn save_now_to(&mut self, path: &std::path::Path) -> bool {
        let lang = self.config.lang;
        match save_to(path, &self.snapshot()) {
            Ok(()) => {
                self.last_save = Instant::now();
                self.dirty = false;
                self.touched.clear();
                self.status = t(lang, "저장했습니다.").to_string();
                true
            }
            Err(e) => {
                self.status = t(lang, "자동 저장 실패: {e}").replace("{e}", &e.to_string());
                false
            }
        }
    }

    /// Opens the save file at the given path into the canvas. Returns true on success.
    /// The file-menu button, Ctrl+O, and tests pick the path.
    pub(crate) fn open_from_path(&mut self, path: &std::path::Path) -> bool {
        match load_from(path) {
            Ok(doc) => {
                self.apply_doc(doc);
                let lang = self.config.lang;
                self.status = t(lang, "저장된 파일을 열었습니다.").to_string();
                true
            }
            Err(e) => {
                let lang = self.config.lang;
                self.status = t(lang, "가져오기 실패: {e}").replace("{e}", &e);
                false
            }
        }
    }

    /// Opens a JSON file picked from the file dialog. Cancel does nothing.
    pub(crate) fn open_file_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        self.open_from_path(&path);
    }

    /// Reports the font installed at startup in the status bar.
    pub fn note_font(&mut self, name: &str) {
        let lang = self.config.lang;
        let label = if name == "기본" {
            t(lang, "기본")
        } else {
            name
        };
        self.status = t(lang, "{} · 폰트: {name}")
            .replace("{}", &self.status)
            .replace("{name}", label);
    }

    // ── Node operations ───────────────────────────────────────────

    /// Three sample nodes for the first screen. Shared by first run and fresh start.
    pub(crate) fn seed_sample_nodes(&mut self) {
        self.add_node(
            "아이디어 1".to_string(),
            "신상품 네이밍 아이디어 5개를 짧게 제안해줘.".to_string(),
            true,
        );
        self.add_node(
            "아이디어 2".to_string(),
            "출근길 10분 운동 루틴을 요일별로 짜줘.".to_string(),
            true,
        );
        self.add_node(
            "끄기 예시".to_string(),
            "이 노드는 OFF라서 전체 실행에서 제외됩니다.".to_string(),
            false,
        );
    }

    /// Resets the board to the first screen. Keeps settings, connections, and the session key.
    /// Keeps one pre-reset snapshot so undo can restore it.
    pub(crate) fn reset_workspace(&mut self) {
        let lang = self.config.lang;
        self.push_undo();
        let safety = self.undo.pop();
        self.nodes.clear();
        self.next_id = 1;
        self.selected.clear();
        self.touched.clear();
        self.pending.clear();
        self.batch = BatchState::Idle;
        self.test_job = None;
        self.test_status.clear();
        self.alarm_job = None;
        self.alarm_status.clear();
        self.canvas = [0.0, 0.0];
        self.zoom = 1.0;
        self.scroll_zones.clear();
        self.result_row_rects.clear();
        self.overlay_rects.clear();
        self.overlay_staging.clear();
        self.drop_baseline = None;
        self.band_start = None;
        self.band_additive = false;
        self.linking = None;
        self.press_pos = None;
        self.was_down = false;
        self.confirm_delete = None;
        self.confirm_multi_delete = None;
        self.confirm_clear = None;
        self.editing_title = None;
        self.title_focus_pending = false;
        self.open_ctx.clear();
        self.prev_open.clear();
        self.node_textures.clear();
        self.search.clear();
        self.link_add_open.clear();
        self.tool_menu_for = None;
        self.tool_server.clear();
        self.tool_name.clear();
        self.tool_args = String::from("{}");
        self.mcp_errors.clear();
        self.show_settings = false;
        self.settings_is_mcp = false;
        self.show_about = false;
        self.show_minimap = true;
        self.results_open = true;
        self.new_confirm_open = false;
        self.show_combined_zoom = false;
        self.show_results_window = false;
        self.results_win_mode = ResultsWinMode::default();
        self.results_win_filter = ResultsWinFilter::default();
        self.results_win_follow = true;
        self.results_win_wrap = true;
        self.output_window = None;
        self.seed_sample_nodes();
        self.undo.clear();
        if let Some(s) = safety {
            self.undo.push(s);
        }
        self.redo.clear();
        self.dirty = true;
        self.last_save = Instant::now();
        self.status = t(lang, "새로 시작했습니다.").to_string();
    }

    pub(crate) fn add_node(&mut self, title: String, prompt: String, enabled: bool) {
        let _lang = self.config.lang;
        self.push_undo();
        // Infinite map: spawns at the current view center even after panning far away.
        // Falls back to a near-origin cascade when no view info exists yet (first run).
        let pos = match self.central_rect {
            Some(r) => {
                let z = self.zoom.max(0.001);
                let center = [
                    ((r[0] + r[2]) / 2.0 - self.canvas[0]) / z,
                    ((r[1] + r[3]) / 2.0 - self.canvas[1]) / z,
                ];
                view_spawn_pos(center, self.next_id)
            }
            None => cascade_pos(self.next_id),
        };
        // Moves the new card to a free spot so it never lands on an existing stack.
        let z = self.zoom.max(0.001);
        let taken: Vec<[f32; 4]> = self
            .nodes
            .iter()
            .map(|n| {
                let [w, h] = n
                    .last_rect
                    .map(|r| [(r[2] - r[0]) / z, (r[3] - r[1]) / z])
                    .unwrap_or([330.0, 470.0]);
                [n.pos[0], n.pos[1], n.pos[0] + w, n.pos[1] + h]
            })
            .collect();
        // Skips the board clamp: spawning at a far-panned spot is normal,
        // and the board is a soft boundary at drag time.
        let pos = find_free_spot(pos, [330.0, 470.0], &taken, 16.0);
        // Skips the panel clamp: pulling inward would stack onto existing cards.
        // Card-area clipping prevents menu overlap.
        self.nodes
            .push(PromptNode::new(self.next_id, title, prompt, enabled, pos));
        self.next_id += 1;
        self.mark_dirty();
    }

    /// Snapshots before a structural change (max 50).
    pub(crate) fn push_undo(&mut self) {
        let _lang = self.config.lang;
        self.undo.push(UndoSnap {
            nodes: self.nodes.clone(),
            next_id: self.next_id,
        });
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub(crate) fn undo(&mut self) {
        let lang = self.config.lang;
        if let Some(snap) = self.undo.pop() {
            self.redo.push(UndoSnap {
                nodes: self.nodes.clone(),
                next_id: self.next_id,
            });
            let live = std::mem::replace(&mut self.nodes, snap.nodes);
            // Restores only the structure, keeping just-received outputs.
            restore_results(&live, &mut self.nodes);
            self.prune_stale_refs();
            self.next_id = snap.next_id;
            self.pending.clear();
            self.batch = BatchState::Idle;
            self.status = t(lang, "되돌렸습니다.").to_string();
            self.mark_dirty();
        }
    }

    pub(crate) fn redo(&mut self) {
        let lang = self.config.lang;
        if let Some(snap) = self.redo.pop() {
            self.undo.push(UndoSnap {
                nodes: self.nodes.clone(),
                next_id: self.next_id,
            });
            let live = std::mem::replace(&mut self.nodes, snap.nodes);
            restore_results(&live, &mut self.nodes);
            self.prune_stale_refs();
            self.next_id = snap.next_id;
            self.pending.clear();
            self.batch = BatchState::Idle;
            self.status = t(lang, "다시 적용했습니다.").to_string();
            self.mark_dirty();
        }
    }

    /// Stops all running jobs.
    pub(crate) fn cancel_all(&mut self) {
        let lang = self.config.lang;
        let n = self.pending.len();
        self.pending.clear();
        self.batch = BatchState::Idle;
        self.status = if n > 0 {
            t(lang, "전체 {n}개 작업을 멈췄습니다.").replace("{n}", &n.to_string())
        } else {
            t(lang, "진행 중인 작업이 없습니다.").to_string()
        };
    }

    /// Drops one node from the batch queue (for individual cancel).
    pub(crate) fn batch_drop(&mut self, id: usize) {
        if let BatchState::Running {
            queue,
            done,
            failed,
            ..
        } = &mut self.batch
        {
            queue.retain(|c| *c != id);
            done.retain(|c| *c != id);
            failed.retain(|c| *c != id);
        }
    }

    /// Cleans up residue after a document swap: dangling id refs, selection, open states.
    pub(crate) fn prune_stale_refs(&mut self) {
        let live: std::collections::HashSet<usize> = self.nodes.iter().map(|n| n.id).collect();
        for n in self.nodes.iter_mut() {
            n.inputs.retain(|c| *c != n.id && live.contains(c));
        }
        self.selected.retain(|id| live.contains(id));
        self.open_ctx.retain(|(nid, _)| live.contains(nid));
        self.prev_open.retain(|id| live.contains(id));
        if let Some(id) = self.tool_menu_for {
            if !live.contains(&id) {
                self.tool_menu_for = None;
            }
        }
        if let Some(id) = self.editing_title {
            if !live.contains(&id) {
                self.editing_title = None;
            }
        }
        if let Some(id) = self.linking {
            if !live.contains(&id) {
                self.linking = None;
            }
        }
        self.confirm_delete = self.confirm_delete.filter(|(cid, _)| live.contains(cid));
        self.confirm_multi_delete = None;
        self.band_start = None;
        self.link_add_open.retain(|id| live.contains(id));
        if self.output_window.is_some_and(|id| !live.contains(&id)) {
            self.output_window = None;
        }
        self.purge_stale_textures();
    }

    /// Replaces everything with the imported document.
    pub(crate) fn apply_doc(&mut self, doc: StoredDoc) {
        let lang = self.config.lang;
        self.push_undo();
        self.nodes = doc.nodes;
        self.next_id = doc.next_id.max(1);
        self.config = doc.config;
        self.canvas = doc.canvas;
        self.zoom = doc.zoom.max(0.001);
        self.servers = doc.mcp_servers;
        self.pending.clear();
        self.batch = BatchState::Idle;
        // A wholesale document swap invalidates previous selection, open, and link states.
        self.prune_stale_refs();
        self.status = t(lang, "캔버스를 가져왔습니다.").to_string();
        self.mark_dirty();
    }

    /// Drops thumbnail caches for missing nodes. Handles are held by egui,
    /// so removing them from the map releases them from the GPU next frame.
    pub(crate) fn purge_stale_textures(&mut self) {
        let live: std::collections::HashSet<usize> = self.nodes.iter().map(|n| n.id).collect();
        self.node_textures.retain(|id, _| live.contains(id));
    }

    /// Converts unix seconds into a relative time like "N minutes ago".
    pub(crate) fn ago_text(lang: Lang, done: u64) -> String {
        let d = unix_now().saturating_sub(done);
        if d < 60 {
            t(lang, "{}초 전").replace("{}", &d.to_string())
        } else if d < 3600 {
            t(lang, "{}분 전").replace("{}", &(d / 60).to_string())
        } else if d < 86400 {
            t(lang, "{}시간 전").replace("{}", &(d / 3600).to_string())
        } else {
            t(lang, "{}일 전").replace("{}", &(d / 86400).to_string())
        }
    }

    /// Viewport center. Used as the anchor for button/shortcut zoom.
    pub(crate) fn zoom_center(&self) -> [f32; 2] {
        match self.central_rect {
            Some(r) => [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0],
            None => [550.0, 350.0],
        }
    }

    /// Brings a node to the view center (for search jump).
    pub(crate) fn focus_node(&mut self, id: usize) {
        let lang = self.config.lang;
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        let (cx, cy) = match self.central_rect {
            Some(r) => ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0),
            None => (550.0, 350.0),
        };
        // Centers on the real card size when known, otherwise uses the existing estimate.
        let (hw, hh) = match node.last_rect {
            Some(r) => ((r[2] - r[0]) / 2.0, (r[3] - r[1]) / 2.0),
            None => (150.0 * self.zoom, 60.0 * self.zoom),
        };
        self.canvas = [
            cx - node.pos[0] * self.zoom - hw,
            cy - node.pos[1] * self.zoom - hh,
        ];
        self.status = t(lang, "'{}'로 이동했습니다.").replace("{}", &node.title);
        self.mark_dirty();
    }

    /// Fits zoom and position so all nodes are visible.
    pub(crate) fn fit_view(&mut self) {
        let lang = self.config.lang;
        let (w, h) = match self.central_rect {
            Some(r) => (r[2] - r[0], r[3] - r[1]),
            None => (1100.0, 700.0),
        };
        if self.nodes.is_empty() {
            self.zoom = 1.0;
            self.canvas = [0.0, 0.0];
            return;
        }
        // Normalizes per-node bounds to canvas coords (last_rect is in screen coords).
        let z = self.zoom.max(0.01);
        let c = self.canvas;
        let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for n in &self.nodes {
            let (nx0, ny0, nx1, ny1) = match n.last_rect {
                Some(r) => (
                    (r[0] - c[0]) / z,
                    (r[1] - c[1]) / z,
                    (r[2] - c[0]) / z,
                    (r[3] - c[1]) / z,
                ),
                None => (n.pos[0], n.pos[1], n.pos[0] + 320.0, n.pos[1] + 420.0),
            };
            bx0 = bx0.min(nx0);
            by0 = by0.min(ny0);
            bx1 = bx1.max(nx1);
            by1 = by1.max(ny1);
        }
        let bw = (bx1 - bx0).max(1.0);
        let bh = (by1 - by0).max(1.0);
        let zoom = clamp_zoom(((w - 80.0) / bw).min((h - 80.0) / bh));
        self.zoom = zoom;
        self.canvas = [40.0 - bx0 * zoom, 40.0 - by0 * zoom];
        self.status = t(lang, "전체 보기로 맞췄습니다.").to_string();
        self.mark_dirty();
    }

    pub(crate) fn clone_node(&mut self, id: usize) {
        let lang = self.config.lang;
        let Some(src) = self.nodes.iter().find(|n| n.id == id).cloned() else {
            return;
        };
        self.push_undo();
        // Spawns below-right of the original. Not a trailing cascade.
        // Clamps inside the map bounds, then also avoids going under the panel.
        let pos = [src.pos[0] + 40.0, src.pos[1] + 40.0];
        let z = self.zoom.max(0.001);
        let size = src
            .last_rect
            .map(|r| [(r[2] - r[0]) / z, (r[3] - r[1]) / z])
            .unwrap_or([320.0, 420.0]);
        let pos = clamp_board(pos, size, [self.config.board_w, self.config.board_h]);
        let pos = clamp_panel_x(pos, size[0] * z, self.central_rect, self.canvas, self.zoom);
        let copy = src.cloned_with_id(self.next_id, pos);
        self.next_id += 1;
        self.nodes.push(copy);
        self.status = t(lang, "노드를 복제했습니다.").to_string();
        self.mark_dirty();
    }

    pub(crate) fn delete_node(&mut self, id: usize) {
        self.delete_nodes(&[id]);
    }

    /// Deletes several nodes in a single undo step.
    pub(crate) fn delete_nodes(&mut self, ids: &[usize]) {
        let lang = self.config.lang;
        let gone: HashSet<usize> = ids.iter().copied().collect();
        if gone
            .iter()
            .all(|id| !self.nodes.iter().any(|n| n.id == *id))
        {
            return;
        }
        self.push_undo();
        let mut removed_count = 0usize;
        let mut removed_title = String::new();
        for id in &gone {
            let Some(i) = self.nodes.iter().position(|n| n.id == *id) else {
                continue;
            };
            let removed = self.nodes.remove(i);
            removed_title = removed.title;
            removed_count += 1;
            self.pending.remove(id);
            // Drops it from the batch queue so nothing lingers after finishing.
            if let BatchState::Running {
                queue,
                done,
                failed,
                ..
            } = &mut self.batch
            {
                queue.retain(|c| c != id);
                done.retain(|c| c != id);
                failed.retain(|c| c != id);
            }
            self.node_textures.remove(id);
            self.touched.remove(id);
            self.open_ctx.retain(|(nid, _)| nid != id);
            if self.tool_menu_for == Some(*id) {
                self.tool_menu_for = None;
            }
            if self.editing_title == Some(*id) {
                self.editing_title = None;
            }
            self.selected.remove(id);
            if self.confirm_delete.is_some_and(|(cid, _)| cid == *id) {
                self.confirm_delete = None;
            }
            if self.linking == Some(*id) {
                self.linking = None;
            }
        }
        // Cleans up all severed input links.
        for n in self.nodes.iter_mut() {
            n.inputs.retain(|c| !gone.contains(c));
        }
        self.prev_open.retain(|id| !gone.contains(id));
        self.link_add_open.retain(|id| !gone.contains(id));
        if self.output_window.is_some_and(|id| gone.contains(&id)) {
            self.output_window = None;
        }
        self.confirm_multi_delete = None;
        self.band_start = None;
        self.status = if removed_count == 1 {
            t(lang, "노드 '{}' 삭제됨.").replace("{}", &removed_title)
        } else {
            t(lang, "{n}개 노드 삭제됨.").replace("{n}", &removed_count.to_string())
        };
        self.mark_dirty();
    }

    /// One context-menu action on a link. Opened over the src→dst edge.
    /// The view only renders the `link_menu_items` list; execution happens here.
    pub(crate) fn apply_link_menu(&mut self, action: LinkMenuAction, lang: Lang) {
        match action {
            LinkMenuAction::Unlink { src, dst } => {
                self.push_undo();
                if let Some(n) = self.nodes.iter_mut().find(|n| n.id == dst) {
                    n.remove_input(src);
                }
                self.mark_dirty();
                self.status = t(lang, "연결을 끊었습니다.").to_string();
            }
            LinkMenuAction::MakeMutual { src, dst } => {
                self.complete_link(dst, src, lang);
            }
            LinkMenuAction::MakeSingle { src, dst } => {
                self.push_undo();
                if let Some(n) = self.nodes.iter_mut().find(|n| n.id == src) {
                    n.remove_input(dst);
                }
                self.mark_dirty();
                self.status = t(lang, "단방향으로 되돌렸습니다.").to_string();
            }
            LinkMenuAction::Run(id) => {
                self.spawn_generate(id);
            }
        }
    }

    /// Context-menu items for the src→dst link. Unlink, mutual toggle, run either end.
    /// Disables the mutual item when the reverse direction would close a long cycle.
    pub(crate) fn link_menu_items(&self, src: usize, dst: usize, lang: Lang) -> Vec<LinkMenuRow> {
        let mut rows = vec![LinkMenuRow::Item(LinkMenuItem {
            label: t(lang, "연결 끊기").to_string(),
            enabled: true,
            tip: None,
            action: LinkMenuAction::Unlink { src, dst },
        })];
        let mutual = self
            .nodes
            .iter()
            .any(|n| n.id == src && n.inputs.contains(&dst));
        if mutual {
            rows.push(LinkMenuRow::Item(LinkMenuItem {
                label: t(lang, "단방향으로 되돌리기").to_string(),
                enabled: true,
                tip: None,
                action: LinkMenuAction::MakeSingle { src, dst },
            }));
        } else {
            let blocked = blocked_cycle(&self.nodes, src, dst);
            rows.push(LinkMenuRow::Item(LinkMenuItem {
                label: t(lang, "양방향으로 만들기").to_string(),
                enabled: !blocked,
                tip: blocked.then(|| t(lang, "순환 연결은 만들 수 없습니다.").to_string()),
                action: LinkMenuAction::MakeMutual { src, dst },
            }));
        }
        rows.push(LinkMenuRow::Sep);
        for (id, key) in [(src, "원본 실행: {t}"), (dst, "대상 실행: {t}")] {
            let node = self.nodes.iter().find(|n| n.id == id);
            let title = node.map(|n| short_title(&n.title)).unwrap_or_default();
            let on = node.is_some_and(|n| n.enabled);
            let busy = self.pending.contains_key(&id);
            let tip = if !on {
                t(lang, "OFF 상태입니다. 켜야 실행됩니다").to_string()
            } else if busy {
                t(lang, "실행 중입니다").to_string()
            } else {
                t(lang, "이 노드만 실행합니다").to_string()
            };
            rows.push(LinkMenuRow::Item(LinkMenuItem {
                label: t(lang, key).replace("{t}", &title),
                enabled: on && !busy,
                tip: Some(tip),
                action: LinkMenuAction::Run(id),
            }));
        }
        rows
    }

    /// Completes a link in one place. Blocks cyclic/duplicate links, connects otherwise.
    /// Duplicate links change nothing, so no snapshot is taken.
    pub(crate) fn complete_link(&mut self, src_id: usize, dst: usize, lang: Lang) {
        let dup = self
            .nodes
            .iter()
            .any(|n| n.id == dst && n.inputs.contains(&src_id));
        if blocked_cycle(&self.nodes, dst, src_id) {
            self.status = t(lang, "순환 연결은 만들 수 없습니다.").to_string();
        } else if dup {
            self.status = t(lang, "이미 연결되어 있습니다.").to_string();
        } else {
            let mutual = is_mutual_only(&self.nodes, dst, src_id);
            self.push_undo();
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == dst) {
                node.add_input(src_id);
            }
            self.status = if mutual {
                t(lang, "양방향으로 연결했습니다.").to_string()
            } else {
                t(lang, "노드를 연결했습니다.").to_string()
            };
            self.mark_dirty();
        }
        self.linking = None;
    }

    /// Delete key: deletes the selection (two-step confirm).
    pub(crate) fn delete_selected(&mut self, lang: Lang) {
        if self.selected.len() == 1 {
            let id = *self.selected.iter().next().unwrap();
            let armed = matches!(self.confirm_delete,
                Some((cid, at)) if cid == id && at.elapsed() <= Duration::from_secs(4));
            if armed {
                self.delete_node(id);
                self.confirm_delete = None;
            } else {
                self.confirm_delete = Some((id, Instant::now()));
                self.status = t(lang, "한 번 더 누르면 삭제됩니다.").to_string();
            }
        } else if !self.selected.is_empty() {
            let mut ids: Vec<usize> = self.selected.iter().copied().collect();
            ids.sort_unstable();
            let armed = matches!(&self.confirm_multi_delete,
                Some((v, at)) if *v == ids && at.elapsed() <= Duration::from_secs(4));
            if armed {
                self.delete_nodes(&ids);
                self.confirm_multi_delete = None;
            } else {
                self.confirm_multi_delete = Some((ids, Instant::now()));
                self.status = t(lang, "선택 {n}개를 지우려면 Delete를 한 번 더 누르세요.")
                    .replace("{n}", &self.selected.len().to_string());
            }
        }
    }

    pub(crate) fn tile_nodes(&mut self) {
        let lang = self.config.lang;
        if self.nodes.is_empty() {
            return;
        }
        self.push_undo();
        // Tiles starting from the top-left of the current view.
        let origin = match self.central_rect {
            Some(r) => {
                let z = self.zoom.max(0.001);
                [
                    (r[0] - self.canvas[0]) / z + 24.0,
                    (r[1] - self.canvas[1]) / z + 24.0,
                ]
            }
            None => [24.0, 24.0],
        };
        let tiles = tile_positions(self.nodes.len(), origin);
        for (node, pos) in self.nodes.iter_mut().zip(tiles) {
            node.pos = pos;
        }
        self.status = t(lang, "노드를 바둑판으로 정렬했습니다.").to_string();
        self.mark_dirty();
    }
}
