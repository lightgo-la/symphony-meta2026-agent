//! Autosave/restore. Saves nodes, settings, and the MCP server list as JSON.
//!
//! **Never stores API keys.** Keys come only from session input (memory) or
//! the `MODEL_API_KEY` env var.

use crate::mcp::McpServerConfig;
use crate::node::PromptNode;

/// LLM provider selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Mock,
    #[default]
    Meta,
}

/// Overall run mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// Runs everything at once.
    Parallel,
    /// Runs one at a time in canvas order.
    Sequential,
    /// Runs one at a time along the input chain (A→B→C).
    #[default]
    Chain,
}

impl RunMode {
    pub fn label(&self, lang: Lang) -> &'static str {
        match self {
            Self::Parallel => t(lang, "다중(병렬)"),
            Self::Sequential => t(lang, "순차"),
            Self::Chain => t(lang, "연결 순서(A→B→C)"),
        }
    }

    pub fn hint(&self, lang: Lang) -> &'static str {
        match self {
            Self::Parallel => t(lang, "켜진 노드를 전부 동시에 실행"),
            Self::Sequential => t(lang, "캔버스 순서대로 하나씩 실행"),
            Self::Chain => t(
                lang,
                "입력 연결을 따라 하나씩 실행, 앞 출력이 뒤 입력으로 전달",
            ),
        }
    }
}

impl Provider {
    pub fn label(&self, lang: Lang) -> &'static str {
        match self {
            Self::Mock => t(lang, "Mock (데모)"),
            Self::Meta => "Meta Muse API",
        }
    }
}

/// App settings persisted to disk. Note: no keys.
use crate::i18n::{t, Lang};
use crate::theme::Theme;

/// Canvas background mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasBg {
    /// Follows the theme palette.
    #[default]
    Theme,
    /// Uses a directly chosen color.
    Custom([u8; 3]),
}

/// Canvas grid style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GridStyle {
    #[default]
    Dots,
    Lines,
    None,
}

impl GridStyle {
    pub fn label(&self, lang: Lang) -> &'static str {
        match self {
            Self::Dots => t(lang, "도트"),
            Self::Lines => t(lang, "선"),
            Self::None => t(lang, "없음"),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub provider: Provider,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub mode: RunMode,
    #[serde(default)]
    pub lang: Lang,
    #[serde(default)]
    pub theme: Theme,
    /// Canvas background (default: theme).
    #[serde(default)]
    pub canvas_bg: CanvasBg,
    /// Grid style (default: dots).
    #[serde(default)]
    pub grid: GridStyle,
    /// Start schedule (default: off).
    #[serde(default)]
    pub schedule: crate::schedule::Schedule,
    /// Map boundary width/height (canvas coordinates).
    #[serde(default = "default_board_w")]
    pub board_w: f32,
    #[serde(default = "default_board_h")]
    pub board_h: f32,
    /// API key saved on this PC. Used only when `remember_key` is on. Stored in plain text.
    #[serde(default)]
    pub saved_key: String,
    /// Saves the API key to the config file.
    #[serde(default)]
    pub remember_key: bool,
    /// Result alarm (Telegram/Discord/WhatsApp) settings.
    #[serde(default)]
    pub alarm: crate::alarm::AlarmConfig,
    /// Output image display height (px). Shared by the result panel, live window, and output window.
    #[serde(default = "default_image_h")]
    pub image_h: f32,
    /// Output font size (px). Shared by the live window and output window.
    #[serde(default = "default_text_size")]
    pub text_size: f32,
    /// Uses a monospace font for output.
    #[serde(default)]
    pub text_mono: bool,
}

fn default_board_w() -> f32 {
    2400.0
}

fn default_board_h() -> f32 {
    1600.0
}

fn default_endpoint() -> String {
    "https://api.meta.ai/v1/chat/completions".to_string()
}

fn default_model() -> String {
    "muse-spark-1.3".to_string()
}

fn default_timeout() -> u64 {
    120
}

fn default_zoom() -> f32 {
    1.0
}

fn default_image_h() -> f32 {
    220.0
}

fn default_text_size() -> f32 {
    14.0
}

impl AppConfig {
    /// Resets API-related values to first-run state.
    /// Connection values (provider, endpoint, model, timeout) go back to defaults; the saved key is cleared.
    /// The session key (memory) is cleared together in `App::reset_api_settings`.
    pub fn reset_api(&mut self) {
        self.provider = Provider::default();
        self.endpoint = default_endpoint();
        self.model = default_model();
        self.timeout_secs = default_timeout();
        self.saved_key.clear();
        self.remember_key = false;
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: Provider::default(),
            endpoint: default_endpoint(),
            model: default_model(),
            timeout_secs: default_timeout(),
            mode: RunMode::default(),
            lang: Lang::default(),
            theme: Theme::default(),
            canvas_bg: CanvasBg::default(),
            grid: GridStyle::default(),
            schedule: crate::schedule::Schedule::default(),
            board_w: default_board_w(),
            board_h: default_board_h(),
            saved_key: String::new(),
            remember_key: false,
            alarm: crate::alarm::AlarmConfig::default(),
            image_h: default_image_h(),
            text_size: default_text_size(),
            text_mono: false,
        }
    }
}

