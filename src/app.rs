//! Update orchestration. Calls state, run, panels, and cards.

use eframe::egui;

use std::time::Duration;

use crate::i18n::t;
use crate::node::{
    blocked_cycle, chain_pairs, in_port, input_port_target, is_mutual_only, out_port, PORT_HIT,
};
pub use crate::state::App;
use crate::zoom::{anchored_canvas, clamp_zoom, point_in_rect, step_zoom_at_point};

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let _lang = self.config.lang;
        use crate::theme::*;

        let lang = self.config.lang;

        self.poll_jobs();

        // ── Port drag-linking: press/drop detection (judged before cards) ──
        let pointer_down = ctx.input(|i| i.pointer.primary_down());
        let hover_now = ctx.pointer_hover_pos();
        let mut press_on_port = false;
        if pointer_down && !self.was_down {
            // Fresh press: start linking when over an output port
            if let Some(p) = hover_now {
                // Check overlapping cards topmost-first (from the back of the vector).
                for node in self.nodes.iter().rev() {
                    if let Some(r) = node.last_rect {
                        let op = out_port(r);
                        let port = egui::Pos2::new(op[0], op[1]);
                        if p.distance(port) < PORT_HIT {
                            self.linking = Some(node.id);
                            self.press_pos = Some(p);
                            press_on_port = true;
                            self.status =
                                t(lang, "입력 동그라미를 클릭하거나 타이틀바를 클릭하세요.")
                                    .to_string();
                            break;
                        }
                    }
                }
            }
        }
        if !pointer_down && self.was_down {
            // Release: if it was a drag (moved 6px or more), try linking at the drop position
            if let (Some(src_id), Some(press), Some(p)) =
                (self.linking, self.press_pos, ctx.pointer_latest_pos())
            {
                if press.distance(p) > 6.0 {
                    let mut target: Option<usize> = None;
                    // Priority 1: another node's input port (topmost card first)
                    for node in self.nodes.iter().rev() {
                        if node.id == src_id {
                            continue;
                        }
                        if let Some(r) = node.last_rect {
                            let ip = in_port(r);
                            if p.distance(egui::Pos2::new(ip[0], ip[1])) < PORT_HIT + 6.0 {
                                target = Some(node.id);
                                break;
                            }
                        }
                    }
                    // Priority 2: another node's card body (topmost card first)
                    if target.is_none() {
                        for node in self.nodes.iter().rev() {
                            if node.id == src_id {
                                continue;
                            }
                            if let Some(r) = node.last_rect {
                                let rect = egui::Rect::from_min_max(
                                    egui::Pos2::new(r[0], r[1]),
                                    egui::Pos2::new(r[2], r[3]),
                                );
                                if rect.contains(p) {
                                    target = Some(node.id);
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(dst) = target {
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
                    } else {
                        self.status = t(lang, "연결 모드를 취소했습니다.").to_string();
                    }
                    self.linking = None;
                } else if let Some(dst) = input_port_target(&self.nodes, src_id, [p.x, p.y]) {
                    // Finished with a click over an input port: complete the link
                    // (output port click → input port click).
                    self.complete_link(src_id, dst, lang);
                }
                self.press_pos = None;
            } else if let (Some(src_id), Some(p)) = (self.linking, ctx.pointer_latest_pos()) {
                // Ended with a click while armed via a button or similar: complete the link if over an input port.
                if let Some(dst) = input_port_target(&self.nodes, src_id, [p.x, p.y]) {
                    self.complete_link(src_id, dst, lang);
                }
                self.press_pos = None;
            } else {
                self.press_pos = None;
            }
        }

        // ── Global shortcuts (untouched while typing) ──
        if !ctx.wants_keyboard_input() {
            let ctrl = ctx.input(|i| i.modifiers.ctrl);
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_batch();
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::N)) {
                self.add_node(
                    t(lang, "프롬프트 {}").replace("{}", &self.next_id.to_string()),
                    String::new(),
                    true,
                );
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::S)) {
                self.save_now();
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::O)) {
                self.open_file_dialog();
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
                self.undo();
            }
            if ctrl
                && (ctx.input(|i| i.key_pressed(egui::Key::Y))
                    || (ctx.input(|i| i.modifiers.shift)
                        && ctx.input(|i| i.key_pressed(egui::Key::Z))))
            {
                self.redo();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.editing_title.is_none() {
                // In link mode only cancel the link; when running, cancel everything.
                if self.linking.is_some() {
                    self.linking = None;
                    self.status = t(lang, "연결 모드를 취소했습니다.").to_string();
                } else if !self.pending.is_empty() || self.batch.is_active() {
                    self.cancel_all();
                } else {
                    // Clear the selection only when there is nothing else to cancel.
                    self.selected.clear();
                    self.band_start = None;
                }
            }
            // Right-click also cancels link mode.
            if ctx.input(|i| i.pointer.secondary_clicked()) && self.linking.is_some() {
                self.linking = None;
                self.status = t(lang, "연결 모드를 취소했습니다.").to_string();
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::R)) {
                self.results_open = !self.results_open;
            }
            // CAD-style zoom shortcuts: Ctrl+Plus/Minus, Ctrl+0 reset
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Plus)) {
                let center = self.zoom_center();
                let (canvas, zoom) = step_zoom_at_point(self.canvas, self.zoom, 1.2, center);
                self.canvas = canvas;
                self.zoom = zoom;
                self.mark_dirty();
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
                let center = self.zoom_center();
                let (canvas, zoom) = step_zoom_at_point(self.canvas, self.zoom, 1.0 / 1.2, center);
                self.canvas = canvas;
                self.zoom = zoom;
                self.mark_dirty();
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Num0)) {
                self.zoom = 1.0;
                self.mark_dirty();
            }
            // F: jump the view to the selected node (smallest id when several)
            if !ctrl && ctx.input(|i| i.key_pressed(egui::Key::F)) {
                if let Some(&sel) = self.selected.iter().min() {
                    self.focus_node(sel);
                }
            }
            // Delete: delete selection (two-step confirm). Ctrl+D: clone selection.
            if !ctrl && ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.delete_selected(lang);
            }
            if ctrl && ctx.input(|i| i.key_pressed(egui::Key::D)) {
                let mut ids: Vec<usize> = self.selected.iter().copied().collect();
                ids.sort_unstable();
                for id in ids {
                    self.clone_node(id);
                }
            }
            // L: link 2+ selected nodes in id order (link without buttons).
            if !ctrl && ctx.input(|i| i.key_pressed(egui::Key::L)) {
                let ids: Vec<usize> = self.selected.iter().copied().collect();
                if ids.len() < 2 {
                    self.status = t(lang, "연결할 노드를 2개 이상 선택하세요.").to_string();
                } else {
                    let mut planned: Vec<(usize, usize)> = Vec::new();
                    for (src, dst) in chain_pairs(ids) {
                        let exists = self
                            .nodes
                            .iter()
                            .any(|n| n.id == dst && n.inputs.contains(&src));
                        if !exists && !blocked_cycle(&self.nodes, dst, src) {
                            planned.push((src, dst));
                        }
                    }
                    if planned.is_empty() {
                        self.status = t(lang, "더 연결할 게 없습니다.").to_string();
                    } else {
                        self.push_undo();
                        for (src, dst) in &planned {
                            if let Some(n) = self.nodes.iter_mut().find(|n| n.id == *dst) {
                                n.add_input(*src);
                            }
                        }
                        self.status = t(lang, "{n}개 연결했습니다.")
                            .replace("{n}", &planned.len().to_string());
                        self.mark_dirty();
                    }
                }
            }
            // Arrow keys pan the canvas (larger step with Shift)
            {
                let pan = if ctx.input(|i| i.modifiers.shift) {
                    200.0
                } else {
                    40.0
                };
                let mut moved = false;
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    self.canvas[0] += pan;
                    moved = true;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    self.canvas[0] -= pan;
                    moved = true;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    self.canvas[1] += pan;
                    moved = true;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    self.canvas[1] -= pan;
                    moved = true;
                }
                if moved {
                    self.mark_dirty();
                }
            }
        }
        // ── Pinch / Ctrl+wheel smooth zoom (cursor-anchored, excluding overlays) ──
        // Kept outside the keyboard gate so it works while typing.
        // Plain wheel (raw) is handled on the card side and left alone here.
        {
            let zf = ctx.input(|i| i.zoom_delta());
            if (zf - 1.0).abs() > 0.0001 {
                let anchor = ctx
                    .pointer_hover_pos()
                    .map_or_else(|| self.zoom_center(), |m| [m.x, m.y]);
                let over_overlay = ctx.pointer_hover_pos().is_some_and(|m| {
                    self.overlay_rects
                        .iter()
                        .any(|&r| point_in_rect([m.x, m.y], r))
                });
                let in_canvas = ctx.pointer_hover_pos().is_some_and(|m| {
                    self.central_rect
                        .is_some_and(|r| m.x >= r[0] && m.x <= r[2] && m.y >= r[1] && m.y <= r[3])
                });
                if in_canvas && !over_overlay {
                    let new_zoom = clamp_zoom(self.zoom * zf);
                    self.canvas = anchored_canvas(self.canvas, anchor, self.zoom, new_zoom);
                    self.zoom = new_zoom;
                    self.mark_dirty();
                }
                // Request repaint so residual smoothing applies on the next frame too.
                ctx.request_repaint();
            }
        }
        // ── File drag-and-drop: attach images to nodes ──
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        let drop_center = match self.central_rect {
            Some(r) => egui::Pos2::new((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0),
            None => egui::Pos2::new(600.0, 350.0),
        };
        for file in &dropped {
            if let Some(path) = file.path.as_ref().and_then(|p| p.to_str()) {
                let lower = path.to_lowercase();
                let is_img = ["png", "jpg", "jpeg", "webp", "bmp", "gif"]
                    .iter()
                    .any(|e| lower.ends_with(&format!(".{e}")));
                if !is_img {
                    continue;
                }
                let pos = ctx.pointer_hover_pos().unwrap_or(drop_center);
                let hit = self.nodes.iter().find(|n| {
                    n.last_rect.is_some_and(|r| {
                        pos.x >= r[0] && pos.x <= r[2] && pos.y >= r[1] && pos.y <= r[3]
                    })
                });
                if let Some(target) = hit.map(|n| n.id) {
                    self.push_undo();
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == target) {
                        node.image_path = Some(path.to_string());
                    }
                    self.status = t(lang, "대표 이미지를 설정했습니다.").to_string();
                    self.mark_dirty();
                } else {
                    self.push_undo();
                    let id = self.next_id;
                    self.add_node(
                        t(lang, "프롬프트 {}").replace("{}", &id.to_string()),
                        String::new(),
                        true,
                    );
                    // Move the just-created node to the drop point.
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                        node.pos = [
                            (pos.x - self.canvas[0]) / self.zoom,
                            (pos.y - self.canvas[1]) / self.zoom,
                        ];
                        node.image_path = Some(path.to_string());
                    }
                    self.status = t(lang, "이미지 노드를 만들었습니다.").to_string();
                    self.mark_dirty();
                }
            }
        }

        self.render_menu(ctx);
        self.render_results(ctx);
        self.render_canvas(ctx, press_on_port);
        self.render_minimap(ctx);
        self.render_result_tab(ctx);
        self.render_cards(ctx);
        self.render_result_jump_layer(ctx);
        self.render_tool_window(ctx);
        self.render_blueprint_window(ctx);
        self.render_settings(ctx);
        self.render_about(ctx);
        self.render_new_confirm(ctx);
        self.render_combined_zoom(ctx);
        // ── OS-independent windows: same Context, so the main theme applies as-is ──
        if self.show_results_window {
            ctx.show_viewport_immediate(
                egui::ViewportId(egui::Id::new("results_window")),
                egui::ViewportBuilder::default()
                    .with_title(t(lang, "라이브 결과"))
                    .with_inner_size([560.0, 460.0]),
                |vctx, _class| {
                    let pal = palette(self.config.theme);
                    egui::CentralPanel::default()
                        .frame(hud_panel_frame(pal.result, pal.accent))
                        .show(vctx, |ui| {
                            self.render_results_window(vctx, ui);
                        });
                    if vctx.input(|i| i.viewport().close_requested()) {
                        self.show_results_window = false;
                    }
                },
            );
        }
        if self.output_window.is_some() {
            let title = self
                .nodes
                .iter()
                .find(|n| Some(n.id) == self.output_window)
                .map(|n| n.title.clone())
                .unwrap_or_default();
            ctx.show_viewport_immediate(
                egui::ViewportId(egui::Id::new("node_output_window")),
                egui::ViewportBuilder::default()
                    .with_title(title)
                    .with_inner_size([520.0, 420.0]),
                |vctx, _class| {
                    let pal = palette(self.config.theme);
                    egui::CentralPanel::default()
                        .frame(hud_panel_frame(pal.window, pal.accent))
                        .show(vctx, |ui| {
                            self.render_output_window(vctx, ui);
                        });
                    if vctx.input(|i| i.viewport().close_requested()) {
                        self.output_window = None;
                    }
                },
            );
        }
        // Finalize the overlay rects collected this frame.
        // The next frame's wheel-zoom check reads them.
        self.overlay_rects = std::mem::take(&mut self.overlay_staging);

        // ── Bottom status bar ─────────────────────────────────────
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(subtle(t(lang, "상태:")));
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(subtle(if self.dirty {
                        t(lang, "저장 중…")
                    } else {
                        t(lang, "저장됨")
                    }));
                });
            });
        });

        if self.linking.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if !self.pending.is_empty()
            || self.test_job.is_some()
            || self.alarm_job.is_some()
            || self.blueprint_job.is_some()
            || self.linking.is_some()
        {
            ctx.request_repaint();
        }
        // Delete/clear confirm arming expires after 4 seconds.
        if let Some((_, at)) = self.confirm_delete {
            if at.elapsed() > Duration::from_secs(4) {
                self.confirm_delete = None;
            }
        }
        if let Some((_, at)) = &self.confirm_multi_delete {
            if at.elapsed() > Duration::from_secs(4) {
                self.confirm_multi_delete = None;
            }
        }
        if let Some(at) = self.confirm_clear {
            if at.elapsed() > Duration::from_secs(4) {
                self.confirm_clear = None;
            }
        }
        if let Some(at) = self.confirm_api_reset {
            if at.elapsed() > Duration::from_secs(4) {
                self.confirm_api_reset = None;
            }
        }
        self.was_down = ctx.input(|i| i.pointer.primary_down());
        // When scheduling is on, wake periodically to check the time.
        let any_sched = self.config.schedule.enabled || self.nodes.iter().any(|n| n.sched.enabled);
        if any_sched {
            ctx.request_repaint_after(std::time::Duration::from_secs(20));
        }
        // Start schedule: trigger a batch run when the time arrives.
        // If a batch is already running, defer to the next tick without consuming the schedule.
        let batch_running = matches!(self.batch, crate::state::BatchState::Running { .. });
        if !batch_running && self.config.schedule.tick(chrono::Local::now()) {
            self.status = t(lang, "예약 시각이 되어 전체 실행을 시작합니다.").to_string();
            self.run_batch();
        }
        // Per-node schedules: run due nodes one at a time.
        self.tick_node_schedules();
        self.autosave();
        crate::run::log_slow_frame(self.nodes.len(), self.pending.len());
    }

    /// Exit/periodic save hook: flush to disk only when dirty.
    /// Autosave uses a 0.5s debounce, so it can miss last-moment settings.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if self.dirty {
            self.save_now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{rects_overlap, NodeMode};
    use crate::run::{JobKind, JobResult, PendingJob};

    fn headless_app(ctx: &egui::Context) -> (App, eframe::Frame) {
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc);
        // Use a deterministic start state regardless of saved user state.
        app.nodes.clear();
        app.next_id = 1;
        app.selected.clear();
        app.scroll_zones.clear();
        app.zoom = 1.0;
        app.canvas = [0.0, 0.0];
        app.show_minimap = false;
        app.results_open = false;
        // Use the default board size regardless of disk (deterministic tests).
        let board = crate::store::AppConfig::default();
        app.config.board_w = board.board_w;
        app.config.board_h = board.board_h;
        (app, eframe::Frame::_new_kittest())
    }

    fn frame_input(screen: egui::Rect, events: Vec<egui::Event>) -> egui::RawInput {
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        raw.events = events;
        raw
    }

    /// Headless wheel-zoom repro: wheeling up over empty canvas center must zoom in.
    /// If this breaks, wheel zoom is broken in the real app too.
    #[test]
    fn wheel_over_empty_canvas_zooms() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1100.0, 700.0));
        let at = egui::pos2(600.0, 350.0);
        // Frame 1: hover to finalize central_rect.
        let _ = ctx.run(
            frame_input(screen, vec![egui::Event::PointerMoved(at)]),
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );
        assert!(app.central_rect.is_some());
        // Frame 2: one wheel notch up at the same position.
        let _ = ctx.run(
            frame_input(
                screen,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, 1.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            ),
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
            },
        );
        assert!(
            app.zoom > 1.0,
            "wheel over empty canvas should zoom in, got {}",
            app.zoom
        );
    }

    fn screen_rect() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1100.0, 700.0))
    }

    fn run_frame(
        app: &mut App,
        frame: &mut eframe::Frame,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(frame_input(screen_rect(), events), |ctx| {
            eframe::App::update(app, ctx, frame);
        });
    }

    /// Short prompts create no scroll, so they must not block wheel zoom.
    #[test]
    fn short_prompt_registers_no_scroll_zone() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.add_node("짧음".to_string(), "hi".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let zones = app.scroll_zones.get(&1).cloned().unwrap_or_default();
        assert!(
            zones.is_empty(),
            "short prompt should not block wheel zoom, got {zones:?}"
        );
    }

    /// Over long prompts, skip zooming to keep content scrolling alive.
    #[test]
    fn long_prompt_keeps_scroll_zone() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        let long = (1..=30)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        app.add_node("긺".to_string(), long, true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let zones = app.scroll_zones.get(&1).cloned().unwrap_or_default();
        assert!(
            !zones.is_empty(),
            "long prompt should keep a scroll zone for content scrolling"
        );
    }

    /// Wheel over a card title bar (outside scroll areas) must zoom.
    #[test]
    fn wheel_over_card_chrome_zooms() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        // Finalize the screen first so the new card spawns inside the viewport.
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let at = egui::pos2((r[0] + r[2]) / 2.0, r[1] + 16.0);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, 1.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(
            app.zoom > 1.0,
            "wheel over card chrome should zoom, got {}",
            app.zoom
        );
    }

    /// At 2x zoom the card height must grow too (text gets bigger).
    #[test]
    fn content_scales_with_zoom() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.add_node(
            "카드".to_string(),
            "첫 줄\n둘째 줄\n셋째 줄".to_string(),
            true,
        );
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let h1 = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let h1 = h1[3] - h1[1];
        app.zoom = 2.0;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let h2 = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let h2 = h2[3] - h2[1];
        assert!(
            h2 > h1 * 1.3,
            "card content should scale with zoom: h1={h1}, h2={h2}"
        );
    }

    /// Ctrl+wheel zoom must work even while typing.
    /// Focus the prompt box directly (same state as clicked) and wheel.
    #[test]
    fn ctrl_wheel_zooms_while_typing() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        // Finalize the screen first so the new card spawns inside the viewport.
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let at = egui::pos2((r[0] + r[2]) / 2.0, r[1] + 16.0);
        ctx.memory_mut(|m| m.request_focus(egui::Id::new(("card-prompt", 1))));
        assert!(
            ctx.wants_keyboard_input(),
            "prompt box should hold keyboard focus"
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, 1.0),
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(
            app.zoom > 1.0,
            "ctrl+wheel should zoom while typing, got {}",
            app.zoom
        );
    }

    /// Unlimited map: nodes at off-screen coordinates must stay in place.
    #[test]
    fn node_keeps_remote_position() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.add_node("원격".to_string(), "프롬프트".to_string(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().pos = [5000.0, -3000.0];
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let pos = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert_eq!(
            pos,
            [5000.0, -3000.0],
            "remote node must not be pulled back"
        );
        // Rendered position must also be remote (must not be pulled to the screen edge).
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        assert!(
            (r[0] - 5000.0).abs() < 1.0 && (r[1] + 3000.0).abs() < 1.0,
            "remote node must render remote, got {r:?}"
        );
    }

    /// Dragging a card under the left panel must leave a 60px grip.
    #[test]
    fn drag_node_keeps_grip_at_panel() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Empty spot at the bar's right end (the center is covered by the collapse button).
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        // Hover priming: a press routes to the last hovered target.
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        let sx0 = r[0];
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        // Drag under the left panel (x=50).
        let target = egui::pos2(50.0, bar.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(target)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(target), click(target, false)],
        );
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        let z = app.zoom.max(0.001);
        let sx = node.pos[0] * z + app.canvas[0];
        let cr = app.central_rect.unwrap();
        let w = node.last_rect.map(|r| r[2] - r[0]).unwrap_or(320.0);
        assert!(
            sx < sx0 - 50.0,
            "card should actually move left when dragged, {sx0} -> {sx}"
        );
        // Stop leaving only a 60px grip: past the edge but not fully.
        assert!(
            sx >= cr[0] - w + 60.0 - 1.0,
            "dragged card should keep a 60px grip, screen x={sx}, edge={}",
            cr[0]
        );
        assert!(
            sx <= cr[0] + 1.0,
            "card should park past the edge instead of pinning at it, {sx} vs {}",
            cr[0]
        );
    }

    /// A card panned under the menu must ignore bar drags.
    #[test]
    fn panned_node_under_menu_ignores_bar_drag() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("판".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let c = app.central_rect.unwrap();
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Pan left so the bar's empty right spot lands inside the menu (edge-20).
        app.canvas[0] += (c[0] - 20.0) - (r[2] - 40.0);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        assert!(
            bar.x < c[0],
            "bar should sit over the menu, bar={bar:?} central={c:?}"
        );
        let before = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        let to = egui::pos2(bar.x + 30.0, bar.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let after = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert_eq!(before, after, "card under menu must not grab");
        assert!(
            !app.selected.contains(&1),
            "menu clicks must not select a hidden card"
        );
    }

    /// Corner resize must stop at the right panel edge.
    #[test]
    fn corner_resize_stops_at_right_panel() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let c = app.central_rect.unwrap();
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Place the right edge 40px before the boundary.
        let w = r[2] - r[0];
        let z = app.zoom.max(0.001);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().pos[0] =
            (c[2] - 40.0 - w - app.canvas[0]) / z;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let w0 = r[2] - r[0];
        assert!(r[2] < c[2], "setup should start inside, {r:?} vs {c:?}");
        let at = egui::pos2(r[2] - 16.0, r[3] - 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at), click(at, true)],
        );
        let to = egui::pos2(at.x + 200.0, at.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        assert!(
            r[2] <= c[2] + 1.0,
            "resized card must stop at panel edge, {r:?} vs {c:?}"
        );
        assert!(
            r[2] - r[0] > w0,
            "resize should still grow until the edge, {w0} -> {}",
            r[2] - r[0]
        );
    }

    /// A card near a panel edge must not snap to the wall as soon as grabbed.
    #[test]
    fn dragged_card_near_right_panel_is_not_pinned() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let c = app.central_rect.unwrap();
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Place the right edge 100px past the boundary (state after opening results).
        let w = r[2] - r[0];
        let z = app.zoom.max(0.001);
        let x0 = (c[2] + 100.0 - w - app.canvas[0]) / z;
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().pos[0] = x0;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // The bar's empty left spot stays visible inside central, so it can be grabbed.
        let bar = egui::pos2(r[0] + 30.0, r[1] + 16.0);
        assert!(
            bar.x < c[2],
            "bar grab spot should stay visible, bar={bar:?} central={c:?}"
        );
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        let to = egui::pos2(bar.x + 30.0, bar.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let x1 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos[0];
        assert!(
            (x1 - (x0 + 30.0 / z)).abs() < 3.0,
            "drag should follow pointer, not pin to wall: {x0} -> {x1}"
        );
    }

    /// A card spawned at a far-panned spot must not yank to the board when grabbed.
    #[test]
    fn grab_remote_spawned_card_does_not_yank_to_board() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.canvas = [-5000.0, -3000.0];
        app.add_node("원격".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let pos0 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert!(
            pos0[0] > app.config.board_w,
            "setup should spawn outside board, pos={pos0:?}"
        );
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let z = app.zoom.max(0.001);
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        let to = egui::pos2(bar.x + 30.0, bar.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let pos1 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert!(
            (pos1[0] - (pos0[0] + 30.0 / z)).abs() < 3.0 && (pos1[1] - pos0[1]).abs() < 3.0,
            "remote drag should follow pointer: {pos0:?} -> {pos1:?}"
        );
    }

    /// A card above the board (negative Y) must drag freely upward.
    #[test]
    fn drag_up_from_above_board_moves_freely() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.canvas = [0.0, 3000.0];
        app.add_node("위쪽".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let pos0 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert!(
            pos0[1] < 0.0,
            "setup should spawn above board, pos={pos0:?}"
        );
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let z = app.zoom.max(0.001);
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        let to = egui::pos2(bar.x, bar.y - 30.0);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let pos1 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert!(
            (pos1[0] - pos0[0]).abs() < 3.0 && (pos1[1] - (pos0[1] - 30.0 / z)).abs() < 3.0,
            "drag up should follow pointer: {pos0:?} -> {pos1:?}"
        );
    }

    /// Clicking a per-node output row in the results window must jump to that node.
    #[test]
    fn result_row_click_jumps_to_node() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("첫째".to_string(), "프롬프트".to_string(), true);
        app.add_node("둘째".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        // Put the second node off-screen.
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().pos = [5000.0, 5000.0];
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.results_open = true;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let row = *app.result_row_rects.get(&2).expect("row rect recorded");
        let at = egui::pos2((row[0] + row[2]) / 2.0, (row[1] + row[3]) / 2.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at), click(at, true)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at), click(at, false)],
        );
        assert!(
            app.selected.len() == 1 && app.selected.contains(&2),
            "jump should select node 2, sel={:?}",
            app.selected
        );
        // After the jump, the card must be centered on the next frame.
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let c = app.central_rect.unwrap();
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 2)
            .unwrap()
            .last_rect
            .unwrap();
        let (ccx, ccy) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
        let (ncx, ncy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        assert!(
            (ncx - ccx).abs() < 2.0 && (ncy - ccy).abs() < 2.0,
            "node 2 should be centered: card=({ncx}, {ncy}) central=({ccx}, {ccy})"
        );
    }

    /// Starting new must restore the first screen (3 samples) and keep undo.
    #[test]
    fn reset_workspace_restores_fresh_first_screen() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("작업".to_string(), "p".to_string(), true);
        app.add_node("작업2".to_string(), "p".to_string(), true);
        app.nodes[0].output = "out".to_string();
        app.selected.insert(1);
        app.canvas = [100.0, 200.0];
        app.zoom = 2.0;
        app.results_open = false;
        app.reset_workspace();
        assert_eq!(app.nodes.len(), 3);
        assert_eq!(app.nodes[0].title, "아이디어 1");
        assert!(!app.nodes[2].enabled);
        assert_eq!(app.next_id, 4);
        assert!(app.selected.is_empty());
        assert_eq!(app.canvas, [0.0, 0.0]);
        assert_eq!(app.zoom, 1.0);
        assert!(app.results_open);
        assert_eq!(app.undo.len(), 1, "reset should leave one safety snapshot");
        assert!(app.redo.is_empty());
        assert!(app.dirty);
    }

    /// Saving to a given path must write the file and read back.
    #[test]
    fn save_now_to_writes_and_loads_back() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("저장본".to_string(), "p".to_string(), true);
        let dir = std::env::temp_dir().join(format!("pc_new_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        assert!(app.save_now_to(&path));
        assert!(!app.dirty);
        let back = crate::store::load_from(&path).unwrap();
        assert_eq!(back.nodes.len(), 1);
        assert_eq!(back.nodes[0].title, "저장본");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opening a saved file must load the document at the path and report status.
    #[test]
    fn open_from_path_loads_and_reports() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.config.lang = crate::i18n::Lang::Ko;
        app.add_node("열기전".to_string(), "p".to_string(), true);
        let dir = std::env::temp_dir().join(format!("pc_open_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        assert!(app.save_now_to(&path));
        // After modifying the canvas, opening the saved copy returns to it.
        app.add_node("열기후".to_string(), "p".to_string(), true);
        assert!(app.open_from_path(&path));
        assert_eq!(app.nodes.len(), 1);
        assert_eq!(app.nodes[0].title, "열기전");
        assert_eq!(app.status, "저장된 파일을 열었습니다.");
        // A missing path fails and leaves the reason in the status.
        let missing = dir.join("missing.json");
        assert!(!app.open_from_path(&missing));
        assert!(
            app.status.contains("가져오기 실패"),
            "status={}",
            app.status
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Image display size defaults to 220px and survives save/load.
    #[test]
    fn image_size_setting_roundtrips() {
        // Check defaults via pure Default; headless_app may read this machine's saved values.
        assert_eq!(crate::store::AppConfig::default().image_h, 220.0);
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.config.image_h = 320.0;
        let dir = std::env::temp_dir().join(format!("pc_img_size_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        assert!(app.save_now_to(&path));
        let back = crate::store::load_from(&path).unwrap();
        assert_eq!(back.config.image_h, 320.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Output text style defaults to 14px non-monospace and survives save/load.
    #[test]
    fn text_style_setting_roundtrips() {
        // Check defaults via pure Default; headless_app may read this machine's saved values.
        let fresh = crate::store::AppConfig::default();
        assert_eq!(fresh.text_size, 14.0);
        assert!(!fresh.text_mono);
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.config.text_size = 20.0;
        app.config.text_mono = true;
        let dir = std::env::temp_dir().join(format!("pc_text_size_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("doc.json");
        assert!(app.save_now_to(&path));
        let back = crate::store::load_from(&path).unwrap();
        assert_eq!(back.config.text_size, 20.0);
        assert!(back.config.text_mono);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The start-new confirm modal must register as an overlay when open.
    #[test]
    fn new_confirm_modal_registers_overlay() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.new_confirm_open = true;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        assert!(
            !app.overlay_rects.is_empty(),
            "open modal should block canvas wheel-zoom"
        );
        app.new_confirm_open = false;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        assert!(
            app.overlay_rects.is_empty(),
            "closed modal should release the overlay"
        );
    }

    /// In a clean state the exit save hook must not touch the disk.
    #[test]
    fn save_hook_skips_clean_state() {
        struct NullStorage;
        impl eframe::Storage for NullStorage {
            fn get_string(&self, _key: &str) -> Option<String> {
                None
            }
            fn set_string(&mut self, _key: &str, _value: String) {}
            fn flush(&mut self) {}
        }
        assert!(
            crate::store::store_path().is_some(),
            "test needs a store path"
        );
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.dirty = false;
        app.status.clear();
        eframe::App::save(&mut app, &mut NullStorage);
        assert!(
            app.status.is_empty(),
            "clean state must not trigger a save, status={}",
            app.status
        );
    }

    /// The results panel must load images from image-output nodes.
    #[test]
    fn results_panel_loads_node_image_texture() {
        // Create a 2x2 red PNG as a temp file.
        let dir = std::env::temp_dir().join(format!("pc_img_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img.png");
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("그림".to_string(), "p".to_string(), true);
        app.add_node("깨짐".to_string(), "p".to_string(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().image_path =
            Some(path.display().to_string());
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().image_path =
            Some(dir.join("missing.png").display().to_string());
        app.results_open = true;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let (cached, _handle) = app.node_textures.get(&1).expect("texture cached");
        assert_eq!(cached, &path.display().to_string());
        assert!(
            !app.node_textures.contains_key(&2),
            "missing file must not cache"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The effective key is session > saved; none when both are empty.
    #[test]
    fn api_key_prefers_session_then_saved() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.session_key = " AAA ".to_string();
        app.config.saved_key = "BBB".to_string();
        assert_eq!(app.api_key_value().as_deref(), Some("AAA"));
        app.session_key = "  ".to_string();
        assert_eq!(app.api_key_value().as_deref(), Some("BBB"));
        app.config.saved_key = String::new();
        assert_eq!(app.api_key_value(), None);
    }

    /// When remember is on, save the session key; when off, clear it.
    #[test]
    fn sync_saved_key_follows_remember_flag() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.session_key = "CCC".to_string();
        app.config.remember_key = true;
        app.sync_saved_key();
        assert_eq!(app.config.saved_key, "CCC");
        app.config.remember_key = false;
        app.sync_saved_key();
        assert!(app.config.saved_key.is_empty());
        // Leave the saved key alone when the session is empty (protect right after restart).
        app.config.saved_key = "DDD".to_string();
        app.config.remember_key = true;
        app.session_key.clear();
        app.sync_saved_key();
        assert_eq!(app.config.saved_key, "DDD");
    }

    /// The combined-results window must register as an overlay when open.
    #[test]
    fn combined_zoom_registers_overlay() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.show_combined_zoom = true;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        assert!(
            !app.overlay_rects.is_empty(),
            "open modal should block canvas wheel-zoom"
        );
        app.show_combined_zoom = false;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        assert!(
            app.overlay_rects.is_empty(),
            "closed modal should release the overlay"
        );
    }

    /// Meshed (bidirectional) links are allowed; longer cycles stay blocked.
    #[test]
    fn mutual_link_succeeds_and_longer_cycle_blocked() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        let lang = app.config.lang;
        app.add_node("A".to_string(), "a".to_string(), true);
        app.add_node("B".to_string(), "b".to_string(), true);
        app.add_node("C".to_string(), "c".to_string(), true);
        app.complete_link(1, 2, lang);
        assert_eq!(
            app.nodes.iter().find(|n| n.id == 2).unwrap().inputs,
            vec![1]
        );
        // Adding the reverse = bidirectional link. It used to be blocked as a cycle.
        app.complete_link(2, 1, lang);
        assert_eq!(
            app.nodes.iter().find(|n| n.id == 1).unwrap().inputs,
            vec![2]
        );
        // 2→3 links as a normal connection.
        app.complete_link(2, 3, lang);
        // 3→1 is a longer cycle, so it stays blocked.
        app.complete_link(3, 1, lang);
        assert_eq!(
            app.nodes.iter().find(|n| n.id == 1).unwrap().inputs,
            vec![2]
        );
    }

    /// In chain mode a meshed pair must not wait on each other.
    #[test]
    fn chain_mode_runs_mutual_pair() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        let lang = app.config.lang;
        app.add_node("A".to_string(), "a".to_string(), true);
        app.add_node("B".to_string(), "b".to_string(), true);
        app.complete_link(1, 2, lang);
        app.complete_link(2, 1, lang);
        app.batch = crate::state::BatchState::Running {
            queue: vec![1, 2],
            done: Vec::new(),
            failed: Vec::new(),
            scope: crate::state::BatchScope::All,
        };
        assert!(app.cascade_ready(1), "mutual pair must not deadlock A");
        assert!(app.cascade_ready(2), "mutual pair must not deadlock B");
    }

    /// Dragging the bottom-right corner must resize the card.
    #[test]
    fn corner_handle_resizes_node() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Aim at the middle of the handle row (below the frame padding).
        let at = egui::pos2(r[2] - 16.0, r[3] - 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at), click(at, true)],
        );
        let to = egui::pos2(at.x + 40.0, at.y + 30.0);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        assert!(
            node.width_scale > 1.0,
            "corner drag should widen: {}",
            node.width_scale
        );
        assert!(
            node.height_scale > 1.0,
            "corner drag should heighten: {}",
            node.height_scale
        );
    }

    /// Dragging the bottom middle (not a corner) must not resize.
    #[test]
    fn bottom_middle_does_not_resize() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Aim at the middle of the handle row (below the frame padding).
        let at = egui::pos2(r[2] - 100.0, r[3] - 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(at), click(at, true)],
        );
        let to = egui::pos2(at.x + 40.0, at.y + 30.0);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(node.width_scale, 1.0, "bottom middle must not resize width");
        assert_eq!(
            node.height_scale, 1.0,
            "bottom middle must not resize height"
        );
    }

    /// Dragging a card past the map's top edge must stop it inside.
    #[test]
    fn drag_node_stops_at_board_edge() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.config.board_w = 600.0;
        app.config.board_h = 600.0;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Empty spot at the bar's right end (the center is covered by the collapse button).
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        // Hover priming: a press routes to the last hovered target.
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        let y0 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos[1];
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        // Drag past the top of the map.
        let target = egui::pos2(bar.x, 5.0);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(target)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(target), click(target, false)],
        );
        let pos = app.nodes.iter().find(|n| n.id == 1).unwrap().pos;
        assert!(
            pos[1] < y0 - 50.0,
            "card should actually move up when dragged, {y0} -> {}",
            pos[1]
        );
        assert!(
            pos[1] >= -0.5,
            "dragged card should stop at board top, y={}",
            pos[1]
        );
    }

    /// Keep wheeling up: zoom must grow past the old 8x cap.
    #[test]
    fn zoom_has_no_practical_limit() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let at = egui::pos2(600.0, 350.0);
        for _ in 0..40 {
            run_frame(
                &mut app,
                &mut frame,
                &ctx,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, 1.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(
            app.zoom > 8.0,
            "zoom should pass the old 8x cap, got {}",
            app.zoom
        );
    }

    /// Linking with a long title / multi-line output must not jump the card size.
    #[test]
    fn link_unlink_keeps_card_size_stable() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("제목".repeat(30), "프롬프트".to_string(), true);
        app.add_node("대상".to_string(), "프롬프트".to_string(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().output = (1..=20)
            .map(|i| format!("출력 {i}번째 줄 내용이 길게 이어집니다"))
            .collect::<Vec<_>>()
            .join("\n");
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r0 = app
            .nodes
            .iter()
            .find(|n| n.id == 2)
            .unwrap()
            .last_rect
            .unwrap();
        let (w0, h0) = (r0[2] - r0[0], r0[3] - r0[1]);
        // Link: only one input row grows; width must stay the same.
        app.nodes
            .iter_mut()
            .find(|n| n.id == 2)
            .unwrap()
            .inputs
            .push(1);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r1 = app
            .nodes
            .iter()
            .find(|n| n.id == 2)
            .unwrap()
            .last_rect
            .unwrap();
        let (w1, h1) = (r1[2] - r1[0], r1[3] - r1[1]);
        assert!(
            (w1 - w0).abs() < 1.0,
            "link must not widen card: {w0} -> {w1}"
        );
        assert!(
            h1 - h0 < 60.0,
            "link must add at most one row: {h0} -> {h1}"
        );
        // Unlink: must return to the original size.
        app.nodes
            .iter_mut()
            .find(|n| n.id == 2)
            .unwrap()
            .inputs
            .clear();
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r2 = app
            .nodes
            .iter()
            .find(|n| n.id == 2)
            .unwrap()
            .last_rect
            .unwrap();
        let (w2, h2) = (r2[2] - r2[0], r2[3] - r2[1]);
        assert!(
            (w2 - w0).abs() < 1.0,
            "unlink must restore width: {w0} -> {w2}"
        );
        assert!(
            (h2 - h0).abs() < 1.0,
            "unlink must restore height: {h0} -> {h2}"
        );
    }

    /// When a node's scheduled time arrives, that node runs.
    #[test]
    fn node_schedule_fires_when_due() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let past = chrono::Local::now() - chrono::TimeDelta::minutes(5);
        let node = app.nodes.iter_mut().find(|n| n.id == 1).unwrap();
        node.sched.enabled = true;
        node.sched.next_due = Some(past);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(node.sched.runs_done, 1, "due node schedule should fire");
    }

    /// While a batch runs, per-node schedules defer instead of firing.
    #[test]
    fn node_schedule_waits_while_batch_runs() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("카드".to_string(), "프롬프트".to_string(), true);
        app.add_node("둘째".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        // Pin the batch in Running with a never-ending fake job on node 2.
        let (_tx, rx) = std::sync::mpsc::channel();
        app.pending.insert(
            2,
            crate::run::PendingJob {
                rx,
                progress: None,
                started: std::time::Instant::now(),
                timeout: std::time::Duration::from_secs(600),
                kind: crate::run::JobKind::Generate,
            },
        );
        app.batch = crate::state::BatchState::Running {
            queue: vec![2],
            done: Vec::new(),
            failed: Vec::new(),
            scope: crate::state::BatchScope::All,
        };
        let past = chrono::Local::now() - chrono::TimeDelta::minutes(5);
        let node = app.nodes.iter_mut().find(|n| n.id == 1).unwrap();
        node.sched.enabled = true;
        node.sched.next_due = Some(past);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(node.sched.runs_done, 0, "batch must defer node schedule");
        assert!(node.sched.enabled, "deferred schedule stays armed");
    }

    /// Compute a link's midpoint in screen coordinates (same formula as rendering).
    fn link_mid(app: &App, src_id: usize, dst_id: usize) -> egui::Pos2 {
        let sr = app
            .nodes
            .iter()
            .find(|n| n.id == src_id)
            .unwrap()
            .last_rect
            .unwrap();
        let dr = app
            .nodes
            .iter()
            .find(|n| n.id == dst_id)
            .unwrap()
            .last_rect
            .unwrap();
        let op = out_port(sr);
        let ip = in_port(dr);
        egui::pos2((op[0] + ip[0]) / 2.0, (op[1] + ip[1]) / 2.0)
    }

    /// Spread two cards apart without overlap and link 1→2.
    fn two_linked_nodes(app: &mut App, frame: &mut eframe::Frame, ctx: &egui::Context) {
        app.add_node("원본".to_string(), "프롬프트".to_string(), true);
        app.add_node("대상".to_string(), "프롬프트".to_string(), true);
        run_frame(app, frame, ctx, vec![]);
        let w1 = {
            let r = app
                .nodes
                .iter()
                .find(|n| n.id == 1)
                .unwrap()
                .last_rect
                .unwrap();
            r[2] - r[0]
        };
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().pos = [0.0, 0.0];
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().pos = [w1 + 120.0, 0.0];
        app.nodes
            .iter_mut()
            .find(|n| n.id == 2)
            .unwrap()
            .inputs
            .push(1);
        run_frame(app, frame, ctx, vec![]);
        run_frame(app, frame, ctx, vec![]);
    }

    /// Right-clicking a link must open the menu and keep the edge.
    #[test]
    fn link_right_click_opens_menu_and_keeps_edge() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        two_linked_nodes(&mut app, &mut frame, &ctx);
        let mid = link_mid(&app, 1, 2);
        let click = |at: egui::Pos2, button: egui::PointerButton, pressed: bool| {
            egui::Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(mid)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![
                egui::Event::PointerMoved(mid),
                click(mid, egui::PointerButton::Secondary, true),
            ],
        );
        // Popup-open state clears after the pass, so read it inside the closure.
        let mut popup_open = false;
        let _ = ctx.run(
            frame_input(
                screen_rect(),
                vec![
                    egui::Event::PointerMoved(mid),
                    click(mid, egui::PointerButton::Secondary, false),
                ],
            ),
            |ctx| {
                eframe::App::update(&mut app, ctx, &mut frame);
                popup_open = ctx.is_popup_open();
            },
        );
        assert!(popup_open, "right-click should open link menu");
        let dst = app.nodes.iter().find(|n| n.id == 2).unwrap();
        assert_eq!(dst.inputs, vec![1], "right-click must not unlink");
    }

    /// Left-clicking a link must unlink it (keep existing behavior).
    #[test]
    fn link_left_click_unlinks() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        two_linked_nodes(&mut app, &mut frame, &ctx);
        let mid = link_mid(&app, 1, 2);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(mid)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(mid), click(mid, true)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(mid), click(mid, false)],
        );
        let dst = app.nodes.iter().find(|n| n.id == 2).unwrap();
        assert!(dst.inputs.is_empty(), "left-click should unlink");
    }

    /// One-way edge menu: unlink, bidirectional, separator, run source, run target.
    #[test]
    fn link_menu_lists_unlink_mutual_and_runs() {
        use crate::state::{LinkMenuAction, LinkMenuRow};
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("원본".to_string(), "a".to_string(), true);
        app.add_node("대상".to_string(), "b".to_string(), true);
        app.nodes
            .iter_mut()
            .find(|n| n.id == 2)
            .unwrap()
            .inputs
            .push(1);
        let lang = app.config.lang;
        let rows = app.link_menu_items(1, 2, lang);
        assert_eq!(rows.len(), 5, "unlink+mutual+sep+2 runs");
        match &rows[0] {
            LinkMenuRow::Item(it) => {
                assert_eq!(it.label, "연결 끊기");
                assert!(it.enabled);
                assert_eq!(it.action, LinkMenuAction::Unlink { src: 1, dst: 2 });
            }
            LinkMenuRow::Sep => panic!("row 0 must be unlink"),
        }
        match &rows[1] {
            LinkMenuRow::Item(it) => {
                assert_eq!(it.label, "양방향으로 만들기");
                assert!(it.enabled);
                assert_eq!(it.action, LinkMenuAction::MakeMutual { src: 1, dst: 2 });
            }
            LinkMenuRow::Sep => panic!("row 1 must be make-mutual"),
        }
        assert!(matches!(rows[2], LinkMenuRow::Sep));
        match &rows[3] {
            LinkMenuRow::Item(it) => {
                assert!(it.label.contains("원본"), "label: {}", it.label);
                assert!(it.enabled);
                assert_eq!(it.action, LinkMenuAction::Run(1));
            }
            LinkMenuRow::Sep => panic!("row 3 must run source"),
        }
        match &rows[4] {
            LinkMenuRow::Item(it) => {
                assert!(it.label.contains("대상"), "label: {}", it.label);
                assert!(it.enabled);
                assert_eq!(it.action, LinkMenuAction::Run(2));
            }
            LinkMenuRow::Sep => panic!("row 4 must run target"),
        }
        // Run items for OFF nodes are disabled.
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().enabled = false;
        let rows = app.link_menu_items(1, 2, lang);
        match &rows[4] {
            LinkMenuRow::Item(it) => {
                assert!(!it.enabled);
                assert!(it.tip.as_deref().unwrap().contains("OFF"));
            }
            LinkMenuRow::Sep => panic!("row 4 must run target"),
        }
    }

    /// A meshed pair's menu offers revert-to-one-way; applying it unlinks only the reverse edge.
    #[test]
    fn link_menu_reverts_mutual_pair_to_single() {
        use crate::state::{LinkMenuAction, LinkMenuRow};
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("a".to_string(), "a".to_string(), true);
        app.add_node("b".to_string(), "b".to_string(), true);
        app.nodes
            .iter_mut()
            .find(|n| n.id == 2)
            .unwrap()
            .inputs
            .push(1);
        app.nodes
            .iter_mut()
            .find(|n| n.id == 1)
            .unwrap()
            .inputs
            .push(2);
        let lang = app.config.lang;
        let rows = app.link_menu_items(1, 2, lang);
        match &rows[1] {
            LinkMenuRow::Item(it) => {
                assert_eq!(it.label, "단방향으로 되돌리기");
                assert_eq!(it.action, LinkMenuAction::MakeSingle { src: 1, dst: 2 });
            }
            LinkMenuRow::Sep => panic!("row 1 must be make-single"),
        }
        app.apply_link_menu(LinkMenuAction::MakeSingle { src: 1, dst: 2 }, lang);
        assert!(app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .inputs
            .is_empty());
        assert_eq!(
            app.nodes.iter().find(|n| n.id == 2).unwrap().inputs,
            vec![1]
        );
    }

    /// When the reverse edge would form a longer cycle, disable the bidirectional item.
    #[test]
    fn link_menu_disables_mutual_when_longer_cycle() {
        use crate::state::LinkMenuRow;
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("x".to_string(), "x".to_string(), true);
        app.add_node("y".to_string(), "y".to_string(), true);
        app.add_node("z".to_string(), "z".to_string(), true);
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().inputs = vec![1, 3];
        app.nodes.iter_mut().find(|n| n.id == 3).unwrap().inputs = vec![1];
        let rows = app.link_menu_items(1, 2, app.config.lang);
        match &rows[1] {
            LinkMenuRow::Item(it) => {
                assert!(!it.enabled, "longer cycle must disable make-mutual");
                assert!(it.tip.is_some());
            }
            LinkMenuRow::Sep => panic!("row 1 must be make-mutual"),
        }
    }

    /// The menu's make-bidirectional links the reverse edge; run spawns a job.
    #[test]
    fn link_menu_make_mutual_and_run_apply() {
        use crate::state::LinkMenuAction;
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("a".to_string(), "a".to_string(), true);
        app.add_node("b".to_string(), "b".to_string(), true);
        app.nodes
            .iter_mut()
            .find(|n| n.id == 2)
            .unwrap()
            .inputs
            .push(1);
        let lang = app.config.lang;
        app.apply_link_menu(LinkMenuAction::MakeMutual { src: 1, dst: 2 }, lang);
        assert_eq!(
            app.nodes.iter().find(|n| n.id == 1).unwrap().inputs,
            vec![2]
        );
        assert_eq!(app.status, "양방향으로 연결했습니다.");
        app.apply_link_menu(LinkMenuAction::Run(1), lang);
        assert!(app.pending.contains_key(&1), "menu run should spawn");
    }

    /// Pulling one card out of a pile must leave the existing pile in place.
    #[test]
    fn pile_drop_leaves_prepile_alone() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        for i in 1..=10 {
            app.add_node(format!("프롬프트 {i}"), "p".to_string(), true);
            run_frame(&mut app, &mut frame, &ctx, vec![]);
        }
        // Stack at the center like old saved values.
        for n in app.nodes.iter_mut() {
            n.pos = [500.0, 200.0];
        }
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 10)
            .unwrap()
            .last_rect
            .unwrap();
        // Empty spot at the bar's right end (starting 11px below the top; the center is covered by buttons).
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // Hover priming: a press routes to the last hovered target.
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        let to = egui::pos2(bar.x + 30.0, bar.y + 20.0);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let p10 = app.nodes.iter().find(|n| n.id == 10).unwrap().pos;
        assert!(
            (p10[0] - 530.0).abs() < 1.0 && (p10[1] - 220.0).abs() < 1.0,
            "dropped card should land at drop point, got {p10:?}"
        );
        for n in app.nodes.iter().filter(|n| n.id != 10) {
            assert_eq!(
                n.pos,
                [500.0, 200.0],
                "pre-existing pile must not move on drop (node {})",
                n.id
            );
        }
    }

    /// Grabbing and dragging while zoomed out must not let magnetic snap fling the card.
    #[test]
    fn low_zoom_drag_does_not_teleport() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("한째".to_string(), "p".to_string(), true);
        app.add_node("두째".to_string(), "p".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let w1 = {
            let r = app
                .nodes
                .iter()
                .find(|n| n.id == 1)
                .unwrap()
                .last_rect
                .unwrap();
            r[2] - r[0]
        };
        app.zoom = 0.5;
        // Stick node 2 flush to node 1's right edge (inside the half-zoom screen).
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().pos = [760.0, 200.0];
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().pos = [760.0 + w1, 200.0];
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Empty spot at the bar's right end (the center is covered by the collapse button).
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // Hover priming: a press routes to the last hovered target.
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        // 30 screen px = 60 canvas px of movement. A lone move of 6px or less
        // is not caught as an egui drag, so unit tests cover small snap checks.
        let to = egui::pos2(bar.x + 30.0, bar.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        let x1 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos[0];
        assert!(
            (x1 - 820.0).abs() < 3.0,
            "low-zoom drag should follow pointer, x={x1}"
        );
    }

    /// Dropping a card overlapped must push the neighbor out so they no longer overlap.
    #[test]
    fn drop_pushes_overlapped_neighbor_out() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.add_node("한째".to_string(), "프롬프트".to_string(), true);
        app.add_node("두째".to_string(), "프롬프트".to_string(), true);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        // Make room at half-zoom and place 60px apart.
        // Read the width from a zoom-1 frame rect (screen width = canvas width).
        let w1 = {
            let r = app
                .nodes
                .iter()
                .find(|n| n.id == 1)
                .unwrap()
                .last_rect
                .unwrap();
            r[2] - r[0]
        };
        app.zoom = 0.5;
        // Place inside central so it fits the half-zoom screen (x<347 is under the left panel).
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().pos = [720.0, 100.0];
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().pos = [720.0 + w1 + 60.0, 100.0];
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        // Grab node 1's title bar and drag 110px (canvas) over node 2 to overlap.
        let r = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        // Empty spot at the bar's right end (the center is covered by the collapse button).
        let bar = egui::pos2(r[2] - 40.0, r[1] + 16.0);
        let click = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(bar), click(bar, true)],
        );
        let to = egui::pos2(bar.x + 55.0, bar.y);
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to)],
        );
        run_frame(
            &mut app,
            &mut frame,
            &ctx,
            vec![egui::Event::PointerMoved(to), click(to, false)],
        );
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let a = app
            .nodes
            .iter()
            .find(|n| n.id == 1)
            .unwrap()
            .last_rect
            .unwrap();
        let b = app
            .nodes
            .iter()
            .find(|n| n.id == 2)
            .unwrap()
            .last_rect
            .unwrap();
        assert!(
            !rects_overlap(a, b, 0.0),
            "drop must declump: {a:?} vs {b:?}"
        );
        // Check the drag really applied (110px move from start x=720).
        let x1 = app.nodes.iter().find(|n| n.id == 1).unwrap().pos[0];
        assert!(x1 > 770.0, "dragged card should actually move, x={x1}");
    }

    /// New nodes must spawn in an empty spot without overlapping existing cards.
    #[test]
    fn spawned_nodes_do_not_stack() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        for i in 0..3 {
            app.add_node(format!("n{i}"), "프롬프트".to_string(), true);
            run_frame(&mut app, &mut frame, &ctx, vec![]);
        }
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let rects: Vec<[f32; 4]> = app.nodes.iter().filter_map(|n| n.last_rect).collect();
        assert_eq!(rects.len(), 3);
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                assert!(
                    !rects_overlap(rects[i], rects[j], 0.0),
                    "spawns must not stack: {:?} vs {:?}",
                    rects[i],
                    rects[j]
                );
            }
        }
    }

    /// New nodes must spawn inside the current viewport even when panned far away.
    #[test]
    fn new_node_spawns_in_viewport() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        app.canvas = [-5000.0, -3000.0];
        app.add_node("새것".to_string(), String::new(), true);
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        let z = app.zoom.max(0.001);
        let sx = node.pos[0] * z + app.canvas[0];
        let sy = node.pos[1] * z + app.canvas[1];
        let r = app.central_rect.unwrap();
        assert!(
            sx >= r[0] && sx <= r[2] && sy >= r[1] && sy <= r[3],
            "new node at screen ({sx}, {sy}) should be inside viewport {r:?}"
        );
    }

    /// Standalone results-window filter: all, running, errors-only.
    #[test]
    fn results_win_filter_modes() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("첫째".to_string(), String::new(), true);
        app.add_node("둘째".to_string(), String::new(), true);
        app.add_node("셋째".to_string(), String::new(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().output = "ok".to_string();
        app.nodes.iter_mut().find(|n| n.id == 2).unwrap().output = "[오류] timed out".to_string();
        let (_tx, rx) = std::sync::mpsc::channel();
        app.pending.insert(
            3,
            crate::run::PendingJob {
                rx,
                progress: None,
                started: std::time::Instant::now(),
                timeout: std::time::Duration::from_secs(600),
                kind: crate::run::JobKind::Generate,
            },
        );
        use crate::state::ResultsWinFilter;
        assert_eq!(
            app.filter_result_nodes(ResultsWinFilter::All),
            vec![1, 2, 3]
        );
        assert_eq!(app.filter_result_nodes(ResultsWinFilter::Running), vec![3]);
        assert_eq!(app.filter_result_nodes(ResultsWinFilter::Errors), vec![2]);
    }

    /// Python node: with empty code, spawn no job and report via status.
    #[test]
    fn spawn_python_empty_prompt_sets_status() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("파이".to_string(), String::new(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().mode = crate::node::NodeMode::Python;
        app.spawn_python(1);
        assert!(app.pending.is_empty());
        assert!(app.status.contains("파이"));
    }

    /// Process output merge: text on success, error on failure.
    #[test]
    fn combine_exec_output_merges_streams() {
        use crate::run::{combine_exec_output, JobError, JobResult};
        match combine_exec_output(true, "42\n", "warn\n") {
            JobResult::Text(t) => {
                assert!(t.contains("42"));
                assert!(t.contains("[stderr]"));
                assert!(t.contains("warn"));
            }
            JobResult::Error(_) => panic!("success should be Text"),
        }
        match combine_exec_output(false, "out", "boom") {
            JobResult::Error(JobError::Other(e)) => {
                assert!(e.contains("out"));
                assert!(e.contains("boom"));
            }
            _ => panic!("failure should be Other error"),
        }
        assert!(matches!(
            combine_exec_output(false, "", ""),
            JobResult::Error(JobError::Other(_))
        ));
    }

    /// Alarms: spawn no job when there is nowhere to send.
    #[test]
    fn launch_alarm_without_channels_spawns_nothing() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.launch_alarm("done".to_string());
        assert!(app.alarm_job.is_none());
        assert!(app.alarm_status.is_empty());
    }

    /// Combined-results window: long output must open at a modest size, not fill the screen.
    #[test]
    fn combined_zoom_stays_compact_with_long_output() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.add_node("김".to_string(), String::new(), true);
        let long = (0..300)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().output = long;
        app.show_combined_zoom = true;
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        let r = ctx
            .memory(|m| m.area_rect(egui::Id::new("modal-combined-zoom")))
            .expect("combined zoom window should exist");
        let h = r.max.y - r.min.y;
        assert!(
            h < 560.0,
            "300-line output should not fill the 700px screen, got height {h}"
        );
    }

    /// Alarm test: with nowhere to send, leave the reason in the status.
    #[test]
    fn send_alarm_test_without_channels_explains() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.send_alarm_test();
        assert!(app.alarm_job.is_none());
        assert!(!app.alarm_status.is_empty());
    }

    /// Zoom view opens the node-output standalone window directly, with no intermediate modal.
    #[test]
    fn open_output_window_goes_direct_to_os_window() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.add_node("첫째".to_string(), String::new(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().output = "hello".to_string();
        app.open_output_window(1);
        assert_eq!(app.output_window, Some(1));
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        assert_eq!(app.output_window, Some(1));
    }

    /// Make a 2x2 red PNG temp file. For standalone-window image tests.
    fn tiny_test_png(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pc_img_win_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img.png");
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();
        path
    }

    /// The zoom-view renderer must load and show the node image.
    #[test]
    fn output_window_renders_node_image() {
        let path = tiny_test_png("output");
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("그림".to_string(), "p".to_string(), true);
        let node = app.nodes.iter_mut().find(|n| n.id == 1).unwrap();
        node.output = "done".to_string();
        node.image_path = Some(path.display().to_string());
        app.output_window = Some(1);
        assert!(app.node_textures.is_empty());
        let _ = ctx.run(frame_input(screen_rect(), vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.render_output_window(ctx, ui);
            });
        });
        assert!(
            app.node_textures.contains_key(&1),
            "output window should load the node image"
        );
    }

    /// The live results window's per-node renderer must load and show node images.
    #[test]
    fn results_window_per_node_renders_images() {
        let path = tiny_test_png("results");
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("그림".to_string(), "p".to_string(), true);
        let node = app.nodes.iter_mut().find(|n| n.id == 1).unwrap();
        node.output = "done".to_string();
        node.image_path = Some(path.display().to_string());
        app.show_results_window = true;
        assert!(app.node_textures.is_empty());
        let _ = ctx.run(frame_input(screen_rect(), vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.render_results_window(ctx, ui);
            });
        });
        assert!(
            app.node_textures.contains_key(&1),
            "results window should load the node image"
        );
    }

    /// With both OS-independent windows on, one frame must run with no close requests.
    #[test]
    fn independent_windows_render_headless() {
        let ctx = egui::Context::default();
        let (mut app, mut frame) = headless_app(&ctx);
        app.add_node("첫째".to_string(), String::new(), true);
        app.nodes.iter_mut().find(|n| n.id == 1).unwrap().output = "hello".to_string();
        app.show_results_window = true;
        app.output_window = Some(1);
        run_frame(&mut app, &mut frame, &ctx, vec![]);
        assert!(app.show_results_window);
        assert_eq!(app.output_window, Some(1));
    }

    fn pending_tool_job(rx: std::sync::mpsc::Receiver<JobResult>, timeout_secs: u64) -> PendingJob {
        PendingJob {
            rx,
            progress: None,
            started: std::time::Instant::now(),
            timeout: std::time::Duration::from_secs(timeout_secs),
            kind: JobKind::Tool {
                server: "s".to_string(),
                tool: "t".to_string(),
            },
        }
    }

    /// poll fast path: with no result, touch nothing and return false.
    #[test]
    fn poll_one_empty_keeps_job() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("첫째".to_string(), "hi".to_string(), true);
        // Keep the sender alive. If dropped, it counts as aborted and returns true.
        let (_tx, rx) = std::sync::mpsc::channel();
        app.pending.insert(1, pending_tool_job(rx, 60));
        assert!(!app.poll_one(1));
        assert!(app.pending.contains_key(&1));
    }

    /// Tool done: the result lands in node context and returns true.
    #[test]
    fn poll_one_tool_result_lands_in_context() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("첫째".to_string(), "hi".to_string(), true);
        let (tx, rx) = std::sync::mpsc::channel();
        app.pending.insert(1, pending_tool_job(rx, 60));
        tx.send(JobResult::Text("42".to_string())).unwrap();
        assert!(app.poll_one(1));
        let node = app.nodes.iter().find(|n| n.id == 1).unwrap();
        assert_eq!(node.context.len(), 1);
        assert_eq!(node.context[0].text, "42");
    }

    /// Tool timeout: return true and leave the reason in the status.
    #[test]
    fn poll_one_timeout_marks_tool_job_done() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.add_node("첫째".to_string(), "hi".to_string(), true);
        let (_tx, rx) = std::sync::mpsc::channel();
        app.pending.insert(1, pending_tool_job(rx, 0));
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(app.poll_one(1));
        assert!(!app.status.is_empty());
    }

    /// API reset: clear connection values/keys and mark for autosave.
    #[test]
    fn reset_api_settings_clears_keys_and_marks_dirty() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.config.model = "custom".to_string();
        app.config.saved_key = "SAVED".to_string();
        app.config.remember_key = true;
        app.session_key = "SESSION".to_string();
        app.test_status = "연결 성공".to_string();
        app.dirty = false;
        app.reset_api_settings();
        assert_eq!(app.config.model, "muse-spark-1.3");
        assert!(app.config.saved_key.is_empty());
        assert!(!app.config.remember_key);
        assert!(app.session_key.is_empty());
        assert!(app.test_status.is_empty());
        assert!(app.dirty);
    }

    fn two_step_plan() -> crate::blueprint::BlueprintPlan {
        use crate::blueprint::BlueprintNode;
        crate::blueprint::BlueprintPlan {
            nodes: vec![
                BlueprintNode {
                    title: "요약".to_string(),
                    prompt: "핵심 3줄".to_string(),
                    mode: NodeMode::Summarize,
                    inputs: vec![],
                    system: String::new(),
                    model: None,
                    temperature: None,
                    max_tokens: None,
                },
                BlueprintNode {
                    title: "번역".to_string(),
                    prompt: "영어로".to_string(),
                    mode: NodeMode::TranslateEn,
                    inputs: vec![0],
                    system: String::new(),
                    model: None,
                    temperature: Some(0.3),
                    max_tokens: None,
                },
            ],
        }
    }

    /// Blueprint apply: nodes are created, inputs wired, and undo kept.
    #[test]
    fn apply_blueprint_creates_wired_nodes() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        let made = app.apply_blueprint(two_step_plan());
        assert_eq!(made, 2);
        assert_eq!(app.nodes.len(), 2);
        assert_eq!(app.nodes[0].mode, NodeMode::Summarize);
        assert_eq!(app.nodes[1].inputs, vec![app.nodes[0].id]);
        assert_eq!(app.nodes[1].temperature, Some(0.3));
        assert!(app.selected.contains(&app.nodes[0].id));
        assert!(app.selected.contains(&app.nodes[1].id));
        assert!(!app.undo.is_empty());
        assert!(app.dirty);
    }

    /// In Mock, send no blueprint request; only show guidance.
    #[test]
    fn spawn_blueprint_mock_explains_without_spawning() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.config.provider = crate::store::Provider::Mock;
        app.blueprint_text = "3단계 파이프라인".to_string();
        app.spawn_blueprint();
        assert!(app.blueprint_job.is_none());
        assert!(!app.blueprint_status.is_empty());
    }

    /// With an empty description, send no blueprint request.
    #[test]
    fn spawn_blueprint_empty_description_stays_idle() {
        let ctx = egui::Context::default();
        let (mut app, _frame) = headless_app(&ctx);
        app.blueprint_text = "   ".to_string();
        app.spawn_blueprint();
        assert!(app.blueprint_job.is_none());
        assert!(!app.blueprint_status.is_empty());
    }
}
