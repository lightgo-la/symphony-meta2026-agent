//! Canvas, node card, port, and minimap rendering.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Id, Sense};

use crate::i18n::t;
use crate::node::{
    blocked_cycle, clamp_board_step, clamp_panel_x_step, combined_results, declump, in_port,
    is_error_output, is_mutual_only, magnetic_snap, out_port, rects_overlap, short_title,
    NodeAccent, NodeMode, NodeRadius, PORT_HIT, UNLINK_HIT,
};
use crate::run::JobError;
use crate::schedule::{fmt_when, SchedRepeat};
use crate::share;
use crate::state::{App, LinkMenuAction, LinkMenuRow};
use crate::store::{load_from, save_to, CanvasBg, GridStyle, RunMode};
use crate::theme::*;
use crate::zoom::{
    anchored_canvas, band_rect, clamp_zoom, point_in_rect, point_in_zones, select_in_rect,
    step_zoom_at_point, wheel_should_zoom, zoom_factor_for_wheel,
};

pub(crate) enum NodeAction {
    Run,
    Cancel,
    Clone,
    Delete,
    OpenTools,
    Front,
    LinkFrom(usize),
    SetInputs(Vec<usize>),
    ArmLink,
    DisarmLink,
    Unlink(usize),
    UnlinkAll,
    OutputWindow,
}

/// Per-item commit action for the inspector (results panel). Applied when show ends.
enum InspBit {
    Toggle,
    ResetTemp,
    ResetTokens,
}

/// Per-item commit actions that must not run before the undo snapshot.
/// Cannot call `&mut self` methods inside the card render closure, so
/// they are deferred past the loop and applied after the snapshot.
enum LateBit {
    ToggleEnabled(usize),
    ResetTemp(usize),
    ResetTokens(usize),
    ResetModel(usize),
    SetImage(usize, String),
    ClearImage(usize),
    RemoveCtx(usize, usize),
}

/// Measures with a galley whether multi-line text overflows its box (visible_rows lines).
/// If it does not overflow, wheel zoom is not blocked.
fn text_overflows(ui: &egui::Ui, text: &str, wrap_width: f32, visible_rows: usize) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout(
        text.to_string(),
        font,
        egui::Color32::WHITE,
        wrap_width.max(20.0),
    );
    galley.rows.len() > visible_rows.max(1)
}

/// Loads a node's representative image as a texture. Reuses the cache for the same path.
/// Returns None when the file is missing or decoding fails (caller shows fallback text).
pub(crate) fn load_node_image(
    ctx: &egui::Context,
    cache: &mut HashMap<usize, (String, egui::TextureHandle)>,
    node_id: usize,
    path: &str,
) -> Option<egui::TextureHandle> {
    if let Some((cached, handle)) = cache.get(&node_id) {
        if cached == path {
            return Some(handle.clone());
        }
    }
    let img = image::open(path).ok()?;
    let thumb = img.thumbnail(640, 640);
    let rgba = thumb.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    if size[0] == 0 || size[1] == 0 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let handle = ctx.load_texture(
        format!("node-img-{node_id}"),
        color,
        egui::TextureOptions::LINEAR,
    );
    cache.insert(node_id, (path.to_string(), handle.clone()));
    Some(handle)
}