/// Whole save-file structure.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StoredDoc {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub nodes: Vec<PromptNode>,
    #[serde(default)]
    pub next_id: usize,
    #[serde(default)]
    pub config: AppConfig,
    #[serde(default)]
    pub canvas: [f32; 2],
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

impl StoredDoc {
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut doc: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        // Migrates the legacy single input of old saves to multiple inputs.
        for node in &mut doc.nodes {
            if node.inputs.is_empty() {
                if let Some(src) = node.input_legacy.take() {
                    node.inputs.push(src);
                }
            } else {
                node.input_legacy = None;
            }
        }
        sanitize_doc(&mut doc);
        Ok(doc)
    }
}

/// Repairs a loaded document: drops duplicate ids (keeping the first), restores next_id.
/// Dangling-input cleanup happens at the restore point (restore/apply_doc).
fn sanitize_doc(doc: &mut StoredDoc) {
    let mut seen = std::collections::HashSet::new();
    doc.nodes.retain(|n| seen.insert(n.id));
    let max_id = doc.nodes.iter().map(|n| n.id).max().unwrap_or(0);
    doc.next_id = doc.next_id.max(max_id.saturating_add(1)).max(1);
    if !doc.zoom.is_finite() {
        doc.zoom = default_zoom();
    }
    for axis in doc.canvas.iter_mut() {
        if !axis.is_finite() {
            *axis = 0.0;
        }
    }
}

/// Save-file path. `None` means the OS could not provide a path.
pub fn store_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("com", "PromptCanvas", "prompt_canvas_gui")
        .map(|dirs| dirs.config_dir().join("prompt_canvas_gui.json"))
}

pub fn save_to(path: &std::path::Path, doc: &StoredDoc) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = doc.to_json()?;
    // First writes to a temp file in the same folder and flushes it to disk.
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    // Copies the existing file as a previous-generation backup (a copy, not a move,
    // so the window where the main file could vanish on power loss is narrower).
    if path.exists() {
        let bak = backup_path(path);
        let _ = std::fs::copy(path, &bak);
        // Windows rename fails when the target exists, so remove it first.
        let _ = std::fs::remove_file(path);
    }
    // Moves the temp file into place. Restores from backup on failure.
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        if !path.exists() {
            let bak = backup_path(path);
            let _ = std::fs::rename(&bak, path);
        }
        e.to_string()
    })
}

/// Previous-generation backup path ("...json" → "...json.bak").
pub fn backup_path(path: &std::path::Path) -> std::path::PathBuf {
    path.with_extension("json.bak")
}

