//! Prompt Canvas — node-based multi-prompt canvas (Meta Muse API + MCP support).
//!
//! The API key is saved to disk only when '이 PC에 키 저장' is on.
//! When off, only the settings-window session key (memory) or the `MODEL_API_KEY` env var is used.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod alarm;
mod app;
mod blueprint;
mod cards;
mod fonts;
mod http;
mod i18n;
mod llm;
mod mcp;
mod media;
mod node;
mod panels;
mod run;
mod schedule;
mod share;
mod state;
mod store;
mod theme;
#[cfg(test)]
mod usability;
mod zoom;

fn app_icon() -> std::sync::Arc<eframe::egui::IconData> {
    let bytes = include_bytes!("../assets/icon.png");
    match eframe::icon_data::from_png_bytes(bytes) {
        Ok(icon) => std::sync::Arc::new(icon),
        Err(e) => {
            eprintln!("window icon skipped: {e}");
            std::sync::Arc::new(eframe::egui::IconData::default())
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Symphony Canvas",
        options,
        Box::new(|cc| {
            let font_name = fonts::install(&cc.egui_ctx);
            let mut app = app::App::new(cc);
            app.note_font(&font_name);
            Ok(Box::new(app))
        }),
    )
}