impl App {
    pub(crate) fn render_menu(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        let pal = palette(self.config.theme);
        // ── Left function menu ───────────────────────────────────
        let on_count = self.nodes.iter().filter(|n| n.enabled).count();
        let total = self.nodes.len();
        let provider_label = self.config.provider.label(lang).to_string();
        let (_key_text, key_ok) = Self::key_status_text(lang);
        let session_key_set = !self.session_key.trim().is_empty();
        egui::SidePanel::left("tool_menu")
            .resizable(true)
            .default_width(224.0)
            .frame(hud_panel_frame(pal.menu, pal.faint))
            .show(ctx, |ui| {
                // Console header: logo plate + wordmark + status pills.
                ui.horizontal(|ui| {
                    logo_plate(ui, pal.accent, "S");
                    ui.vertical(|ui| {
                        ui.label(brand_title("Symphony Canvas", pal.brand));
                        ui.label(subtle(t(lang, "노드형 멀티 프롬프트")));
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let fg = ui.visuals().widgets.noninteractive.fg_stroke.color;
                    pill(
                        ui,
                        t(lang, "전체 {total} · ON {on_count}")
                            .replace("{total}", &total.to_string())
                            .replace("{on_count}", &on_count.to_string()),
                        pal.faint,
                        fg,
                    );
                    pill(ui, provider_label, pal.faint, INK_SUB);
                    if session_key_set || key_ok {
                        pill(ui, t(lang, "● 키 OK"), pal.accent, Color32::WHITE);
                    } else {
                        pill(ui, t(lang, "○ 키 없음"), pal.faint, INK_SUB);
                    }
                });
                hud_rule(ui, pal.accent);
                ui.label(section_title(t(lang, "기능")));
                egui::CollapsingHeader::new(t(lang, "파일"))
                    .default_open(true)
                    .id_salt("cat-file")
                    .show(ui, |ui| {
                        if hud_button(ui, t(lang, "저장"), pal.accent)
                            .on_hover_text(t(lang, "단축키: Ctrl+S"))
                            .clicked()
                        {
                            self.save_now();
                        }
                        if hud_button(ui, t(lang, "저장된 파일 열기"), pal.accent)
                            .on_hover_text(t(lang, "단축키: Ctrl+O"))
                            .clicked()
                        {
                            self.open_file_dialog();
                        }
                        if hud_button(ui, t(lang, "새로 시작"), pal.accent).clicked() {
                            self.new_confirm_open = true;
                        }
                    });
                egui::CollapsingHeader::new(t(lang, "실행"))
                    .default_open(true)
                    .id_salt("cat-run")
                    .show(ui, |ui| {
                        ui.label(t(lang, "실행 모드:"));
                        let prev_mode = self.config.mode;
                        egui::ComboBox::from_id_salt("run-mode")
                            .selected_text(self.config.mode.label(lang))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.config.mode,
                                    RunMode::Parallel,
                                    RunMode::Parallel.label(lang),
                                );
                                ui.selectable_value(
                                    &mut self.config.mode,
                                    RunMode::Sequential,
                                    RunMode::Sequential.label(lang),
                                );
                                ui.selectable_value(
                                    &mut self.config.mode,
                                    RunMode::Chain,
                                    RunMode::Chain.label(lang),
                                );
                            });
                        ui.label(subtle(self.config.mode.hint(lang)));
                        if self.config.mode != prev_mode {
                            self.status = t(lang, "실행 모드: {}")
                                .replace("{}", self.config.mode.label(lang));
                            self.mark_dirty();
                        }
                        if hud_button(ui, t(lang, "＋ 노드 추가"), pal.accent).clicked() {
                            self.add_node(
                                t(lang, "프롬프트 {}").replace("{}", &self.next_id.to_string()),
                                String::new(),
                                true,
                            );
                            self.status = t(lang, "새 노드를 추가했습니다. 타이틀바로 이동하세요.")
                                .to_string();
                        }
                        if hud_button(ui, t(lang, "▶ ON 전체 실행"), pal.accent)
                            .on_hover_text(t(lang, "단축키: Ctrl+Enter"))
                            .clicked()
                        {
                            self.run_batch();
                        }
                        if hud_button(ui, t(lang, "✨ 블루프린트"), pal.accent)
                            .on_hover_text(t(lang, "프롬프트로 노드 여러 개 만들기"))
                            .clicked()
                        {
                            self.blueprint_open = true;
                            self.blueprint_status.clear();
                        }
                        // Run selection: runs only the chosen nodes the same way.
                        let sel_count = self.selected.len();
                        let sel_label =
                            t(lang, "▶ 선택 {n}개 실행").replace("{n}", &sel_count.to_string());
                        if ui
                            .add_enabled(
                                sel_count > 0,
                                egui::Button::new(egui::RichText::new(sel_label).strong()),
                            )
                            .on_hover_text(t(
                                lang,
                                "선택한 노드만 실행합니다 (Shift+클릭·Shift+드래그로 선택)",
                            ))
                            .clicked()
                        {
                            self.run_selected();
                        }
                        // Progress + cancel all
                        if self.batch.is_active() || !self.pending.is_empty() {
                            let total = self.batch.queue().len().max(1) as f32;
                            let done =
                                (self.batch.done_count().min(self.batch.queue().len())) as f32;
                            ui.add(
                                egui::ProgressBar::new(done / total)
                                    .show_percentage()
                                    .animate(true),
                            );
                            if ui
                                .button(t(lang, "■ 전체 취소"))
                                .on_hover_text(t(lang, "단축키: Esc"))
                                .clicked()
                            {
                                self.cancel_all();
                            }
                        }
                    });
                egui::CollapsingHeader::new(t(lang, "예약 실행"))
                    .default_open(false)
                    .id_salt("cat-sched")
                    .show(ui, |ui| {
                        if ui
                            .checkbox(&mut self.config.schedule.enabled, t(lang, "예약 켜기"))
                            .changed()
                        {
                            self.config.schedule.last_run = None;
                            self.mark_dirty();
                        }
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "날짜:"));
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.config.schedule.date)
                                        .desired_width(92.0)
                                        .hint_text(t(lang, "YYYY-MM-DD")),
                                )
                                .changed()
                            {
                                self.mark_dirty();
                            }
                        });
                        ui.label(subtle(t(lang, "날짜를 비우면 오늘로 봅니다")));
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "시간:"));
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.config.schedule.time)
                                        .desired_width(60.0)
                                        .hint_text(t(lang, "HH:MM")),
                                )
                                .changed()
                            {
                                self.mark_dirty();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "반복:"));
                            egui::ComboBox::from_id_salt("sched-repeat")
                                .selected_text(t(lang, self.config.schedule.repeat.label()))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.config.schedule.repeat,
                                        SchedRepeat::Once,
                                        t(lang, SchedRepeat::Once.label()),
                                    );
                                    ui.selectable_value(
                                        &mut self.config.schedule.repeat,
                                        SchedRepeat::Daily,
                                        t(lang, SchedRepeat::Daily.label()),
                                    );
                                    ui.selectable_value(
                                        &mut self.config.schedule.repeat,
                                        SchedRepeat::Interval,
                                        t(lang, SchedRepeat::Interval.label()),
                                    );
                                });
                        });
                        if self.config.schedule.repeat == SchedRepeat::Interval {
                            ui.horizontal(|ui| {
                                ui.label(t(lang, "간격:"));
                                let mut hours = self.config.schedule.interval_hours;
                                if ui
                                    .add(egui::DragValue::new(&mut hours).range(0.0..=72.0))
                                    .changed()
                                {
                                    self.config.schedule.interval_hours = hours.min(72);
                                    self.mark_dirty();
                                }
                                ui.label(t(lang, "시간"));
                                let mut mins = self.config.schedule.interval_mins;
                                if ui
                                    .add(egui::DragValue::new(&mut mins).range(0.0..=59.0))
                                    .changed()
                                {
                                    self.config.schedule.interval_mins = mins.min(59);
                                    self.mark_dirty();
                                }
                                ui.label(t(lang, "분"));
                            });
                        }
                        match self.config.schedule.next_due(chrono::Local::now()) {
                            Some(due) => {
                                ui.label(
                                    t(lang, "다음 실행: {at}").replace("{at}", &fmt_when(due)),
                                );
                            }
                            None => {
                                ui.label(subtle(if self.config.schedule.enabled {
                                    t(lang, "날짜·시간 형식이 틀렸습니다.")
                                } else {
                                    t(lang, "예약이 꺼져 있습니다.")
                                }));
                            }
                        }
                    });
                egui::CollapsingHeader::new(t(lang, "화면"))
                    .default_open(true)
                    .id_salt("cat-view")
                    .show(ui, |ui| {
                        if ui
                            .button(if self.results_open {
                                t(lang, "결과 패널 닫기")
                            } else {
                                t(lang, "전체 결과 보기")
                            })
                            .clicked()
                        {
                            self.results_open = !self.results_open;
                        }
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "줌:"));
                            if ui.button("－").clicked() {
                                let center = self.zoom_center();
                                let (canvas, zoom) =
                                    step_zoom_at_point(self.canvas, self.zoom, 1.0 / 1.2, center);
                                self.canvas = canvas;
                                self.zoom = zoom;
                                self.mark_dirty();
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}%",
                                    (self.zoom * 100.0).round() as i32
                                ))
                                .monospace()
                                .strong(),
                            );
                            if ui.button("＋").clicked() {
                                let center = self.zoom_center();
                                let (canvas, zoom) =
                                    step_zoom_at_point(self.canvas, self.zoom, 1.2, center);
                                self.canvas = canvas;
                                self.zoom = zoom;
                                self.mark_dirty();
                            }
                            if ui
                                .button(t(lang, "맞춤"))
                                .on_hover_text(t(lang, "줌 100%·원점으로 되돌립니다"))
                                .clicked()
                            {
                                self.zoom = 1.0;
                                self.canvas = [0.0, 0.0];
                                self.mark_dirty();
                            }
                            if ui
                                .button(t(lang, "전체 보기"))
                                .on_hover_text(t(lang, "모든 노드가 들어오게 맞춥니다"))
                                .clicked()
                            {
                                self.fit_view();
                            }
                            if ui
                                .button(if self.show_minimap {
                                    t(lang, "미니맵 끄기")
                                } else {
                                    t(lang, "미니맵 켜기")
                                })
                                .on_hover_text(t(lang, "우하단 전체 지도 표시 전환"))
                                .clicked()
                            {
                                self.show_minimap = !self.show_minimap;
                            }
                        });
                        // Canvas background: follow theme or custom color + grid style
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "배경:"));
                            let mut bg_theme = matches!(self.config.canvas_bg, CanvasBg::Theme);
                            if ui
                                .selectable_value(&mut bg_theme, true, t(lang, "테마"))
                                .clicked()
                            {
                                self.config.canvas_bg = CanvasBg::Theme;
                                self.mark_dirty();
                            }
                            let mut custom = match self.config.canvas_bg {
                                CanvasBg::Custom(c) => c,
                                CanvasBg::Theme => [255, 255, 255],
                            };
                            if ui
                                .selectable_value(&mut bg_theme, false, t(lang, "직접"))
                                .clicked()
                            {
                                self.config.canvas_bg = CanvasBg::Custom(custom);
                                self.mark_dirty();
                            }
                            if !bg_theme
                                && egui::color_picker::color_edit_button_srgb(ui, &mut custom)
                                    .changed()
                            {
                                self.config.canvas_bg = CanvasBg::Custom(custom);
                                self.mark_dirty();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "격자:"));
                            for style in [GridStyle::Dots, GridStyle::Lines, GridStyle::None] {
                                if ui
                                    .selectable_value(
                                        &mut self.config.grid,
                                        style,
                                        style.label(lang),
                                    )
                                    .clicked()
                                {
                                    self.mark_dirty();
                                }
                            }
                        });
                        // Map boundary size (board). Node placement is confined inside it.
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "맵 크기:"));
                            let mut w = self.config.board_w;
                            let mut h = self.config.board_h;
                            let rw = ui.add(
                                egui::DragValue::new(&mut w)
                                    .range(400.0..=20000.0)
                                    .speed(20.0),
                            );
                            ui.label("×");
                            let rh = ui.add(
                                egui::DragValue::new(&mut h)
                                    .range(400.0..=20000.0)
                                    .speed(20.0),
                            );
                            if rw.changed() || rh.changed() {
                                self.config.board_w = w.clamp(400.0, 20000.0);
                                self.config.board_h = h.clamp(400.0, 20000.0);
                                self.mark_dirty();
                            }
                        });
                    });
                egui::CollapsingHeader::new(t(lang, "노드"))
                    .default_open(true)
                    .id_salt("cat-node")
                    .show(ui, |ui| {
                        if ui
                            .button(t(lang, "바둑판 정렬"))
                            .on_hover_text(t(lang, "노드를 격자로 가지런히 놓습니다"))
                            .clicked()
                        {
                            self.tile_nodes();
                        }
                        ui.horizontal(|ui| {
                            if ui.button(t(lang, "모두 접기")).clicked() {
                                for n in self.nodes.iter_mut() {
                                    n.title_only = true;
                                }
                                self.mark_dirty();
                                self.status = t(lang, "모든 노드를 접었습니다.").to_string();
                            }
                            if ui.button(t(lang, "모두 펼치기")).clicked() {
                                for n in self.nodes.iter_mut() {
                                    n.title_only = false;
                                }
                                self.mark_dirty();
                                self.status = t(lang, "모든 노드를 펼쳤습니다.").to_string();
                            }
                        });
                        // Node search + jump
                        ui.horizontal(|ui| {
                            ui.label(t(lang, "검색:"));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .desired_width(120.0)
                                    .hint_text(t(lang, "제목 검색")),
                            );
                        });
                        if !self.search.trim().is_empty() {
                            let q = self.search.trim().to_lowercase();
                            let hits: Vec<(usize, String)> = self
                                .nodes
                                .iter()
                                .filter(|n| n.title.to_lowercase().contains(&q))
                                .map(|n| (n.id, n.title.clone()))
                                .collect();
                            for (id, title) in hits.iter().take(8) {
                                if ui.button(title).clicked() {
                                    self.focus_node(*id);
                                }
                            }
                            if hits.is_empty() {
                                ui.label(subtle(t(lang, "일치하는 노드 없음")));
                            }
                        }
                    });
                egui::CollapsingHeader::new(t(lang, "편집"))
                    .default_open(true)
                    .id_salt("cat-edit")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(!self.undo.is_empty(), |ui| {
                                if ui
                                    .button(t(lang, "되돌리기"))
                                    .on_hover_text(t(lang, "단축키: Ctrl+Z"))
                                    .clicked()
                                {
                                    self.undo();
                                }
                            });
                            ui.add_enabled_ui(!self.redo.is_empty(), |ui| {
                                if ui
                                    .button(t(lang, "다시 하기"))
                                    .on_hover_text(t(lang, "단축키: Ctrl+Y"))
                                    .clicked()
                                {
                                    self.redo();
                                }
                            });
                        });
                        // Two-step confirmation for clearing outputs
                        let clear_armed = self
                            .confirm_clear
                            .is_some_and(|at| at.elapsed() <= Duration::from_secs(4));
                        if ui
                            .button(if clear_armed {
                                t(lang, "지우기 확인")
                            } else {
                                t(lang, "출력 지우기")
                            })
                            .clicked()
                        {
                            if clear_armed {
                                self.push_undo();
                                for node in self.nodes.iter_mut() {
                                    node.output.clear();
                                }
                                self.confirm_clear = None;
                                self.status = t(lang, "모든 출력을 지웠습니다.").to_string();
                                self.mark_dirty();
                            } else {
                                self.confirm_clear = Some(Instant::now());
                                self.status = t(lang, "한 번 더 누르면 지워집니다.").to_string();
                            }
                        }
                    });
                egui::CollapsingHeader::new(t(lang, "저장"))
                    .default_open(true)
                    .id_salt("cat-save")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .button(t(lang, "내보내기"))
                                .on_hover_text(t(lang, "캔버스를 JSON 파일로 저장합니다"))
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("JSON", &["json"])
                                    .set_file_name("symphony-canvas.json")
                                    .save_file()
                                {
                                    match save_to(&path, &self.snapshot()) {
                                        Ok(()) => {
                                            self.status =
                                                t(lang, "캔버스를 내보냈습니다.").to_string()
                                        }
                                        Err(e) => {
                                            self.status = t(lang, "자동 저장 실패: {e}")
                                                .replace("{e}", &e.to_string())
                                        }
                                    }
                                }
                            }
                            if ui
                                .button(t(lang, "가져오기"))
                                .on_hover_text(t(lang, "JSON 파일에서 캔버스를 불러옵니다"))
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("JSON", &["json"])
                                    .pick_file()
                                {
                                    match load_from(&path) {
                                        Ok(doc) => self.apply_doc(doc),
                                        Err(e) => {
                                            self.status = t(lang, "가져오기 실패: {e}")
                                                .replace("{e}", &e.to_string())
                                        }
                                    }
                                }
                            }
                        });
                    });
                egui::CollapsingHeader::new(t(lang, "설정"))
                    .default_open(true)
                    .id_salt("cat-config")
                    .show(ui, |ui| {
                        if ui.button(t(lang, "설정")).clicked() {
                            self.show_settings = true;
                        }
                        if ui.button(t(lang, "정보")).clicked() {
                            self.show_about = true;
                        }
                    });
                hud_rule(ui, pal.accent);
                ui.label(subtle(t(lang, "빈 곳 드래그: 캔버스 이동")));
                ui.label(subtle(t(lang, "타이틀바 드래그: 노드 이동")));
                ui.label(subtle(t(
                    lang,
                    "노드 입력을 연결하면 출력이 다음 노드로 이어집니다",
                )));
            });
    }

    pub(crate) fn render_results(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        let pal = palette(self.config.theme);
        // ── Right live results ─────────────────────────────────
        if self.results_open {
            egui::SidePanel::right("results_panel")
                .resizable(true)
                .default_width(300.0)
                .frame(hud_panel_frame(pal.result, pal.faint))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(section_title(t(lang, "전체 결과")));
                        let done = self
                            .nodes
                            .iter()
                            .filter(|n| !n.output.trim().is_empty())
                            .count();
                        pill(
                            ui,
                            format!("{done}/{}", self.nodes.len()),
                            pal.faint,
                            INK_SUB,
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t(lang, "닫기")).clicked() {
                                self.results_open = false;
                            }
                        });
                    });
                    hud_rule(ui, pal.accent);
                    // ── Selected-node details: clicking a node on the canvas shows it here ──
                    let mut inspector_run: Option<usize> = None;
                    // Inspector edits: applied after the snapshot.
                    let mut insp_need_undo = false;
                    let mut insp_late: Vec<InspBit> = Vec::new();
                    // Drop vanished node ids from the selection.
                    self.selected
                        .retain(|id| self.nodes.iter().any(|n| n.id == *id));
                    if self.selected.len() > 1 {
                        // Multi-select: batch action card.
                        let n = self.selected.len();
                        hud_card_frame(pal.window, pal.faint).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(section_title(t(lang, "선택 노드")));
                                pill(
                                    ui,
                                    t(lang, "{n}개 선택됨").replace("{n}", &n.to_string()),
                                    pal.faint,
                                    INK_SUB,
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(t(lang, "선택 해제")).clicked() {
                                            self.selected.clear();
                                        }
                                    },
                                );
                            });
                            ui.horizontal(|ui| {
                                if ui.button(t(lang, "모두 켜기")).clicked() {
                                    self.push_undo();
                                    let ids: Vec<usize> = self.selected.iter().copied().collect();
                                    for node in self.nodes.iter_mut() {
                                        if ids.contains(&node.id) {
                                            node.enabled = true;
                                        }
                                    }
                                    self.dirty = true;
                                }
                                if ui.button(t(lang, "모두 끄기")).clicked() {
                                    self.push_undo();
                                    let ids: Vec<usize> = self.selected.iter().copied().collect();
                                    for node in self.nodes.iter_mut() {
                                        if ids.contains(&node.id) {
                                            node.enabled = false;
                                        }
                                    }
                                    self.dirty = true;
                                }
                                if ui.button(t(lang, "모두 삭제")).clicked() {
                                    self.delete_selected(lang);
                                }
                            });
                        });
                        ui.add_space(6.0);
                    }
                    if self.selected.len() == 1 {
                        let sel = *self.selected.iter().next().unwrap();
                        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == sel) {
                            hud_card_frame(pal.window, pal.faint).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(section_title(t(lang, "선택 노드")));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button(t(lang, "선택 해제")).clicked() {
                                                self.selected.clear();
                                            }
                                        },
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(t(lang, "제목"));
                                    let title_resp = ui.add(
                                        egui::TextEdit::singleline(&mut node.title)
                                            .desired_width(180.0),
                                    );
                                    if title_resp.gained_focus() {
                                        insp_need_undo = true;
                                    }
                                    if title_resp.changed() {
                                        self.dirty = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    let toggle = egui::Button::new(if node.enabled {
                                        t(lang, "ON: 실행함")
                                    } else {
                                        t(lang, "OFF: 건너뜀")
                                    });
                                    if ui.add(toggle).clicked() {
                                        insp_late.push(InspBit::Toggle);
                                    }
                                });
                                ui.label(t(lang, node.mode.prompt_label_key()));
                                let insp_prompt = ui.add(
                                    egui::TextEdit::multiline(&mut node.prompt)
                                        .desired_rows(5)
                                        .hint_text(t(lang, node.mode.prompt_hint_key()))
                                        .desired_width(f32::INFINITY),
                                );
                                if insp_prompt.gained_focus() {
                                    insp_need_undo = true;
                                }
                                if insp_prompt.changed() {
                                    self.touched.insert(sel);
                                    self.dirty = true;
                                }
                                ui.horizontal(|ui| {
                                    ui.label(t(lang, "모델:"));
                                    let mut m = node.model.clone().unwrap_or_default();
                                    let insp_model = ui.add(
                                        egui::TextEdit::singleline(&mut m)
                                            .desired_width(150.0)
                                            .hint_text(self.config.model.clone()),
                                    );
                                    if insp_model.gained_focus() {
                                        insp_need_undo = true;
                                    }
                                    if insp_model.changed() {
                                        let trimmed = m.trim().to_string();
                                        node.model = if trimmed.is_empty() {
                                            None
                                        } else {
                                            Some(trimmed)
                                        };
                                        self.dirty = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(t(lang, "온도:"));
                                    let mut tmp = node.temperature.unwrap_or(1.0);
                                    let insp_temp = ui.add(egui::Slider::new(&mut tmp, 0.0..=2.0));
                                    if insp_temp.drag_started() {
                                        insp_need_undo = true;
                                    }
                                    if insp_temp.changed() {
                                        node.temperature = Some(tmp.clamp(0.0, 2.0));
                                        self.dirty = true;
                                    }
                                    if ui.button(t(lang, "기본값")).clicked() {
                                        insp_late.push(InspBit::ResetTemp);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(t(lang, "최대 토큰:"));
                                    let mut mt = node.max_tokens.unwrap_or(0);
                                    let insp_tokens =
                                        ui.add(egui::DragValue::new(&mut mt).range(0.0..=32768.0));
                                    if insp_tokens.drag_started() {
                                        insp_need_undo = true;
                                    }
                                    if insp_tokens.changed() {
                                        node.max_tokens = if mt == 0 { None } else { Some(mt) };
                                        self.dirty = true;
                                    }
                                    if ui.button(t(lang, "기본값")).clicked() {
                                        insp_late.push(InspBit::ResetTokens);
                                    }
                                });
                                if hud_button(ui, t(lang, "▶ 실행"), pal.accent).clicked() {
                                    inspector_run = Some(sel);
                                }
                            });
                        }
                        ui.add_space(6.0);
                    }
                    if let Some(id) = inspector_run {
                        self.spawn_generate(id);
                    }
                    // Apply inspector edits: mutate after the snapshot.
                    if insp_need_undo || !insp_late.is_empty() {
                        self.push_undo();
                    }
                    if !insp_late.is_empty() {
                        if let Some(&sel) = self.selected.iter().next() {
                            for bit in insp_late {
                                match bit {
                                    InspBit::Toggle => {
                                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == sel)
                                        {
                                            n.enabled = !n.enabled;
                                        }
                                    }
                                    InspBit::ResetTemp => {
                                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == sel)
                                        {
                                            n.temperature = None;
                                        }
                                    }
                                    InspBit::ResetTokens => {
                                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == sel)
                                        {
                                            n.max_tokens = None;
                                        }
                                    }
                                }
                            }
                        }
                        self.dirty = true;
                    }
                    // Main: shows the combined full output large at the top.
                    output_frame(ctx).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(combined_results(&self.nodes))
                                        .wrap()
                                        .selectable(true),
                                );
                            });
                    });
                    if ui
                        .add_sized(
                            [ui.available_width(), 26.0],
                            egui::Button::new(egui::RichText::new(t(lang, "전체 복사")).strong()),
                        )
                        .clicked()
                    {
                        ctx.copy_text(combined_results(&self.nodes));
                        self.status = t(lang, "전체 결과를 복사했습니다.").to_string();
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), 26.0],
                            egui::Button::new(t(lang, "새창으로 열기")),
                        )
                        .clicked()
                    {
                        self.show_combined_zoom = true;
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), 26.0],
                            egui::Button::new(t(lang, "라이브 결과창 열기")),
                        )
                        .clicked()
                    {
                        self.show_results_window = true;
                    }
                    // Share: mail, SNS, text, document
                    ui.horizontal_wrapped(|ui| {
                        ui.label(subtle(t(lang, "공유:")));
                        if ui.button(t(lang, "메일")).clicked() {
                            let url = share::mailto_url(
                                "Symphony Canvas 결과",
                                &combined_results(&self.nodes),
                            );
                            if open::that(&url).is_ok() {
                                self.status = t(lang, "메일 앱을 열었습니다.").to_string();
                            } else {
                                self.status = t(lang, "메일 앱을 열지 못했습니다.").to_string();
                            }
                        }
                        if ui.button("Facebook").clicked() {
                            // No prefilled-text support, so copy the caption and open the home page.
                            ctx.copy_text(combined_results(&self.nodes));
                            let _ = open::that(share::facebook_url());
                            self.status = t(lang, "캡션을 복사했습니다. 페이스북에 붙여넣으세요.")
                                .to_string();
                        }
                        if ui.button("Threads").clicked() {
                            let url = share::threads_url(&combined_results(&self.nodes));
                            let _ = open::that(&url);
                            self.status = t(lang, "Threads 작성 창을 열었습니다.").to_string();
                        }
                        if ui.button("Instagram").clicked() {
                            ctx.copy_text(combined_results(&self.nodes));
                            let _ = open::that(share::instagram_url());
                            self.status =
                                t(lang, "캡션을 복사했습니다. 인스타그램에 붙여넣으세요.")
                                    .to_string();
                        }
                        if ui.button(t(lang, "문서 저장")).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Markdown", &["md"])
                                .add_filter("Text", &["txt"])
                                .set_file_name("symphony-result.md")
                                .save_file()
                            {
                                let text = share::to_markdown(
                                    "Symphony Canvas 결과",
                                    &combined_results(&self.nodes),
                                );
                                match std::fs::write(&path, text) {
                                    Ok(()) => {
                                        self.status = t(lang, "문서를 저장했습니다.").to_string()
                                    }
                                    Err(e) => {
                                        self.status = t(lang, "자동 저장 실패: {e}")
                                            .replace("{e}", &e.to_string())
                                    }
                                }
                            }
                        }
                    });
                    ui.separator();
                    ui.label(section_title(t(lang, "노드별 출력")));
                    ui.label(subtle(t(lang, "실행되면 여기에 실시간으로 모입니다")));
                    let mut row_rects: Vec<(usize, [f32; 4])> = Vec::new();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for node in &self.nodes {
                            let busy = self.pending.contains_key(&node.id);
                            let row = ui.horizontal(|ui| {
                                let dot = if node.enabled { "●" } else { "○" };
                                ui.label(egui::RichText::new(dot).color(if node.enabled {
                                    META_BLUE
                                } else {
                                    INK_SUB
                                }));
                                ui.label(egui::RichText::new(&node.title).strong());
                                if busy {
                                    ui.spinner();
                                    ui.label(subtle(t(lang, "생성 중…")));
                                } else {
                                    ui.label(
                                        egui::RichText::new(
                                            t(lang, "{}회")
                                                .replace("{}", &node.run_count.to_string()),
                                        )
                                        .monospace()
                                        .color(INK_SUB),
                                    );
                                }
                            });
                            let rr = row.response.rect;
                            row_rects.push((node.id, [rr.min.x, rr.min.y, rr.max.x, rr.max.y]));
                            // Generated/attached images also show as thumbnails in results.
                            if let Some(path) = node.image_path.clone() {
                                if let Some(tex) =
                                    load_node_image(ctx, &mut self.node_textures, node.id, &path)
                                {
                                    output_frame(ctx).show(ui, |ui| {
                                        ui.add(
                                            egui::Image::new(&tex)
                                                .max_width(ui.available_width())
                                                .max_height(self.config.image_h),
                                        );
                                    });
                                } else {
                                    ui.label(subtle(t(lang, "(이미지를 찾을 수 없음)")));
                                }
                            }
                            if node.output.is_empty() {
                                ui.label(subtle(t(lang, "(출력 없음)")));
                            } else {
                                output_frame(ctx).show(ui, |ui| {
                                    ui.add(egui::Label::new(&node.output).wrap().selectable(true));
                                });
                            }
                            ui.separator();
                        }
                    });
                    self.result_row_rects.clear();
                    self.result_row_rects.extend(row_rects);
                });
        }
    }

    pub(crate) fn render_canvas(&mut self, ctx: &egui::Context, press_on_port: bool) {
        let lang = self.config.lang;
        let pal = palette(self.config.theme);
        // ── Canvas ───────────────────────────────────────────

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default().fill(canvas_color(self.config.theme, self.config.canvas_bg)),
            )
            .show(ctx, |ui| {
                // Dot grid
                let rect = ui.max_rect();
                self.central_rect = Some([rect.min.x, rect.min.y, rect.max.x, rect.max.y]);
                // Mouse wheel: zoom over the canvas (cursor-anchored). Only plain wheel here;
                // Ctrl+wheel and pinch go through the smooth-zoom path (app.rs).
                // Over in-card scroll areas, keep content scrolling alive;
                // never zoom over overlays such as modals or the minimap.
                let ctrl = ctx.input(|i| i.modifiers.ctrl);
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel != 0.0 && !ctrl {
                    if let Some(mouse) = ctx.pointer_hover_pos() {
                        let mouse_xy = [mouse.x, mouse.y];
                        let over_scroll = self
                            .scroll_zones
                            .values()
                            .any(|zones| point_in_zones(mouse_xy, zones));
                        let over_overlay = self
                            .overlay_rects
                            .iter()
                            .any(|&r| point_in_rect(mouse_xy, r));
                        let in_canvas = rect.contains(mouse);
                        if wheel_should_zoom(in_canvas, over_scroll, over_overlay) {
                            let new_zoom = clamp_zoom(self.zoom * zoom_factor_for_wheel(wheel));
                            // Adjust panning so the point under the cursor stays fixed
                            self.canvas =
                                anchored_canvas(self.canvas, mouse_xy, self.zoom, new_zoom);
                            self.zoom = new_zoom;
                            self.mark_dirty();
                        }
                    }
                }
                // Map boundary board: lay a visible-bordered board over the infinite background.
                {
                    let bw = self.config.board_w.max(400.0);
                    let bh = self.config.board_h.max(400.0);
                    let z = self.zoom.max(0.001);
                    let board = egui::Rect::from_min_max(
                        egui::Pos2::new(self.canvas[0], self.canvas[1]),
                        egui::Pos2::new(self.canvas[0] + bw * z, self.canvas[1] + bh * z),
                    );
                    ui.painter().rect_filled(board, 0.0, pal.window);
                    ui.painter().rect_stroke(
                        board,
                        0.0,
                        egui::Stroke::new(2.0_f32, pal.accent),
                        egui::StrokeKind::Middle,
                    );
                }
                // Adaptive grid: pick a step so on-screen spacing is ~30px.
                {
                    let bgc = canvas_color(self.config.theme, self.config.canvas_bg);
                    let gc = grid_color(bgc);
                    let zoom_now = self.zoom;
                    let cv = self.canvas;
                    if self.config.grid != GridStyle::None && zoom_now > 0.001 {
                        let raw = 30.0 / zoom_now;
                        let mag = 10_f32.powf(raw.log10().floor());
                        let mult = raw / mag;
                        let step = if mult >= 5.0 {
                            5.0 * mag
                        } else if mult >= 2.0 {
                            2.0 * mag
                        } else {
                            mag
                        };
                        let s = (step * zoom_now).max(4.0);
                        let x0 = rect.min.x - (rect.min.x - cv[0]).rem_euclid(s);
                        let y0 = rect.min.y - (rect.min.y - cv[1]).rem_euclid(s);
                        match self.config.grid {
                            GridStyle::Dots => {
                                let mut x = x0;
                                while x < rect.max.x {
                                    let mut y = y0;
                                    while y < rect.max.y {
                                        ui.painter().circle_filled(egui::Pos2::new(x, y), 1.5, gc);
                                        y += s;
                                    }
                                    x += s;
                                }
                            }
                            GridStyle::Lines => {
                                let mut x = x0;
                                while x < rect.max.x {
                                    ui.painter().line_segment(
                                        [
                                            egui::Pos2::new(x, rect.min.y),
                                            egui::Pos2::new(x, rect.max.y),
                                        ],
                                        egui::Stroke::new(1.0_f32, gc),
                                    );
                                    x += s;
                                }
                                let mut y = y0;
                                while y < rect.max.y {
                                    ui.painter().line_segment(
                                        [
                                            egui::Pos2::new(rect.min.x, y),
                                            egui::Pos2::new(rect.max.x, y),
                                        ],
                                        egui::Stroke::new(1.0_f32, gc),
                                    );
                                    y += s;
                                }
                            }
                            GridStyle::None => {}
                        }
                    }
                }
                // Drag empty space = move canvas. Shift+drag = box select.
                let bg = ui.interact(rect, Id::new("canvas-bg"), Sense::click_and_drag());
                let shift = ctx.input(|i| i.modifiers.shift);
                if bg.drag_started() && shift && self.linking.is_none() {
                    if let Some(m) = ctx.pointer_latest_pos() {
                        self.band_start = Some([m.x, m.y]);
                        // Additive-ness is fixed at gesture start.
                        // Releasing Shift just before drop won't lose the selection.
                        self.band_additive = true;
                    }
                }
                // Drags starting on a port are for linking, so the canvas does not move.
                // press_on_port is true only on the press frame, so in-progress is tracked separately.
                // The canvas also stays put during box selection.
                let link_dragging = self.linking.is_some()
                    && self.press_pos.is_some()
                    && ctx.input(|i| i.pointer.primary_down());
                if bg.dragged() && !press_on_port && !link_dragging && self.band_start.is_none() {
                    let d = bg.drag_delta();
                    self.canvas[0] += d.x;
                    self.canvas[1] += d.y;
                    self.mark_dirty();
                }
                // Box select: draw the band, and on release select overlapping nodes.
                if let Some(start) = self.band_start {
                    if let Some(m) = ctx.pointer_latest_pos() {
                        let band = band_rect(start, [m.x, m.y]);
                        let band_rect_egui = egui::Rect::from_min_max(
                            egui::Pos2::new(band[0], band[1]),
                            egui::Pos2::new(band[2], band[3]),
                        );
                        ui.painter().rect_filled(
                            band_rect_egui,
                            egui::CornerRadius::ZERO,
                            egui::Color32::from_rgba_unmultiplied(0, 100, 224, 24),
                        );
                        ui.painter().rect_stroke(
                            band_rect_egui,
                            egui::CornerRadius::ZERO,
                            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0, 100, 224)),
                            egui::StrokeKind::Middle,
                        );
                        if bg.drag_stopped() {
                            let rects: Vec<(usize, [f32; 4])> = self
                                .nodes
                                .iter()
                                .filter_map(|n| n.last_rect.map(|r| (n.id, r)))
                                .collect();
                            let hit = select_in_rect(&rects, band);
                            if self.band_additive {
                                for id in hit {
                                    self.selected.insert(id);
                                }
                            } else {
                                self.selected = hit.into_iter().collect();
                            }
                            self.band_start = None;
                        }
                    } else {
                        self.band_start = None;
                    }
                }
                // Click empty space = deselect. Shift+click toggles just that card.
                if bg.clicked() {
                    if shift {
                        if let Some(m) = ctx.pointer_latest_pos() {
                            let mouse_xy = [m.x, m.y];
                            // Test overlapping cards topmost-first (from the back of the vector).
                            if let Some(hit) =
                                self.nodes.iter().rev().find(|n| {
                                    n.last_rect.is_some_and(|r| point_in_rect(mouse_xy, r))
                                })
                            {
                                let id = hit.id;
                                if !self.selected.remove(&id) {
                                    self.selected.insert(id);
                                }
                            }
                        }
                    } else {
                        self.selected.clear();
                    }
                }
                // Node link lines (based on previous-frame rects)
                let link_stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 100, 224));
                // Link-line actions (left-click unlink, right-click menu). Batch-processed after the loop.
                let mut link_actions: Vec<LinkMenuAction> = Vec::new();
                for node in &self.nodes {
                    let Some(dst) = node.last_rect else {
                        continue;
                    };
                    // Spread multiple input lines vertically so they don't overlap the input port.
                    let count = node.inputs.len();
                    for (k, src_id) in node.inputs.iter().copied().enumerate() {
                        let Some(src) = self.nodes.iter().find(|n| n.id == src_id) else {
                            continue;
                        };
                        let Some(sr) = src.last_rect else {
                            continue;
                        };
                        let op = out_port(sr);
                        let ip = in_port(dst);
                        let spread = (k as f32 - (count as f32 - 1.0) / 2.0) * 10.0;
                        let p0 = egui::Pos2::new(op[0], op[1]);
                        let p1 = egui::Pos2::new(ip[0], ip[1] + spread);
                        let mx = (p0.x + p1.x) / 2.0;
                        ui.painter()
                            .line_segment([p0, egui::Pos2::new(mx, p0.y)], link_stroke);
                        ui.painter().line_segment(
                            [egui::Pos2::new(mx, p0.y), egui::Pos2::new(mx, p1.y)],
                            link_stroke,
                        );
                        ui.painter()
                            .line_segment([egui::Pos2::new(mx, p1.y), p1], link_stroke);
                        ui.painter().circle_filled(p0, 4.0, link_stroke.color);
                        // Direction arrowhead (input-port side)
                        let arrow = egui::epaint::PathShape::convex_polygon(
                            vec![
                                p1,
                                egui::Pos2::new(p1.x - 10.0, p1.y - 5.0),
                                egui::Pos2::new(p1.x - 10.0, p1.y + 5.0),
                            ],
                            link_stroke.color,
                            egui::Stroke::NONE,
                        );
                        ui.painter().add(arrow);
                        // For meshed (bidirectional) links, also draw a reverse arrowhead at the output port.
                        let mutual = self
                            .nodes
                            .iter()
                            .any(|n| n.id == src_id && n.inputs.contains(&node.id));
                        if mutual {
                            let back = egui::epaint::PathShape::convex_polygon(
                                vec![
                                    p0,
                                    egui::Pos2::new(p0.x + 10.0, p0.y - 5.0),
                                    egui::Pos2::new(p0.x + 10.0, p0.y + 5.0),
                                ],
                                link_stroke.color,
                                egui::Stroke::NONE,
                            );
                            ui.painter().add(back);
                        }
                        // Elbow-midpoint ✕ button: hidden normally, shown only on hover.
                        let mid = egui::Pos2::new(mx, (p0.y + p1.y) / 2.0);
                        let hit_rect =
                            egui::Rect::from_center_size(mid, egui::vec2(UNLINK_HIT, UNLINK_HIT));
                        let link_resp = ui.interact(
                            hit_rect,
                            Id::new(("canvas-unlink", src_id, node.id, k)),
                            Sense::click(),
                        );
                        if link_resp.hovered() {
                            let red = egui::Color32::from_rgb(220, 38, 38);
                            ui.painter().circle_filled(mid, 9.0, egui::Color32::WHITE);
                            ui.painter()
                                .circle_stroke(mid, 9.0, egui::Stroke::new(1.5_f32, red));
                            let x = 9.0 * 0.55;
                            let x_stroke = egui::Stroke::new(2.0_f32, red);
                            ui.painter().line_segment(
                                [mid + egui::vec2(-x, -x), mid + egui::vec2(x, x)],
                                x_stroke,
                            );
                            ui.painter().line_segment(
                                [mid + egui::vec2(-x, x), mid + egui::vec2(x, -x)],
                                x_stroke,
                            );
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if link_resp.clicked() {
                            link_actions.push(LinkMenuAction::Unlink {
                                src: src_id,
                                dst: node.id,
                            });
                        }
                        // Right-click menu: unlink, mutual toggle, run both ends.
                        let menu_rows = self.link_menu_items(src_id, node.id, lang);
                        link_resp.context_menu(|ui| {
                            for row in &menu_rows {
                                match row {
                                    LinkMenuRow::Sep => {
                                        ui.separator();
                                    }
                                    LinkMenuRow::Item(it) => {
                                        let b = ui
                                            .add_enabled(it.enabled, egui::Button::new(&it.label));
                                        let b = match &it.tip {
                                            Some(tip) => b.on_hover_text(tip),
                                            None => b,
                                        };
                                        if b.clicked() {
                                            link_actions.push(it.action);
                                        }
                                    }
                                }
                            }
                        });
                        link_resp.on_hover_text(t(lang, "클릭: 연결 끊기 · 우클릭: 메뉴"));
                    }
                }
                for action in link_actions {
                    self.apply_link_menu(action, lang);
                }
                ui.label(subtle(t(
                    lang,
                    "빈 곳 드래그: 캔버스 이동 · 설정에서 AI·MCP 연결",
                )));
                ui.label(subtle(t(
                    lang,
                    "연결: 동그라미 드래그 · 카드의 → 연결 버튼 · L(선택 연결)",
                )));
                ui.label(subtle(t(lang, "휠: 확대/축소 · 긴 글 위는 내용 스크롤")));
                ui.label(subtle(t(
                    lang,
                    "Shift+드래그: 박스 선택 · 선 ×·카드 ×: 연결 끊기 · Delete: 선택 삭제",
                )));
                // Empty-canvas hint: nudge to add a node when there are none
                if self.nodes.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.label(section_title(t(lang, "아직 노드가 없습니다")));
                            ui.label(subtle(t(lang, "버튼을 눌러 첫 프롬프트를 만드세요")));
                            if theme_button(ui, t(lang, "＋ 첫 노드 만들기"), pal.accent).clicked()
                            {
                                self.add_node(
                                    t(lang, "프롬프트 {}").replace("{}", &self.next_id.to_string()),
                                    String::new(),
                                    true,
                                );
                            }
                        });
                    });
                }
                // Canvas boundary guardrails: dotted lines left/right mark the area.
                let edge = ui.visuals().widgets.noninteractive.bg_stroke.color;
                dotted_vline(
                    ui.painter(),
                    rect.min.x + 3.0,
                    rect.min.y + 4.0,
                    rect.max.y - 4.0,
                    edge,
                );
                dotted_vline(
                    ui.painter(),
                    rect.max.x - 3.0,
                    rect.min.y + 4.0,
                    rect.max.y - 4.0,
                    edge,
                );
                // During link mode, mark linkable cards with a glowing border.
                if let Some(src_id) = self.linking {
                    for node in &self.nodes {
                        if node.id == src_id {
                            continue;
                        }
                        if let Some(r) = node.last_rect {
                            ui.painter().rect_stroke(
                                egui::Rect::from_min_max(
                                    egui::Pos2::new(r[0] - 3.0, r[1] - 3.0),
                                    egui::Pos2::new(r[2] + 3.0, r[3] + 3.0),
                                ),
                                egui::CornerRadius::same(14),
                                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 100, 224)),
                                egui::StrokeKind::Middle,
                            );
                        }
                    }
                }
                // Selected-card marker: shows what is picked even with the results panel closed.
                for sel in self.selected.iter().copied().collect::<Vec<_>>() {
                    if let Some(node) = self.nodes.iter().find(|n| n.id == sel) {
                        if let Some(r) = node.last_rect {
                            ui.painter().rect_stroke(
                                egui::Rect::from_min_max(
                                    egui::Pos2::new(r[0] - 3.0, r[1] - 3.0),
                                    egui::Pos2::new(r[2] + 3.0, r[3] + 3.0),
                                ),
                                egui::CornerRadius::same(14),
                                egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
                                egui::StrokeKind::Middle,
                            );
                        }
                    }
                }
                // Link-mode temp line: from the source port to the cursor
                if let Some(src_id) = self.linking {
                    if let (Some(src), Some(mouse)) = (
                        self.nodes.iter().find(|n| n.id == src_id),
                        ctx.pointer_latest_pos(),
                    ) {
                        if let Some(sr) = src.last_rect {
                            let op = out_port(sr);
                            let p0 = egui::Pos2::new(op[0], op[1]);
                            ui.painter().line_segment(
                                [p0, mouse],
                                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 100, 224)),
                            );
                            ui.painter().circle_filled(
                                p0,
                                4.0,
                                egui::Color32::from_rgb(0, 100, 224),
                            );
                        }
                    }
                }
                // Node ports (circles): output = filled right, input = outlined left
                // Show a pointing-hand cursor over ports to signal linkability.
                {
                    let mouse = ctx.pointer_hover_pos();
                    let mut port_hot = false;
                    for node in &self.nodes {
                        let Some(r) = node.last_rect else {
                            continue;
                        };
                        let op = out_port(r);
                        let ip = in_port(r);
                        let out_c = egui::Pos2::new(op[0], op[1]);
                        let in_c = egui::Pos2::new(ip[0], ip[1]);
                        let (ar, ag, ab) = node.accent.rgb();
                        let accent = egui::Color32::from_rgb(ar, ag, ab);
                        // Always show input ports (emphasized in link mode)
                        let in_hot = self.linking.is_some_and(|s| s != node.id);
                        let in_r = if in_hot { 8.0 } else { 6.0 };
                        ui.painter().circle_filled(
                            in_c,
                            in_r,
                            if in_hot {
                                egui::Color32::from_rgb(0, 100, 224)
                            } else {
                                egui::Color32::WHITE
                            },
                        );
                        ui.painter().circle_stroke(
                            in_c,
                            in_r,
                            egui::Stroke::new(
                                2.0_f32,
                                if in_hot { egui::Color32::WHITE } else { accent },
                            ),
                        );
                        // Drop-target highlight: enlarge when the cursor is over the input port
                        if let Some(m) = mouse {
                            if self.linking.is_some_and(|s| s != node.id)
                                && m.distance(in_c) < PORT_HIT + 6.0
                            {
                                ui.painter().circle_stroke(
                                    in_c,
                                    in_r + 4.0,
                                    egui::Stroke::new(
                                        2.0_f32,
                                        egui::Color32::from_rgb(0, 100, 224),
                                    ),
                                );
                            }
                        }
                        // Output port: link start point
                        let out_hot = mouse.is_some_and(|m| m.distance(out_c) < PORT_HIT);
                        let in_hot = mouse.is_some_and(|m| m.distance(in_c) < PORT_HIT);
                        if out_hot || in_hot {
                            port_hot = true;
                        }
                        ui.painter()
                            .circle_filled(out_c, if out_hot { 8.0 } else { 6.5 }, accent);
                        ui.painter().circle_stroke(
                            out_c,
                            if out_hot { 8.0 } else { 6.5 },
                            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
                        );
                    }
                    if port_hot {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            });
    }

    pub(crate) fn render_minimap(&mut self, ctx: &egui::Context) {
        let pal = palette(self.config.theme);
        // ── Minimap (bottom-right): all nodes + current view ──
        if self.show_minimap && !self.nodes.is_empty() {
            if let Some(cr) = self.central_rect {
                let z = self.zoom.max(0.01);
                let c = self.canvas;
                let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                let mut rects: Vec<[f32; 4]> = Vec::new();
                for n in &self.nodes {
                    let r = match n.last_rect {
                        Some(r) => [
                            (r[0] - c[0]) / z,
                            (r[1] - c[1]) / z,
                            (r[2] - c[0]) / z,
                            (r[3] - c[1]) / z,
                        ],
                        None => [n.pos[0], n.pos[1], n.pos[0] + 300.0, n.pos[1] + 200.0],
                    };
                    rects.push(r);
                    bx0 = bx0.min(r[0]);
                    by0 = by0.min(r[1]);
                    bx1 = bx1.max(r[2]);
                    by1 = by1.max(r[3]);
                }
                // Include the current view as well.
                let vx = [(cr[0] - c[0]) / z, (cr[2] - c[0]) / z];
                let vy = [(cr[1] - c[1]) / z, (cr[3] - c[1]) / z];
                bx0 = bx0.min(vx[0]);
                by0 = by0.min(vy[0]);
                bx1 = bx1.max(vx[1]);
                by1 = by1.max(vy[1]);
                let bw = (bx1 - bx0).max(1.0);
                let bh = (by1 - by0).max(1.0);
                let map_w = 150.0_f32;
                let map_h = 100.0_f32;
                let sc = (map_w / bw).min(map_h / bh);
                let ow = bw * sc;
                let oh = bh * sc;
                egui::Area::new(Id::new("minimap"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0))
                    .show(ctx, |ui| {
                        egui::Frame::default()
                            .fill(pal.window)
                            .stroke(egui::Stroke::new(1.0_f32, pal.faint))
                            .corner_radius(egui::CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(6))
                            .show(ui, |ui| {
                                let (mr, resp) = ui.allocate_exact_size(
                                    egui::vec2(map_w, map_h),
                                    Sense::click_and_drag(),
                                );
                                // Block canvas wheel zoom over the minimap.
                                self.overlay_staging
                                    .push([mr.min.x, mr.min.y, mr.max.x, mr.max.y]);
                                let ox = mr.min.x + (map_w - ow) / 2.0;
                                let oy = mr.min.y + (map_h - oh) / 2.0;
                                let mp = |x: f32, y: f32| {
                                    egui::Pos2::new(ox + (x - bx0) * sc, oy + (y - by0) * sc)
                                };
                                for r in &rects {
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_max(mp(r[0], r[1]), mp(r[2], r[3])),
                                        2.0,
                                        pal.faint,
                                    );
                                }
                                ui.painter().rect_stroke(
                                    egui::Rect::from_min_max(mp(vx[0], vy[0]), mp(vx[1], vy[1])),
                                    2.0,
                                    egui::Stroke::new(2.0_f32, pal.accent),
                                    egui::StrokeKind::Middle,
                                );
                                if resp.clicked() || resp.dragged() {
                                    if let Some(mpos) = resp.interact_pointer_pos() {
                                        let px = bx0 + (mpos.x - ox) / sc;
                                        let py = by0 + (mpos.y - oy) / sc;
                                        let cx = (cr[0] + cr[2]) / 2.0;
                                        let cy = (cr[1] + cr[3]) / 2.0;
                                        self.canvas[0] = cx - px * self.zoom;
                                        self.canvas[1] = cy - py * self.zoom;
                                        self.mark_dirty();
                                    }
                                }
                            });
                    });
            }
        }
    }

    pub(crate) fn render_result_tab(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        let pal = palette(self.config.theme);
        // ── Results open tab (right edge when closed) ──
        if !self.results_open {
            egui::Area::new(Id::new("results-tab"))
                .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-4.0, 0.0))
                .show(ctx, |ui| {
                    if theme_button(ui, t(lang, "◀ 결과"), pal.accent)
                        .on_hover_text(t(lang, "결과 패널 열기 (Ctrl+R)"))
                        .clicked()
                    {
                        self.results_open = true;
                    }
                });
        }
    }

    /// Results-row click layer: receives row clicks above the cards (Foreground).
    /// Card widgets swallow panel-area clicks, so rows themselves never get them.
    /// Skip rows buried under a modal (previous-frame overlay).
    /// Live results standalone window content: running status + view options + results.
    /// Redrawn every frame as the OS window body, so it is always current.
    pub(crate) fn render_results_window(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let lang = self.config.lang;
        let pal = palette(self.config.theme);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("●").color(pal.accent).size(14.0));
            ui.label(section_title(t(lang, "라이브 결과")));
        });
        hud_rule(ui, pal.accent);
        // Running status (live).
        if !self.pending.is_empty() {
            let mut running: Vec<(String, u64)> = self
                .pending
                .iter()
                .filter_map(|(id, job)| {
                    let node = self.nodes.iter().find(|n| n.id == *id)?;
                    Some((node.title.clone(), job.started.elapsed().as_secs()))
                })
                .collect();
            running.sort();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    egui::RichText::new(format!("{} ({})", t(lang, "실행 중"), running.len()))
                        .strong(),
                );
            });
            for (title, secs) in running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&title);
                    ui.label(subtle(format!("{secs}{}", t(lang, "초"))));
                });
            }
            ui.separator();
        }
        // View options.
        let text_cfg_before = (self.config.text_size, self.config.text_mono);
        ui.horizontal_wrapped(|ui| {
            ui.label(subtle(t(lang, "보기:")));
            ui.selectable_value(
                &mut self.results_win_mode,
                crate::state::ResultsWinMode::PerNode,
                t(lang, "노드별"),
            );
            ui.selectable_value(
                &mut self.results_win_mode,
                crate::state::ResultsWinMode::Combined,
                t(lang, "합침"),
            );
            ui.separator();
            ui.label(subtle(t(lang, "필터:")));
            ui.selectable_value(
                &mut self.results_win_filter,
                crate::state::ResultsWinFilter::All,
                t(lang, "전체"),
            );
            ui.selectable_value(
                &mut self.results_win_filter,
                crate::state::ResultsWinFilter::Running,
                t(lang, "실행 중"),
            );
            ui.selectable_value(
                &mut self.results_win_filter,
                crate::state::ResultsWinFilter::Errors,
                t(lang, "오류만"),
            );
            ui.separator();
            ui.checkbox(&mut self.results_win_follow, t(lang, "자동 스크롤"));
            ui.checkbox(&mut self.results_win_wrap, t(lang, "자동 줄바꿈"));
            ui.separator();
            ui.label(subtle(t(lang, "글자 크기")));
            ui.add(egui::Slider::new(&mut self.config.text_size, 10.0..=28.0).suffix("px"));
            ui.checkbox(&mut self.config.text_mono, t(lang, "고정폭"));
        });
        if (self.config.text_size, self.config.text_mono) != text_cfg_before {
            self.mark_dirty();
        }
        ui.separator();
        let follow = self.results_win_follow;
        let wrap = self.results_win_wrap;
        match self.results_win_mode {
            crate::state::ResultsWinMode::Combined => {
                let text = combined_results(&self.nodes);
                let body = output_text(&text, self.config.text_size, self.config.text_mono);
                output_frame(ctx).show(ui, |ui| {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .scroll_source(text_scroll_source())
                        .stick_to_bottom(follow)
                        .show(ui, |ui| {
                            if wrap {
                                ui.add(egui::Label::new(body).wrap().selectable(true));
                            } else {
                                ui.add(egui::Label::new(body).selectable(true));
                            }
                        });
                });
            }
            crate::state::ResultsWinMode::PerNode => {
                let ids = self.filter_result_nodes(self.results_win_filter);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_source(text_scroll_source())
                    .stick_to_bottom(follow)
                    .show(ui, |ui| {
                        for id in ids {
                            let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                let dot = if node.enabled { "●" } else { "○" };
                                ui.label(egui::RichText::new(dot).color(if node.enabled {
                                    META_BLUE
                                } else {
                                    INK_SUB
                                }));
                                ui.label(egui::RichText::new(&node.title).strong());
                                if self.pending.contains_key(&node.id) {
                                    ui.spinner();
                                    ui.label(subtle(t(lang, "생성 중…")));
                                }
                                if ui.button(t(lang, "복사")).clicked() {
                                    ctx.copy_text(node.output.clone());
                                    self.status =
                                        t(lang, "출력을 클립보드에 복사했습니다.").to_string();
                                }
                            });
                            if let Some(path) = node.image_path.clone() {
                                if let Some(tex) =
                                    load_node_image(ctx, &mut self.node_textures, node.id, &path)
                                {
                                    output_frame(ctx).show(ui, |ui| {
                                        ui.add(
                                            egui::Image::new(&tex)
                                                .max_width(ui.available_width())
                                                .max_height(self.config.image_h),
                                        );
                                    });
                                }
                            }
                            if node.output.is_empty() {
                                ui.label(subtle(t(lang, "(출력 없음)")));
                            } else {
                                let body = output_text(
                                    &node.output,
                                    self.config.text_size,
                                    self.config.text_mono,
                                );
                                output_frame(ctx).show(ui, |ui| {
                                    if wrap {
                                        ui.add(egui::Label::new(body).wrap().selectable(true));
                                    } else {
                                        ui.add(egui::Label::new(body).selectable(true));
                                    }
                                });
                            }
                            ui.separator();
                        }
                    });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if theme_button(ui, t(lang, "전체 복사"), pal.accent).clicked() {
                ctx.copy_text(combined_results(&self.nodes));
                self.status = t(lang, "전체 결과를 복사했습니다.").to_string();
            }
            if ui.button(t(lang, "닫기")).clicked() {
                self.show_results_window = false;
            }
        });
    }

    pub(crate) fn render_result_jump_layer(&mut self, ctx: &egui::Context) {
        if self.result_row_rects.is_empty() {
            return;
        }
        let lang = self.config.lang;
        let mut jump_to: Option<usize> = None;
        egui::Area::new(Id::new("result-jump-layer"))
            .order(egui::Order::Foreground)
            .movable(false)
            .show(ctx, |ui| {
                for (&id, r) in &self.result_row_rects {
                    if self
                        .overlay_rects
                        .iter()
                        .any(|o| rects_overlap(*r, *o, 0.0))
                    {
                        continue;
                    }
                    let rect =
                        egui::Rect::from_min_max(egui::pos2(r[0], r[1]), egui::pos2(r[2], r[3]));
                    let jump = ui.interact(rect, Id::new(("result-jump", id)), Sense::click());
                    if jump.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if jump.clicked() {
                        jump_to = Some(id);
                    }
                    jump.on_hover_text(t(lang, "클릭: 노드로 이동"));
                }
            });
        if let Some(id) = jump_to {
            self.selected.clear();
            self.selected.insert(id);
            self.focus_node(id);
        }
    }

    pub(crate) fn render_cards(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        let zoom = self.zoom;
        let canvas = self.canvas;
        // ── Node cards ────────────────────────────────────────
        let mut actions: Vec<(usize, NodeAction)> = Vec::new();
        // Collect in-card scroll-area screen rects: (node id, [x0, y0, x1, y1]).
        // Moved into self.scroll_zones at frame end (for wheel-zoom checks).
        let mut scroll_rects: Vec<(usize, [f32; 4])> = Vec::new();
        // Multi-select move deltas: (node id, [dx, dy]) in canvas coords. Applied after the loop.
        let mut group_moves: Vec<(usize, [f32; 2])> = Vec::new();
        // Card ids dropped this frame. Declumped after the loop.
        let mut released: Vec<usize> = Vec::new();
        // Gesture-start / focus-entered flag. One snapshot is taken after the loop.
        let mut need_undo = false;
        // Per-item commit actions. Applied after the snapshot.
        let mut late_bits: Vec<LateBit> = Vec::new();
        let mut status_msg: Option<String> = None;
        // Peer list for the input-link combobox (snapshot to avoid borrow conflicts in the loop)
        let peers: Vec<(usize, String)> =
            self.nodes.iter().map(|n| (n.id, n.title.clone())).collect();
        // Link-source output previews (150 chars) + MCP-connected snapshot
        let peer_preview: HashMap<usize, String> = self
            .nodes
            .iter()
            .map(|n| (n.id, n.output.chars().take(150).collect()))
            .collect();
        let mcp_online = !self.mcp.connected_names().is_empty();

        // Neighbor-rect snapshot for magnetic snap (avoids borrow conflicts)
        let neighbor_rects: Vec<(usize, [f32; 4])> = self
            .nodes
            .iter()
            .filter_map(|n| n.last_rect.map(|r| (n.id, r)))
            .collect();
        // Canvas rects for declumping (gesture start and drop both use this frame)
        let z0 = self.zoom.max(0.001);
        let canvas_rects: Vec<(usize, [f32; 4])> = self
            .nodes
            .iter()
            .map(|n| {
                let [w, h] = n
                    .last_rect
                    .map(|r| [(r[2] - r[0]) / z0, (r[3] - r[1]) / z0])
                    .unwrap_or([320.0, 420.0]);
                (n.id, [n.pos[0], n.pos[1], n.pos[0] + w, n.pos[1] + h])
            })
            .collect();
        for index in 0..self.nodes.len() {
            let node_id = self.nodes[index].id;

            // Unlimited map: node positions are not confined to the window.
            // Find far-away cards via minimap, fit (F), or search jump.
            let origin = egui::Pos2::new(
                self.nodes[index].pos[0] * zoom + self.canvas[0],
                self.nodes[index].pos[1] * zoom + self.canvas[1],
            );
            let busy = self.pending.contains_key(&node_id);
            let (ar, ag, ab) = self.nodes[index].accent.rgb();
            let accent = Color32::from_rgb(ar, ag, ab);
            let radius = self.nodes[index].radius.px();
            let node = &mut self.nodes[index];
            // constrain(false): the egui default (true) drags off-screen cards into the viewport,
            // piling them up at the edges. On an unlimited map pos is the truth, and
            // clamp_board / clamp_panel_x own the bounds at interaction time.
            // Card render clip: never covers the menu panel regardless of pan/spawn path.
            let clip = self.central_rect;
            let area_resp = egui::Area::new(Id::new(("prompt-node", node_id)))
                .movable(false)
                .constrain(false)
                .current_pos(origin)
                .show(ctx, |ui| {
                    // Don't draw or hit-test outside the central view.
                    if let Some(c) = clip {
                        ui.set_clip_rect(egui::Rect::from_min_max(
                            egui::pos2(c[0], c[1]),
                            egui::pos2(c[2], c[3]),
                        ));
                    }
                    // Highlight running cards with a blue border so they are easy to spot.
                    let mut frame = card_frame_styled(ctx, accent, radius);
                    if busy {
                        frame = frame.stroke(egui::Stroke::new(
                            2.5_f32,
                            egui::Color32::from_rgb(0, 100, 224),
                        ));
                    }
                    frame.show(ui, |ui| {
                        // Card width grows/shrinks with wheel zoom and the width scale.
                        // No upper bound.
                        // Only a minimum so it never drops to 0 or below.
                        let card_w = (300.0 * node.width_scale.max(0.05) * zoom).max(4.0);
                        let h_scale = node.height_scale.max(0.3);
                        ui.set_min_width(card_w - 20.0);
                        ui.set_max_width(card_w);
                        // Scale text and spacing with zoom: the card truly looks bigger.
                        // Clamp the range so text doesn't explode at extreme zoom.
                        let z_ui = zoom.clamp(0.4, 2.5);
                        {
                            let style = ui.style_mut();
                            for font in style.text_styles.values_mut() {
                                font.size *= z_ui;
                            }
                            style.spacing.item_spacing *= z_ui;
                            style.spacing.button_padding *= z_ui;
                            style.spacing.indent *= z_ui;
                            style.spacing.interact_size *= z_ui;
                        }

                        // Title bar: the whole bar is a drag handle.
                        // Title input etc. are drawn later, so they take priority.
                        // Without click_and_drag, clicks and double-clicks never register
                        // (selection, title editing, and link completion would all die).
                        let (bar_rect, bar) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), (32.0 * z_ui).max(20.0)),
                            Sense::click_and_drag(),
                        );
                        // Light up blue on hover to show it can be grabbed.
                        // Must be drawn before the contents so it doesn't cover the text.
                        let bar_hot = bar.hovered() || bar.dragged();
                        if bar_hot {
                            ui.painter().rect_filled(
                                bar_rect,
                                egui::CornerRadius::same(6),
                                META_BLUE_SOFT,
                            );
                        }
                        // Three dots mark the handle instead of a glyph missing from the font
                        let grip = bar_rect.left_center();
                        for dy in [-5.0_f32, 0.0, 5.0] {
                            ui.painter().circle_filled(
                                egui::Pos2::new(grip.x + 8.0, grip.y + dy * z_ui),
                                1.8 * z_ui,
                                accent,
                            );
                        }
                        {
                            let mut row = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
                            row.horizontal(|ui| {
                                ui.add_space(20.0);
                                let dot = if node.enabled { "●" } else { "○" };
                                let dot_color = if node.enabled { accent } else { INK_SUB };
                                ui.label(egui::RichText::new(dot).color(dot_color).strong());
                                // Title: normally a label (edit via bar double-click), dimmed when OFF
                                if self.editing_title == Some(node_id) {
                                    let edit = ui.add(
                                        egui::TextEdit::singleline(&mut node.title)
                                            .id(Id::new(("card-title", node_id)))
                                            .desired_width(
                                                (card_w - 120.0 * z_ui)
                                                    .clamp(60.0 * z_ui, 150.0 * z_ui),
                                            )
                                            .hint_text(t(lang, "제목")),
                                    );
                                    // Grab focus once right after entering edit mode.
                                    // Otherwise typing won't register even with the box open.
                                    if self.title_focus_pending {
                                        edit.request_focus();
                                        self.title_focus_pending = false;
                                    }
                                    if edit.changed() {
                                        self.touched.insert(node_id);
                                        self.dirty = true;
                                    }
                                    let done = edit.lost_focus()
                                        || (edit.has_focus()
                                            && ui.input(|i| {
                                                i.key_pressed(egui::Key::Enter)
                                                    || i.key_pressed(egui::Key::Escape)
                                            }));
                                    if done {
                                        self.editing_title = None;
                                    }
                                } else {
                                    let full = node.title.clone();
                                    let short = short_title(&full);
                                    let mut rt = egui::RichText::new(short).strong();
                                    if !node.enabled {
                                        rt = rt.color(INK_SUB);
                                    }
                                    ui.label(rt).on_hover_text(full).on_hover_text(t(
                                        lang,
                                        "드래그: 이동 · 더블클릭: 제목 편집",
                                    ));
                                }
                                // Unsaved-changes marker
                                if self.touched.contains(&node_id) {
                                    ui.label(
                                        egui::RichText::new("●")
                                            .color(egui::Color32::from_rgb(245, 158, 11))
                                            .strong(),
                                    )
                                    .on_hover_text(t(lang, "저장되지 않은 변경 있음"));
                                }
                                ui.label(subtle(
                                    t(lang, "{}회").replace("{}", &node.run_count.to_string()),
                                ));
                                // Title edit button (same action as bar double-click)
                                if ui
                                    .small_button(t(lang, "편집"))
                                    .on_hover_text(t(lang, "제목을 편집합니다"))
                                    .clicked()
                                {
                                    need_undo = true;
                                    self.editing_title = Some(node_id);
                                    self.title_focus_pending = true;
                                }
                                // Title-only fold/unfold
                                let fold = if node.title_only { "+" } else { "-" };
                                if ui
                                    .small_button(fold)
                                    .on_hover_text(t(lang, "제목만 표시 전환"))
                                    .clicked()
                                {
                                    node.title_only = !node.title_only;
                                    self.dirty = true;
                                }
                            });
                        }
                        // Bar double-click edits the title (doesn't conflict with drag)
                        if bar.double_clicked() && self.linking.is_none() {
                            need_undo = true;
                            self.editing_title = Some(node_id);
                            self.title_focus_pending = true;
                        }
                        if bar.dragged() {
                            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            let d = bar.drag_delta();
                            // With multi-select, move the co-grabbed cards by the same amount.
                            if self.selected.contains(&node_id) && self.selected.len() > 1 {
                                let step = [d.x / zoom, d.y / zoom];
                                for oid in self.selected.iter().copied() {
                                    if oid != node_id {
                                        group_moves.push((oid, step));
                                    }
                                }
                            }
                            let size = node.last_rect.map(|r| [r[2] - r[0], r[3] - r[1]]);

                            let pre = node.pos;
                            node.pos[0] += d.x / zoom;
                            node.pos[1] += d.y / zoom;
                            // Magnetic snap to neighbor card corners (screen coordinates)
                            let size = size.unwrap_or([320.0, 420.0]);
                            let screen_origin = [
                                node.pos[0] * zoom + canvas[0],
                                node.pos[1] * zoom + canvas[1],
                            ];
                            let others: Vec<[f32; 4]> = neighbor_rects
                                .iter()
                                .filter(|(id, _)| *id != node_id)
                                .map(|(_, r)| *r)
                                .collect();
                            // Scale the threshold with zoom (else it jumps the moment you grab while zoomed out).
                            let snapped = magnetic_snap(screen_origin, size, &others, 10.0 * zoom);
                            node.pos = [
                                (snapped[0] - canvas[0]) / zoom,
                                (snapped[1] - canvas[1]) / zoom,
                            ];
                            // One-sided guard: a wall inside, free outside (must not snap in the moment you grab).
                            let board = [self.config.board_w, self.config.board_h];
                            node.pos = clamp_board_step(
                                pre,
                                node.pos,
                                [size[0] / zoom, size[1] / zoom],
                                board,
                            );
                            node.pos = clamp_panel_x_step(
                                pre,
                                node.pos,
                                size[0],
                                self.central_rect,
                                self.canvas,
                                zoom,
                            );
                            self.status = t(lang, "'{}' 이동 중…").replace("{}", &node.title);
                        } else if bar.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::Grab);
                        }
                        if bar.drag_started() {
                            // Gesture-start snapshot: taken once after the loop.
                            need_undo = true;
                            // If the grabbed card is outside the selection, move the selection here.
                            if self.linking.is_none() && !self.selected.contains(&node_id) {
                                let shift = ctx.input(|i| i.modifiers.shift);
                                if !shift {
                                    self.selected.clear();
                                }
                                self.selected.insert(node_id);
                            }
                            // For drop declumping: record currently overlapping neighbors (only push new overlaps).
                            let sel_here: Vec<usize> =
                                if self.selected.len() > 1 && self.selected.contains(&node_id) {
                                    self.selected.iter().copied().collect()
                                } else {
                                    vec![node_id]
                                };
                            let overlapped: Vec<usize> = canvas_rects
                                .iter()
                                .filter(|(id, _)| !sel_here.contains(id))
                                .filter(|(_, r)| {
                                    sel_here.iter().any(|sid| {
                                        canvas_rects
                                            .iter()
                                            .find(|(i, _)| i == sid)
                                            .is_some_and(|(_, sr)| rects_overlap(*sr, *r, 0.0))
                                    })
                                })
                                .map(|(id, _)| *id)
                                .collect();
                            self.drop_baseline = Some((sel_here, overlapped));
                            actions.push((node_id, NodeAction::Front));
                        }
                        // No grid snap at drop: if the card jumped the moment you let go,
                        // interaction would feel stiff.
                        // Magnetic snap during drag already handles alignment.
                        if bar.drag_stopped() {
                            released.push(node_id);
                            self.dirty = true;
                        }
                        // Normal title-bar click = select node.
                        // Force-opening the results panel would jump the layout and
                        // break the editing flow, so select only and leave the panel alone.
                        // Shift+click toggles multi-select.
                        if self.linking.is_none() && bar.clicked() {
                            let shift = ctx.input(|i| i.modifiers.shift);
                            if shift {
                                if !self.selected.remove(&node_id) {
                                    self.selected.insert(node_id);
                                }
                            } else if self.selected.len() != 1 || !self.selected.contains(&node_id)
                            {
                                self.selected.clear();
                                self.selected.insert(node_id);
                            }
                        }
                        // Link mode: clicking this title bar makes it the link target.
                        if self.linking.is_some() && bar.clicked() {
                            if let Some(src) = self.linking {
                                if src == node_id {
                                    actions.push((node_id, NodeAction::DisarmLink));
                                } else {
                                    actions.push((node_id, NodeAction::LinkFrom(src)));
                                }
                            }
                        }
                        // While a link is armed, show a prominent connect button.
                        // Same action as a title-bar click, so pressing anywhere connects.
                        if self.linking.is_some_and(|s| s != node_id)
                            && ui
                                .add_sized(
                                    [ui.available_width(), 28.0 * z_ui],
                                    egui::Button::new(
                                        egui::RichText::new(t(lang, "← 이 노드에 연결")).strong(),
                                    ),
                                )
                                .on_hover_text(t(lang, "누르면 바로 연결됩니다"))
                                .clicked()
                        {
                            if let Some(src) = self.linking {
                                actions.push((node_id, NodeAction::LinkFrom(src)));
                            }
                        }
                        // Representative-image thumbnail (also shown in title-only mode)
                        if let Some(path) = node.image_path.clone() {
                            if let Some(tex) =
                                load_node_image(ctx, &mut self.node_textures, node_id, &path)
                            {
                                // Cap the height so tall portrait images don't take over the card.
                                ui.add(
                                    egui::Image::new(&tex)
                                        .max_width(card_w - 20.0)
                                        .max_height(180.0 * z_ui),
                                );
                            } else {
                                ui.label(subtle(t(lang, "(이미지를 찾을 수 없음)")));
                            }
                        }
                        if !node.title_only {
                            ui.separator();

                            // ON/OFF + progress status
                            ui.horizontal(|ui| {
                                let toggle_text = if node.enabled { "● ON" } else { "○ OFF" };
                                let toggle = egui::Button::new(
                                    egui::RichText::new(toggle_text)
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(if node.enabled {
                                    accent
                                } else {
                                    Color32::from_rgb(148, 163, 184)
                                });
                                if ui.add(toggle).clicked() {
                                    late_bits.push(LateBit::ToggleEnabled(node_id));
                                }
                                if busy {
                                    ui.spinner();
                                    ui.label(subtle(t(lang, "작업 중…")));
                                } else if !node.enabled {
                                    ui.label(subtle(t(lang, "OFF — 전체 실행 제외")));
                                }
                            });
                            // Per-node mode (22 kinds)
                            ui.horizontal(|ui| {
                                ui.label(t(lang, "모드:"));
                                egui::ComboBox::from_id_salt(("node-mode", node_id))
                                    .selected_text(t(lang, node.mode.label()))
                                    .show_ui(ui, |ui| {
                                        // The first 4 (general, agent, image, audio) are execution styles;
                                        // the rest are text presets. A separator keeps them findable.
                                        for (i, m) in NodeMode::ALL.iter().enumerate() {
                                            if i == 4 {
                                                ui.separator();
                                            }
                                            ui.selectable_value(
                                                &mut node.mode,
                                                *m,
                                                t(lang, m.label()),
                                            );
                                        }
                                    });
                                if node.mode == NodeMode::Agent {
                                    let mut steps = node.max_steps;
                                    ui.add(
                                        egui::DragValue::new(&mut steps)
                                            .range(1.0..=10.0)
                                            .suffix(t(lang, "단계")),
                                    );
                                    node.max_steps = steps.clamp(1, 10);
                                }
                            });
                            if node.mode == NodeMode::Agent && !mcp_online {
                                ui.label(
                                    egui::RichText::new(t(lang, "MCP 연결 필요"))
                                        .color(egui::Color32::from_rgb(217, 119, 6))
                                        .strong(),
                                );
                            }
                            ui.separator();

                            ui.label(t(lang, node.mode.prompt_label_key()));
                            let prompt_rows = (4.0 * h_scale).max(1.0).round() as usize;
                            let prompt_resp = ui.add(
                                egui::TextEdit::multiline(&mut node.prompt)
                                    .id(Id::new(("card-prompt", node_id)))
                                    .hint_text(t(lang, node.mode.prompt_hint_key()))
                                    .desired_rows(prompt_rows)
                                    .desired_width(card_w - 20.0),
                            );
                            {
                                // Only register a scroll zone when content overflows.
                                // Wheel over short text goes to zoom.
                                let pr = prompt_resp.rect;
                                if text_overflows(ui, &node.prompt, card_w - 28.0, prompt_rows) {
                                    scroll_rects
                                        .push((node_id, [pr.min.x, pr.min.y, pr.max.x, pr.max.y]));
                                }
                            }
                            if prompt_resp.gained_focus() {
                                need_undo = true;
                            }
                            if prompt_resp.changed() {
                                self.touched.insert(node_id);
                                self.dirty = true;
                            }
                            // Hover explains why running is unavailable (shared by top/bottom run buttons).
                            let run_hint = if !node.enabled {
                                t(lang, "OFF 상태입니다. 켜야 실행됩니다")
                            } else if busy {
                                t(lang, "실행 중입니다")
                            } else {
                                t(lang, "이 노드만 실행합니다")
                            };
                            // Run button right under the prompt: no need to scroll down.
                            ui.horizontal(|ui| {
                                let quick = ui.add_enabled(
                                    node.enabled && !busy,
                                    egui::Button::new(
                                        egui::RichText::new(t(lang, "▶ 실행"))
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .fill(META_BLUE),
                                );
                                if quick.on_hover_text(run_hint).clicked() {
                                    actions.push((node_id, NodeAction::Run));
                                }
                                if busy {
                                    ui.spinner();
                                    ui.label(subtle(t(lang, "작업 중…")));
                                }
                            });

                            // Input links: only linked ones show; additions are picked from the list.
                            // The card won't flood with checkboxes even with many nodes.
                            ui.horizontal(|ui| {
                                ui.label(t(lang, "입력 ←"));
                                ui.label(subtle(
                                    t(lang, "연결 {n}개")
                                        .replace("{n}", &node.inputs.len().to_string()),
                                ));
                                let add_open = self.link_add_open.contains(&node_id);
                                if ui
                                    .small_button(if add_open {
                                        t(lang, "＋ 추가 닫기")
                                    } else {
                                        t(lang, "＋ 추가")
                                    })
                                    .on_hover_text(t(lang, "연결할 노드를 골라 잇습니다"))
                                    .clicked()
                                {
                                    if add_open {
                                        self.link_add_open.remove(&node_id);
                                    } else {
                                        self.link_add_open.insert(node_id);
                                    }
                                }
                                if !node.inputs.is_empty()
                                    && ui
                                        .small_button(t(lang, "전체 끊기"))
                                        .on_hover_text(t(lang, "입력 연결을 모두 끊습니다"))
                                        .clicked()
                                {
                                    actions.push((node_id, NodeAction::UnlinkAll));
                                }
                            });
                            // Per linked source: × unlink + title + output preview.
                            // Long text is trimmed to one line: never stretches the card sideways.
                            for src_id in node.inputs.clone() {
                                let src_title = peers
                                    .iter()
                                    .find_map(|(pid, title)| {
                                        (*pid == src_id).then(|| title.clone())
                                    })
                                    .unwrap_or_else(|| format!("#{src_id}"));
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button(t(lang, "× 끊기"))
                                        .on_hover_text(
                                            t(lang, "'{title}' 연결을 끊습니다.")
                                                .replace("{title}", &src_title),
                                        )
                                        .clicked()
                                    {
                                        actions.push((node_id, NodeAction::Unlink(src_id)));
                                    }
                                    ui.add(
                                        egui::Label::new(subtle(format!(
                                            "← {}",
                                            short_title(&src_title)
                                        )))
                                        .truncate(),
                                    );
                                    if let Some(prev) = peer_preview.get(&src_id) {
                                        let first = prev.lines().next().unwrap_or("").trim();
                                        if !first.is_empty() {
                                            ui.add(
                                                egui::Label::new(subtle(format!(
                                                    "→ {}",
                                                    short_title(first)
                                                )))
                                                .truncate(),
                                            );
                                        }
                                    }
                                });
                            }
                            // ＋ add list: only not-yet-linked nodes appear as buttons.
                            // Only register a scroll zone when it overflows.
                            if self.link_add_open.contains(&node_id) {
                                let add_list = egui::ScrollArea::vertical()
                                    .max_height(120.0 * z_ui)
                                    .id_salt(("link-add", node_id))
                                    .show(ui, |ui| {
                                        let mut added: Option<usize> = None;
                                        for (pid, title) in &peers {
                                            if *pid == node_id || node.inputs.contains(pid) {
                                                continue;
                                            }
                                            if ui
                                                .small_button(format!("＋ {}", short_title(title)))
                                                .on_hover_text(title)
                                                .clicked()
                                            {
                                                added = Some(*pid);
                                            }
                                        }
                                        if peers.iter().all(|(pid, _)| {
                                            *pid == node_id || node.inputs.contains(pid)
                                        }) {
                                            ui.label(subtle(t(lang, "더 이을 노드가 없습니다")));
                                        }
                                        if let Some(pid) = added {
                                            let mut want = node.inputs.clone();
                                            want.push(pid);
                                            actions.push((node_id, NodeAction::SetInputs(want)));
                                        }
                                    });
                                if add_list.content_size.y > add_list.inner_rect.height() + 1.0 {
                                    let lr = add_list.inner_rect;
                                    scroll_rects
                                        .push((node_id, [lr.min.x, lr.min.y, lr.max.x, lr.max.y]));
                                }
                            }
                            // Representative image pick/clear
                            ui.horizontal(|ui| {
                                ui.label(t(lang, "이미지:"));
                                if ui.button(t(lang, "이미지 선택")).clicked() {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter(
                                            "이미지",
                                            &["png", "jpg", "jpeg", "webp", "bmp", "gif"],
                                        )
                                        .pick_file()
                                    {
                                        late_bits.push(LateBit::SetImage(
                                            node_id,
                                            path.display().to_string(),
                                        ));
                                        status_msg = Some(
                                            t(lang, "대표 이미지를 설정했습니다.").to_string(),
                                        );
                                    }
                                }
                                if node.image_path.is_some()
                                    && ui.button(t(lang, "이미지 지우기")).clicked()
                                {
                                    late_bits.push(LateBit::ClearImage(node_id));
                                }
                            });

                            // Detail options (fold/unfold)
                            egui::CollapsingHeader::new(t(lang, "세부 옵션"))
                                .id_salt(("node-opts", node_id))
                                .show(ui, |ui| {
                                    // Look: accent color + corners
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "모양"));
                                        egui::ComboBox::from_id_salt(("node-accent", node_id))
                                            .selected_text(node.accent.label(lang))
                                            .show_ui(ui, |ui| {
                                                for accent in [
                                                    NodeAccent::Blue,
                                                    NodeAccent::Green,
                                                    NodeAccent::Orange,
                                                    NodeAccent::Gray,
                                                ] {
                                                    if ui
                                                        .selectable_value(
                                                            &mut node.accent,
                                                            accent,
                                                            accent.label(lang),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.dirty = true;
                                                    }
                                                }
                                                // Save the current color as the custom color
                                                let (cr, cg, cb) = node.accent.rgb();
                                                if ui
                                                    .selectable_value(
                                                        &mut node.accent,
                                                        NodeAccent::Custom([cr, cg, cb]),
                                                        t(lang, "직접"),
                                                    )
                                                    .clicked()
                                                {
                                                    self.dirty = true;
                                                }
                                            });
                                        // Palette button: picking a color applies it to the node immediately
                                        let (pr, pg, pb) = node.accent.rgb();
                                        let mut picked = [pr, pg, pb];
                                        if egui::color_picker::color_edit_button_srgb(
                                            ui,
                                            &mut picked,
                                        )
                                        .changed()
                                        {
                                            node.accent = NodeAccent::Custom(picked);
                                            self.dirty = true;
                                        }
                                        egui::ComboBox::from_id_salt(("node-radius", node_id))
                                            .selected_text(node.radius.label(lang))
                                            .show_ui(ui, |ui| {
                                                for radius in [
                                                    NodeRadius::Round,
                                                    NodeRadius::Soft,
                                                    NodeRadius::Sharp,
                                                ] {
                                                    ui.selectable_value(
                                                        &mut node.radius,
                                                        radius,
                                                        radius.label(lang),
                                                    );
                                                }
                                            });
                                    });
                                    // Temperature: single-slider control. The value shows right on the slider.
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "온도:"));
                                        let mut tmp = node.temperature.unwrap_or(1.0);
                                        let temp_slider =
                                            ui.add(egui::Slider::new(&mut tmp, 0.0..=2.0));
                                        if temp_slider.drag_started() {
                                            need_undo = true;
                                        }
                                        if temp_slider.changed() {
                                            node.temperature = Some(tmp.clamp(0.0, 2.0));
                                            self.dirty = true;
                                        }
                                        if ui.button(t(lang, "기본값")).clicked() {
                                            late_bits.push(LateBit::ResetTemp(node_id));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "최대 토큰:"));
                                        let mut mt = node.max_tokens.unwrap_or(0);
                                        let tokens_drag = ui.add(
                                            egui::DragValue::new(&mut mt).range(0.0..=32768.0),
                                        );
                                        if tokens_drag.drag_started() {
                                            need_undo = true;
                                        }
                                        if tokens_drag.changed() {
                                            node.max_tokens = if mt == 0 { None } else { Some(mt) };
                                            self.dirty = true;
                                        }
                                        if ui.button(t(lang, "기본값")).clicked() {
                                            late_bits.push(LateBit::ResetTokens(node_id));
                                        }
                                    });
                                    ui.label(subtle(t(lang, "0이면 기본값 사용")));
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "모델:"));
                                        let mut m = node.model.clone().unwrap_or_default();
                                        let model_resp = ui.add(
                                            egui::TextEdit::singleline(&mut m)
                                                .id(Id::new(("card-model", node_id)))
                                                .desired_width((card_w - 190.0).max(60.0))
                                                .hint_text(self.config.model.clone()),
                                        );
                                        if model_resp.gained_focus() {
                                            need_undo = true;
                                        }
                                        if model_resp.changed() {
                                            let trimmed = m.trim().to_string();
                                            node.model = if trimmed.is_empty() {
                                                None
                                            } else {
                                                Some(trimmed)
                                            };
                                            self.touched.insert(node_id);
                                            self.dirty = true;
                                        }
                                        // Recently used model history: pick one to apply it directly.
                                        let history = self.model_history.clone();
                                        if !history.is_empty() {
                                            egui::ComboBox::from_id_salt((
                                                "node-model-hist",
                                                node_id,
                                            ))
                                            .selected_text(t(lang, "기록"))
                                            .show_ui(
                                                ui,
                                                |ui| {
                                                    for h in &history {
                                                        if ui.selectable_label(false, h).clicked() {
                                                            node.model = Some(h.clone());
                                                            self.touched.insert(node_id);
                                                            self.dirty = true;
                                                        }
                                                    }
                                                },
                                            );
                                        }
                                        if ui.button(t(lang, "기본값")).clicked() {
                                            late_bits.push(LateBit::ResetModel(node_id));
                                        }
                                    });
                                    ui.label(t(lang, "어시스턴트 말투·역할 지시 (선택):"));
                                    let system_resp = ui.add(
                                        egui::TextEdit::multiline(&mut node.system)
                                            .id(Id::new(("card-system", node_id)))
                                            .desired_rows(2)
                                            .desired_width(card_w - 20.0)
                                            .hint_text(t(lang, "예: 너는 친절한 한국어 튜터야.")),
                                    );
                                    if system_resp.gained_focus() {
                                        need_undo = true;
                                    }
                                    if system_resp.changed() {
                                        self.touched.insert(node_id);
                                        self.dirty = true;
                                    }
                                });

                            // Context blocks (MCP tool results etc.)
                            let mut remove_ctx: Vec<usize> = Vec::new();
                            for (ci, block) in node.context.iter().enumerate() {
                                let key = (node_id, ci);
                                let is_open = self.open_ctx.contains(&key);
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(subtle(format!(
                                            "[{}]",
                                            short_title(&block.source)
                                        )))
                                        .truncate(),
                                    );
                                    ui.label(subtle(
                                        t(lang, "{}자")
                                            .replace("{}", &block.text.chars().count().to_string()),
                                    ));
                                    if ui
                                        .small_button(if is_open {
                                            t(lang, "닫기")
                                        } else {
                                            t(lang, "보기")
                                        })
                                        .clicked()
                                    {
                                        if is_open {
                                            self.open_ctx.remove(&key);
                                        } else {
                                            self.open_ctx.insert(key);
                                        }
                                    }
                                    if ui
                                        .small_button("×")
                                        .on_hover_text(t(lang, "이 컨텍스트를 지웁니다"))
                                        .clicked()
                                    {
                                        remove_ctx.push(ci);
                                    }
                                });
                                if self.open_ctx.contains(&key) {
                                    ui.add(
                                        egui::Label::new(subtle(share::truncate_chars(
                                            &block.text,
                                            400,
                                        )))
                                        .wrap()
                                        .selectable(true),
                                    );
                                }
                            }
                            // Removals must apply after the snapshot so undo works.
                            for ci in remove_ctx.into_iter().rev() {
                                late_bits.push(LateBit::RemoveCtx(node_id, ci));
                            }

                            ui.horizontal(|ui| {
                                // Show re-run when there is already output
                                let run_text = if node.output.is_empty() {
                                    t(lang, "▶ 실행")
                                } else {
                                    t(lang, "▶ 재실행")
                                };
                                let run = ui.add_enabled(
                                    node.enabled && !busy,
                                    egui::Button::new(
                                        egui::RichText::new(run_text)
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .fill(META_BLUE),
                                );
                                if run.on_hover_text(run_hint).clicked() {
                                    actions.push((node_id, NodeAction::Run));
                                }
                                if busy
                                    && ui
                                        .button(t(lang, "취소"))
                                        .on_hover_text(t(lang, "진행 중인 작업을 멈춥니다"))
                                        .clicked()
                                {
                                    actions.push((node_id, NodeAction::Cancel));
                                }
                                // Disable when MCP is disconnected and explain why.
                                let ctx_btn = ui.add_enabled(
                                    mcp_online,
                                    egui::Button::new(t(lang, "＋ 컨텍스트")),
                                );
                                if ctx_btn
                                    .on_hover_text(if mcp_online {
                                        t(lang, "MCP 도구 결과를 붙입니다")
                                    } else {
                                        t(lang, "설정에서 MCP 서버를 먼저 연결하세요")
                                    })
                                    .clicked()
                                {
                                    actions.push((node_id, NodeAction::OpenTools));
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui
                                    .button(t(lang, "복제"))
                                    .on_hover_text(t(lang, "같은 설정으로 복사합니다"))
                                    .clicked()
                                {
                                    actions.push((node_id, NodeAction::Clone));
                                }
                                // Delete uses two-step confirmation (press again within 4s)
                                let armed = matches!(self.confirm_delete,
                                Some((id, at)) if id == node_id
                                    && at.elapsed() <= Duration::from_secs(4));
                                let del_text = if armed {
                                    t(lang, "삭제 확인")
                                } else {
                                    t(lang, "삭제")
                                };
                                let mut del_btn = egui::Button::new(del_text);
                                if armed {
                                    del_btn = del_btn.fill(egui::Color32::from_rgb(220, 38, 38));
                                }
                                if ui
                                    .add(del_btn)
                                    .on_hover_text(t(lang, "두 번 누르면 삭제됩니다"))
                                    .clicked()
                                {
                                    if armed {
                                        actions.push((node_id, NodeAction::Delete));
                                        self.confirm_delete = None;
                                    } else {
                                        self.confirm_delete = Some((node_id, Instant::now()));
                                        status_msg = Some(
                                            t(lang, "한 번 더 누르면 삭제됩니다.").to_string(),
                                        );
                                    }
                                }
                                let armed_here = self.linking == Some(node_id);
                                let link_label = if armed_here {
                                    t(lang, "연결 취소")
                                } else {
                                    t(lang, "→ 연결")
                                };
                                if ui
                                    .button(link_label)
                                    .on_hover_text(t(lang, "출력 포트에서 드래그해도 연결됩니다"))
                                    .clicked()
                                {
                                    actions.push((
                                        node_id,
                                        if armed_here {
                                            NodeAction::DisarmLink
                                        } else {
                                            NodeAction::ArmLink
                                        },
                                    ));
                                }
                            });

                            // Per-node scheduling: wait time, loops, and stop conditions set on this card.
                            egui::CollapsingHeader::new(t(lang, "예약 실행"))
                                .default_open(false)
                                .id_salt(("node-sched", node_id))
                                .show(ui, |ui| {
                                    if ui
                                        .checkbox(
                                            &mut node.sched.enabled,
                                            t(lang, "이 노드 예약 켜기"),
                                        )
                                        .changed()
                                    {
                                        node.sched.next_due = None;
                                        node.sched.runs_done = 0;
                                        need_undo = true;
                                        self.dirty = true;
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "대기:"));
                                        let mut hours = node.sched.wait_hours;
                                        if ui
                                            .add(egui::DragValue::new(&mut hours).range(0.0..=72.0))
                                            .changed()
                                        {
                                            node.sched.wait_hours = hours.min(72);
                                            node.sched.next_due = None;
                                            need_undo = true;
                                            self.dirty = true;
                                        }
                                        ui.label(t(lang, "시간"));
                                        let mut mins = node.sched.wait_mins;
                                        if ui
                                            .add(egui::DragValue::new(&mut mins).range(0.0..=59.0))
                                            .changed()
                                        {
                                            node.sched.wait_mins = mins.min(59);
                                            node.sched.next_due = None;
                                            need_undo = true;
                                            self.dirty = true;
                                        }
                                        ui.label(t(lang, "분"));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "루프 횟수 (1=한 번만):"));
                                        let mut loops = node.sched.loop_count.max(1);
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut loops).range(1.0..=999.0),
                                            )
                                            .changed()
                                        {
                                            node.sched.loop_count = loops.clamp(1, 999);
                                            need_undo = true;
                                            self.dirty = true;
                                        }
                                    });
                                    if ui
                                        .checkbox(
                                            &mut node.sched.stop_on_success,
                                            t(lang, "성공하면 중단"),
                                        )
                                        .on_hover_text(t(
                                            lang,
                                            "에러가 아닌 출력이 나오면 루프를 멈춥니다",
                                        ))
                                        .changed()
                                    {
                                        need_undo = true;
                                        self.dirty = true;
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(t(lang, "출력에 포함되면 중단:"));
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(
                                                    &mut node.sched.stop_when_contains,
                                                )
                                                .desired_width(120.0),
                                            )
                                            .changed()
                                        {
                                            need_undo = true;
                                            self.dirty = true;
                                        }
                                    });
                                    // Re-arm the time on render so the next run shows immediately.
                                    node.sched.arm(chrono::Local::now());
                                    if node.sched.enabled {
                                        match node.sched.next_due {
                                            Some(due) => {
                                                ui.label(
                                                    t(lang, "다음 실행: {at}")
                                                        .replace("{at}", &fmt_when(due)),
                                                );
                                                if node.sched.runs_done > 0 {
                                                    ui.label(subtle(
                                                        t(lang, "{n}회 실행").replace(
                                                            "{n}",
                                                            &node.sched.runs_done.to_string(),
                                                        ),
                                                    ));
                                                }
                                            }
                                            None => {
                                                ui.label(subtle(t(lang, "예약이 꺼져 있습니다.")));
                                            }
                                        }
                                    }
                                });

                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(t(lang, "개별 출력:"));
                                // Status badge: running / waiting / error / done
                                if busy {
                                    ui.spinner();
                                    ui.label(subtle(t(lang, "실행 중…")));
                                } else if node.output.is_empty() {
                                    ui.label(subtle(t(lang, "대기")));
                                } else if is_error_output(&node.output) {
                                    ui.label(
                                        egui::RichText::new(t(lang, "오류"))
                                            .color(egui::Color32::from_rgb(220, 38, 38))
                                            .strong(),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(t(lang, "완료"))
                                            .color(egui::Color32::from_rgb(14, 148, 96))
                                            .strong(),
                                    );
                                }
                                if !node.output.is_empty() {
                                    let mut stat = t(lang, "{}자")
                                        .replace("{}", &node.output.chars().count().to_string());
                                    if let Some(s) = node.last_secs {
                                        stat.push_str(&format!(" · {:.1}{}", s, t(lang, "초")));
                                    }
                                    if let Some(done) = node.last_done {
                                        stat.push_str(&format!(
                                            " · {}",
                                            Self::ago_text(lang, done)
                                        ));
                                    }
                                    ui.label(subtle(stat));
                                }
                                if node.prev_output.as_ref().is_some_and(|p| !p.is_empty()) {
                                    let open = self.prev_open.contains(&node_id);
                                    if ui
                                        .small_button(if open {
                                            t(lang, "이전 닫기")
                                        } else {
                                            t(lang, "이전 출력")
                                        })
                                        .on_hover_text(t(lang, "덮어쓰기 전 출력을 봅니다"))
                                        .clicked()
                                    {
                                        if open {
                                            self.prev_open.remove(&node_id);
                                        } else {
                                            self.prev_open.insert(node_id);
                                        }
                                    }
                                }
                                // Disable the copy button for empty output.
                                let copy_btn = ui.add_enabled(
                                    !node.output.is_empty(),
                                    egui::Button::new(t(lang, "출력 복사")),
                                );
                                if copy_btn.clicked() {
                                    ctx.copy_text(node.output.clone());
                                    status_msg = Some(
                                        t(lang, "출력을 클립보드에 복사했습니다.").to_string(),
                                    );
                                }
                                // Long output opens in a standalone OS window (same as the live results window).
                                let zoom_btn = ui.add_enabled(
                                    !node.output.is_empty(),
                                    egui::Button::new(t(lang, "크게 보기")),
                                );
                                if zoom_btn
                                    .on_hover_text(t(lang, "출력을 큰 창으로 봅니다"))
                                    .clicked()
                                {
                                    actions.push((node_id, NodeAction::OutputWindow));
                                }
                                // Generated audio plays in the default player.
                                if let Some(apath) = node.audio_path.clone() {
                                    if ui.button(t(lang, "▶ 재생")).clicked() {
                                        if open::that(&apath).is_ok() {
                                            status_msg =
                                                Some(t(lang, "오디오를 재생합니다.").to_string());
                                        } else {
                                            status_msg = Some(
                                                t(lang, "오디오를 열 수 없습니다: {path}")
                                                    .replace("{path}", &apath),
                                            );
                                        }
                                    }
                                }
                            });
                            // Output: only register a scroll zone when it overflows.
                            let mut out_overflows = false;
                            let out_frame = output_frame(ctx).show(ui, |ui| {
                                let out = egui::ScrollArea::vertical()
                                    .max_height(110.0 * h_scale * z_ui)
                                    .show(ui, |ui| {
                                        if node.output.is_empty() {
                                            ui.label(subtle(t(
                                                lang,
                                                "실행을 누르면 여기 출력이 표시됩니다.",
                                            )));
                                        } else if is_error_output(&node.output) {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&node.output).color(
                                                        egui::Color32::from_rgb(220, 38, 38),
                                                    ),
                                                )
                                                .wrap()
                                                .selectable(true),
                                            );
                                        } else {
                                            ui.add(
                                                egui::Label::new(&node.output)
                                                    .wrap()
                                                    .selectable(true),
                                            );
                                        }
                                    });
                                out_overflows = out.content_size.y > out.inner_rect.height() + 1.0;
                            });
                            if out_overflows {
                                let or_ = out_frame.response.rect;
                                scroll_rects
                                    .push((node_id, [or_.min.x, or_.min.y, or_.max.x, or_.max.y]));
                            }
                            // Previous-generation output view
                            if self.prev_open.contains(&node_id) {
                                if let Some(prev) = node.prev_output.clone() {
                                    ui.label(subtle(t(lang, "이전 출력:")));
                                    let mut prev_overflows = false;
                                    let prev_frame = output_frame(ctx).show(ui, |ui| {
                                        let out = egui::ScrollArea::vertical()
                                            .max_height(80.0 * z_ui)
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::Label::new(prev).wrap().selectable(true),
                                                );
                                            });
                                        prev_overflows =
                                            out.content_size.y > out.inner_rect.height() + 1.0;
                                    });
                                    if prev_overflows {
                                        let pr_ = prev_frame.response.rect;
                                        scroll_rects.push((
                                            node_id,
                                            [pr_.min.x, pr_.min.y, pr_.max.x, pr_.max.y],
                                        ));
                                    }
                                }
                            }
                            // Resize handle (bottom): horizontal=width, vertical=height, double-click reset.
                            // Without click_and_drag, double-clicks never register.
                            ui.horizontal(|ui| {
                                ui.label(subtle(
                                    t(lang, "너비 {w}% · 높이 {h}%")
                                        .replace("{w}", &format!("{:.0}", node.width_scale * 100.0))
                                        .replace(
                                            "{h}",
                                            &format!("{:.0}", node.height_scale * 100.0),
                                        ),
                                ));
                                // Confine the handle inside the card so it never outgrows the card at low zoom.
                                let grip_w = (card_w - 120.0).clamp(0.0, card_w.max(0.0));
                                let (grip_rect, _spacer) = ui.allocate_exact_size(
                                    egui::vec2(grip_w, (16.0 * z_ui).max(10.0)),
                                    Sense::hover(),
                                );
                                // Resize only from the bottom-right corner.
                                let hw = (30.0 * z_ui).min(grip_rect.width());
                                let corner_rect = egui::Rect::from_min_max(
                                    egui::pos2(grip_rect.right() - hw, grip_rect.top()),
                                    grip_rect.right_bottom(),
                                );
                                let corner = ui.interact(
                                    corner_rect,
                                    Id::new(("corner-resize", node_id)),
                                    Sense::click_and_drag(),
                                );
                                if corner.hovered() || corner.dragged() {
                                    ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                                }
                                // Three corner slashes (Windows-style resize affordance)
                                for (i, off) in [4.0_f32, 9.0, 14.0].iter().enumerate() {
                                    let o = off * z_ui;
                                    let a = egui::Pos2::new(
                                        corner_rect.right() - o,
                                        corner_rect.bottom() - 3.0 * z_ui,
                                    );
                                    let b = egui::Pos2::new(
                                        corner_rect.right() - 3.0 * z_ui,
                                        corner_rect.bottom() - o,
                                    );
                                    let alpha = if i == 2 { 140 } else { 200 };
                                    ui.painter().line_segment(
                                        [a, b],
                                        egui::Stroke::new(
                                            2.0 * z_ui,
                                            egui::Color32::from_rgba_unmultiplied(
                                                accent.r(),
                                                accent.g(),
                                                accent.b(),
                                                alpha,
                                            ),
                                        ),
                                    );
                                }
                                if corner.drag_started() {
                                    need_undo = true;
                                    // On drop, only push neighbors newly overlapped by the growth.
                                    let self_rect = canvas_rects
                                        .iter()
                                        .find(|(i, _)| *i == node_id)
                                        .map(|(_, r)| *r);
                                    let overlapped: Vec<usize> = match self_rect {
                                        Some(sr) => canvas_rects
                                            .iter()
                                            .filter(|(id, _)| *id != node_id)
                                            .filter(|(_, r)| rects_overlap(sr, *r, 0.0))
                                            .map(|(id, _)| *id)
                                            .collect(),
                                        None => Vec::new(),
                                    };
                                    self.drop_baseline = Some((vec![node_id], overlapped));
                                }
                                if corner.drag_stopped() {
                                    released.push(node_id);
                                }
                                if corner.dragged() {
                                    let d = corner.drag_delta();
                                    // Divide by the zoom factor so resizing feels the same at any zoom.
                                    let z = zoom.max(0.01);
                                    // Width stops at the right-panel boundary (based on the previous screen rect).
                                    let dx = match (node.last_rect, self.central_rect) {
                                        (Some(r), Some(c)) if d.x > 0.0 => {
                                            d.x.min((c[2] - r[2]).max(0.0))
                                        }
                                        _ => d.x,
                                    };
                                    node.width_scale =
                                        (node.width_scale + dx / (300.0 * z)).max(0.05);
                                    node.height_scale =
                                        (node.height_scale + d.y / (200.0 * z)).max(0.3);
                                    self.dirty = true;
                                }
                                if corner.double_clicked() {
                                    node.width_scale = 1.0;
                                    node.height_scale = 1.0;
                                    self.dirty = true;
                                }
                                corner.on_hover_text(t(
                                    lang,
                                    "모서리 드래그: 크기 조절 · 더블클릭: 원래대로",
                                ));
                            });
                        } // if !title_only
                    });
                });
            let r = area_resp.response.rect;
            node.last_rect = Some([r.min.x, r.min.y, r.max.x, r.max.y]);
        }
        // Replace the scroll zones wholesale with this frame's collection.
        // So stale rects from vanished nodes or closed areas don't linger.
        self.scroll_zones.clear();
        for (id, r) in scroll_rects {
            self.scroll_zones.entry(id).or_default().push(r);
        }
        // Apply multi-select moves (the dragged card already moved inside the loop).
        if !group_moves.is_empty() {
            for (oid, step) in group_moves {
                if let Some(other) = self.nodes.iter_mut().find(|n| n.id == oid) {
                    let pre = other.pos;
                    other.pos[0] += step[0];
                    other.pos[1] += step[1];
                    let z = self.zoom.max(0.001);
                    let size = other
                        .last_rect
                        .map(|r| [(r[2] - r[0]) / z, (r[3] - r[1]) / z])
                        .unwrap_or([320.0, 420.0]);
                    other.pos = clamp_board_step(
                        pre,
                        other.pos,
                        size,
                        [self.config.board_w, self.config.board_h],
                    );
                    other.pos = clamp_panel_x_step(
                        pre,
                        other.pos,
                        size[0] * z,
                        self.central_rect,
                        self.canvas,
                        self.zoom,
                    );
                }
            }
            self.dirty = true;
        }
        // Of neighbors overlapping the dropped card, push only ones not overlapping at gesture start.
        // Leave existing piles alone. Must run before the snapshot so undo
        // restores the pre-drag state in one step.
        if !released.is_empty() {
            let fixed: Vec<usize> = if self.selected.len() > 1
                && released.iter().all(|id| self.selected.contains(id))
            {
                self.selected.iter().copied().collect()
            } else {
                released.clone()
            };
            // Only declump with a start record (without one, cards would fly away).
            if let Some((_, pre_overlapped)) = self.drop_baseline.take() {
                let moves = declump(&canvas_rects, &fixed, &pre_overlapped, 0.0);
                if !moves.is_empty() {
                    let z = self.zoom.max(0.001);
                    for (id, pos) in moves {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            let pre = n.pos;
                            n.pos = pos;
                            let size = n
                                .last_rect
                                .map(|r| [(r[2] - r[0]) / z, (r[3] - r[1]) / z])
                                .unwrap_or([320.0, 420.0]);
                            n.pos = clamp_board_step(
                                pre,
                                n.pos,
                                size,
                                [self.config.board_w, self.config.board_h],
                            );
                            n.pos = clamp_panel_x_step(
                                pre,
                                n.pos,
                                size[0] * z,
                                self.central_rect,
                                self.canvas,
                                self.zoom,
                            );
                        }
                    }
                    need_undo = true;
                    self.dirty = true;
                }
            }
        }
        // Per-item commit actions: applied after the snapshot.
        if need_undo || !late_bits.is_empty() {
            self.push_undo();
        }
        if !late_bits.is_empty() {
            for bit in late_bits {
                match bit {
                    LateBit::ToggleEnabled(id) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            n.enabled = !n.enabled;
                        }
                    }
                    LateBit::ResetTemp(id) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            n.temperature = None;
                        }
                    }
                    LateBit::ResetTokens(id) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            n.max_tokens = None;
                        }
                    }
                    LateBit::ResetModel(id) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            n.model = None;
                        }
                    }
                    LateBit::SetImage(id, path) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            n.image_path = Some(path);
                        }
                        self.node_textures.remove(&id);
                        self.touched.insert(id);
                    }
                    LateBit::ClearImage(id) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            n.image_path = None;
                        }
                        self.node_textures.remove(&id);
                    }
                    LateBit::RemoveCtx(id, ci) => {
                        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
                            if ci < n.context.len() {
                                n.context.remove(ci);
                                self.touched.insert(id);
                            }
                        }
                    }
                }
            }
            self.dirty = true;
        }

        for (id, action) in actions {
            match action {
                NodeAction::Run => self.spawn_generate(id),
                NodeAction::Cancel => {
                    let had_job = self.pending.remove(&id).is_some();
                    self.batch_drop(id);
                    if had_job {
                        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                            Self::stash_output(node);
                            node.output = t(lang, "[취소됨] {e}")
                                .replace("{e}", &JobError::Cancelled.display(lang));
                            self.mark_dirty();
                        }
                    }
                    self.status = t(lang, "작업을 취소했습니다.").to_string();
                }
                NodeAction::Clone => self.clone_node(id),
                NodeAction::Delete => self.delete_node(id),
                NodeAction::OutputWindow => self.open_output_window(id),
                NodeAction::OpenTools => {
                    self.tool_menu_for = Some(id);
                    self.tool_args = "{}".to_string();
                }
                NodeAction::Front => {
                    if let Some(i) = self.nodes.iter().position(|n| n.id == id) {
                        let node = self.nodes.remove(i);
                        self.nodes.push(node);
                        self.mark_dirty();
                    }
                }
                NodeAction::ArmLink => {
                    self.linking = Some(id);
                    self.status = t(
                        lang,
                        "연결할 노드의 타이틀바를 클릭하세요 (다시 누르면 취소).",
                    )
                    .to_string();
                }
                NodeAction::DisarmLink => {
                    self.linking = None;
                    self.status = t(lang, "연결 모드를 취소했습니다.").to_string();
                }
                NodeAction::LinkFrom(src) => {
                    let ok = src != id
                        && self.nodes.iter().any(|n| n.id == src)
                        && self.nodes.iter().any(|n| n.id == id);
                    let dup = self
                        .nodes
                        .iter()
                        .any(|n| n.id == id && n.inputs.contains(&src));
                    if ok && blocked_cycle(&self.nodes, id, src) {
                        self.linking = None;
                        self.status = t(lang, "순환 연결은 만들 수 없습니다.").to_string();
                    } else if ok && dup {
                        self.linking = None;
                        self.status = t(lang, "이미 연결되어 있습니다.").to_string();
                    } else if ok {
                        let mutual = is_mutual_only(&self.nodes, id, src);
                        self.push_undo();
                        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                            node.add_input(src);
                        }
                        self.linking = None;
                        self.status = if mutual {
                            t(lang, "양방향으로 연결했습니다.").to_string()
                        } else {
                            t(lang, "노드를 연결했습니다.").to_string()
                        };
                        self.mark_dirty();
                    } else {
                        self.linking = None;
                    }
                }
                NodeAction::Unlink(src) => {
                    self.push_undo();
                    if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                        node.remove_input(src);
                    }
                    self.mark_dirty();
                    self.status = t(lang, "연결을 끊었습니다.").to_string();
                }
                NodeAction::UnlinkAll => {
                    let had = self
                        .nodes
                        .iter()
                        .any(|n| n.id == id && !n.inputs.is_empty());
                    if had {
                        self.push_undo();
                        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                            node.clear_inputs();
                        }
                        self.mark_dirty();
                    }
                    self.status = t(lang, "입력 연결을 모두 끊었습니다.").to_string();
                }
                NodeAction::SetInputs(want) => {
                    // Cycle-check only newly added inputs; leave the rest as-is.
                    let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
                        continue;
                    };
                    let current = node.inputs.clone();
                    let mut kept: Vec<usize> = Vec::new();
                    let mut blocked = false;
                    for s in &want {
                        if current.contains(s) || *s == id {
                            if current.contains(s) {
                                kept.push(*s);
                            }
                            continue;
                        }
                        if self.nodes.iter().any(|n| n.id == *s)
                            && !blocked_cycle(&self.nodes, id, *s)
                        {
                            kept.push(*s);
                        } else {
                            blocked = true;
                        }
                    }
                    if kept != current {
                        self.push_undo();
                        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                            node.inputs = kept;
                        }
                        self.mark_dirty();
                    }
                    self.status = if blocked {
                        t(lang, "순환 연결은 만들 수 없습니다.").to_string()
                    } else {
                        t(lang, "노드를 연결했습니다.").to_string()
                    };
                }
            }
        }
    }
}