pub fn load_from(path: &std::path::Path) -> Result<StoredDoc, String> {
    // Falls back to the previous backup when the main file is missing or corrupt.
    let main = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| StoredDoc::from_json(&text));
    match main {
        Ok(doc) => Ok(doc),
        Err(first) => {
            let bak = backup_path(path);
            let btext = std::fs::read_to_string(&bak).map_err(|_| first)?;
            StoredDoc::from_json(&btext)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ContextBlock;

    #[test]
    fn roundtrip_preserves_opt_in_key() {
        let mut node = PromptNode::new(1, "t".to_string(), "p".to_string(), true, [5.0, 6.0]);
        node.context.push(ContextBlock::new("srv", "ctx"));
        let cfg = AppConfig {
            saved_key: "EEE".to_string(),
            remember_key: true,
            ..AppConfig::default()
        };
        let doc = StoredDoc {
            version: 1,
            nodes: vec![node],
            next_id: 2,
            config: cfg,
            canvas: [1.0, 2.0],
            zoom: 1.5,
            mcp_servers: vec![McpServerConfig {
                name: "s".to_string(),
                command: "npx".to_string(),
                args: vec!["-y".to_string()],
            }],
        };
        let json = doc.to_json().unwrap();
        let back = StoredDoc::from_json(&json).unwrap();
        assert_eq!(back.config.saved_key, "EEE");
        assert!(back.config.remember_key);
        assert_eq!(back.nodes.len(), 1);
        assert_eq!(back.nodes[0].pos, [5.0, 6.0]);
        assert_eq!(back.nodes[0].context.len(), 1);
        assert_eq!(back.zoom, 1.5);
        assert_eq!(back.mcp_servers.len(), 1);
        assert_eq!(back.canvas, [1.0, 2.0]);
        // Defaults are empty key + remember-off, so old saves gain no key either.
        assert!(AppConfig::default().saved_key.is_empty());
        assert!(!AppConfig::default().remember_key);
    }

    #[test]
    fn save_is_atomic_and_leaves_no_tmp() {
        let dir = std::env::temp_dir().join(format!("pc_save_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        let mut doc = StoredDoc {
            next_id: 7,
            ..Default::default()
        };
        // Saving twice leaves no temp file and the latest content is read.
        save_to(&path, &doc).unwrap();
        doc.next_id = 8;
        save_to(&path, &doc).unwrap();
        let back = load_from(&path).unwrap();
        assert_eq!(back.next_id, 8);
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(leftovers.is_empty(), "임시 파일이 남음: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_single_input_migrates_to_inputs() {
        let doc = StoredDoc::from_json(
            r#"{"nodes":[{"id":1,"title":"a","prompt":"p","output":"","enabled":true,"run_count":0,"pos":[0.0,0.0],"input":7}],"next_id":2}"#,
        )
        .unwrap();
        assert_eq!(doc.nodes[0].inputs, vec![7]);
        assert!(doc.nodes[0].input_legacy.is_none());
    }

    #[test]
    fn backup_keeps_previous_generation() {
        let dir = std::env::temp_dir().join(format!("pc_bak_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        let mut first = StoredDoc {
            next_id: 1,
            ..Default::default()
        };
        save_to(&path, &first).unwrap();
        first.next_id = 2;
        save_to(&path, &first).unwrap();
        // The main file is the latest; the backup is the previous generation.
        assert_eq!(load_from(&path).unwrap().next_id, 2);
        let bak = backup_path(&path);
        let bak_doc = load_from(&bak).unwrap();
        assert_eq!(bak_doc.next_id, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_main_falls_back_to_backup() {
        let dir = std::env::temp_dir().join(format!("pc_corrupt_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        let doc = StoredDoc {
            next_id: 9,
            ..Default::default()
        };
        save_to(&path, &doc).unwrap();
        save_to(&path, &doc).unwrap();
        // Corrupting the main file recovers from the backup.
        std::fs::write(&path, "{깨짐").unwrap();
        assert_eq!(load_from(&path).unwrap().next_id, 9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_mode_is_chain_and_roundtrips() {
        assert_eq!(AppConfig::default().mode, RunMode::Chain);
        let cfg = AppConfig {
            mode: RunMode::Sequential,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("sequential"));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mode, RunMode::Sequential);
    }

    #[test]
    fn old_or_empty_files_fall_back_to_defaults() {
        let doc = StoredDoc::from_json("{}").unwrap();
        assert!(doc.nodes.is_empty());
        assert_eq!(doc.config.model, "muse-spark-1.3");
        assert_eq!(
            doc.config.endpoint,
            "https://api.meta.ai/v1/chat/completions"
        );
    }

    #[test]
    fn missing_main_falls_back_to_backup() {
        let dir = std::env::temp_dir().join(format!("pc_missing_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        let doc = StoredDoc {
            next_id: 4,
            ..Default::default()
        };
        save_to(&path, &doc).unwrap();
        save_to(&path, &doc).unwrap();
        // Even a wholly missing main file recovers from the backup.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(load_from(&path).unwrap().next_id, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_repairs_duplicate_ids_and_next_id() {
        let doc = StoredDoc::from_json(
            r#"{"nodes":[
                {"id":1,"title":"a","prompt":"p","output":"","enabled":true,"run_count":0,"pos":[0.0,0.0]},
                {"id":1,"title":"dup","prompt":"q","output":"","enabled":true,"run_count":0,"pos":[9.0,9.0]},
                {"id":5,"title":"b","prompt":"r","output":"","enabled":true,"run_count":0,"pos":[1.0,1.0]}
            ],"next_id":2}"#,
        )
        .unwrap();
        // Duplicates keep only the first; next_id is restored to max id+1 or more.
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0].title, "a");
        assert_eq!(doc.next_id, 6);
    }

    #[test]
    fn canvas_bg_and_grid_roundtrip() {
        let cfg = AppConfig {
            canvas_bg: CanvasBg::Custom([10, 20, 30]),
            grid: GridStyle::Lines,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.canvas_bg, CanvasBg::Custom([10, 20, 30]));
        assert_eq!(back.grid, GridStyle::Lines);
        // Old saves lack both fields, so they load as defaults.
        let old: AppConfig = serde_json::from_str("{\"mode\":\"chain\"}").unwrap();
        assert_eq!(old.canvas_bg, CanvasBg::Theme);
        assert_eq!(old.grid, GridStyle::Dots);
    }

    #[test]
    fn reset_api_restores_first_run_values() {
        let mut cfg = AppConfig {
            provider: Provider::Mock,
            endpoint: "http://localhost:1".to_string(),
            model: "custom".to_string(),
            timeout_secs: 33,
            saved_key: "KEEP-ME-NOT".to_string(),
            remember_key: true,
            ..AppConfig::default()
        };
        cfg.reset_api();
        let fresh = AppConfig::default();
        assert_eq!(cfg.provider, fresh.provider);
        assert_eq!(cfg.endpoint, fresh.endpoint);
        assert_eq!(cfg.model, fresh.model);
        assert_eq!(cfg.timeout_secs, fresh.timeout_secs);
        assert!(cfg.saved_key.is_empty());
        assert!(!cfg.remember_key);
        // Values outside the API are untouched.
        assert_eq!(cfg.mode, fresh.mode);
    }
}
