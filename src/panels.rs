//! Settings, about, and tool windows.

use std::time::{Duration, Instant};

use eframe::egui;

use crate::cards::load_node_image;
use crate::i18n::{t, Lang};
use crate::mcp::McpServerConfig;
use crate::node::combined_results;
use crate::state::App;
use crate::store::Provider;
use crate::theme::{
    brand_title, hud_rule, output_frame, output_text, palette, section_title, subtle,
    text_scroll_source, theme_button, Theme, META_BLUE,
};

impl App {
    /// Records the modal window's previous-frame rect into the overlay staging.
    /// Keeps wheel/pinch zoom from reaching the canvas behind the modal.
    fn stage_modal_rect(&mut self, ctx: &egui::Context, id: &str) {
        if let Some(r) = ctx.memory(|m| m.area_rect(egui::Id::new(id))) {
            self.overlay_staging
                .push([r.min.x, r.min.y, r.max.x, r.max.y]);
        }
    }

    pub(crate) fn render_settings(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;

        if !self.show_settings {
            return;
        }
        // Open modals only: records the previous-frame rect as an overlay (to block wheel zoom).
        self.stage_modal_rect(ctx, "modal-settings");
        let mut open = true;
        egui::Window::new(t(lang, "설정"))
            .id(egui::Id::new("modal-settings"))
            .open(&mut open)
            .resizable(true)
            .default_size([500.0, 560.0])
            .min_size([380.0, 300.0])
            .max_size([700.0, 800.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings_is_mcp, false, "AI");
                    ui.selectable_value(&mut self.settings_is_mcp, true, t(lang, "MCP 연결"));
                });
                ui.horizontal(|ui| {
                    ui.label(t(lang, "언어"));
                    let prev_lang = self.config.lang;
                    egui::ComboBox::from_id_salt("app-lang")
                        .selected_text(self.config.lang.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.lang, Lang::Ko, Lang::Ko.label());
                            ui.selectable_value(&mut self.config.lang, Lang::En, Lang::En.label());
                        });
                    ui.label(t(lang, "테마"));
                    let prev_theme = self.config.theme;
                    egui::ComboBox::from_id_salt("app-theme")
                        .selected_text(self.config.theme.label(lang))
                        .show_ui(ui, |ui| {
                            for theme in Theme::ALL {
                                ui.selectable_value(
                                    &mut self.config.theme,
                                    theme,
                                    theme.label(lang),
                                );
                            }
                        });
                    // Current theme preview (accent, menu, result, canvas, soft)
                    {
                        let p = palette(self.config.theme);
                        ui.horizontal(|ui| {
                            ui.label(subtle(t(lang, "테마 미리 보기")));
                            for c in [p.accent, p.menu, p.result, p.canvas, p.soft] {
                                ui.label(egui::RichText::new("●").color(c).strong());
                            }
                        });
                    }
                    if self.config.lang != prev_lang || self.config.theme != prev_theme {
                        crate::theme::apply_meta_theme(ctx, self.config.theme);
                        self.mark_dirty();
                    }
                });
                ui.separator();
                // Scrolls so content is not clipped on small screens.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.settings_is_mcp {
                            self.settings_mcp(ui);
                        } else {
                            self.settings_ai(ui);
                        }
                    });
            });
        self.show_settings = open;
    }

    /// About window: shows version, license, and key policy.
    pub(crate) fn render_about(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;

        if !self.show_about {
            return;
        }
        self.stage_modal_rect(ctx, "modal-about");
        let mut open = true;
        let mut close_req = false;
        egui::Window::new(t(lang, "정보"))
            .id(egui::Id::new("modal-about"))
            .open(&mut open)
            .resizable(false)
            .default_size([360.0, 300.0])
            .show(ctx, |ui| {
                ui.label(brand_title("Symphony Canvas", palette(self.config.theme).brand));
                ui.label(subtle(format!(
                    "{} {}",
                    t(lang, "버전"),
                    env!("CARGO_PKG_VERSION")
                )));
                ui.separator();
                ui.label(t(lang, "노드형 멀티 프롬프트 캔버스 (Meta Muse API + MCP)"));
                ui.label(subtle(t(lang, "라이선스: MIT")));
                ui.label(subtle(t(
                    lang,
                    "API 키는 '이 PC에 키 저장'을 켤 때만 디스크에 저장됩니다.",
                )));
                ui.separator();
                ui.label(subtle(t(
                    lang,
                    "단축키: Ctrl+Enter 실행 · Ctrl+N 노드 · Ctrl+S 저장 · Ctrl+Z 되돌리기 · Esc 전체 취소",
                )));
                if ui.button(t(lang, "닫기")).clicked() {
                    close_req = true;
                }
            });
        if close_req {
            open = false;
        }
        self.show_about = open;
    }

    /// New-workspace confirmation: asks whether to save first or discard.
    pub(crate) fn render_new_confirm(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        if !self.new_confirm_open {
            return;
        }
        self.stage_modal_rect(ctx, "modal-new-confirm");
        let mut open = true;
        let mut choice: Option<bool> = None;
        let mut close_req = false;
        egui::Window::new(t(lang, "새로 시작할까요?"))
            .id(egui::Id::new("modal-new-confirm"))
            .open(&mut open)
            .resizable(false)
            .default_size([360.0, 180.0])
            .show(ctx, |ui| {
                ui.label(t(lang, "저장하지 않은 내용이 사라집니다."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t(lang, "저장하고 새로 만들기")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(t(lang, "저장하지 않고 새로 만들기")).clicked() {
                        choice = Some(false);
                    }
                    if ui.button(t(lang, "취소")).clicked() {
                        close_req = true;
                    }
                });
            });
        if close_req {
            open = false;
        }
        if let Some(save_first) = choice {
            if save_first {
                self.save_now();
                if self.dirty {
                    return;
                }
            }
            self.new_confirm_open = false;
            self.reset_workspace();
        }
        if !open {
            self.new_confirm_open = false;
        }
    }

    pub(crate) fn settings_ai(&mut self, ui: &mut egui::Ui) {
        let lang = self.config.lang;
        // Marks dirty only on change: avoids disk writes while the settings window sits open.
        let before = (format!("{:?}", self.config), self.session_key.clone());

        ui.label(section_title(t(lang, "언어 모델")));
        egui::ComboBox::from_label(t(lang, "제공자"))
            .selected_text(self.config.provider.label(lang))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.config.provider,
                    Provider::Mock,
                    Provider::Mock.label(lang),
                );
                ui.selectable_value(
                    &mut self.config.provider,
                    Provider::Meta,
                    Provider::Meta.label(lang),
                );
            });
        if self.config.provider == Provider::Meta {
            ui.add(
                egui::TextEdit::singleline(&mut self.config.endpoint)
                    .desired_width(420.0)
                    .hint_text("https://api.meta.ai/v1/chat/completions"),
            );
            ui.horizontal(|ui| {
                ui.label(t(lang, "모델:"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.model)
                        .desired_width(220.0)
                        .hint_text("muse-spark-1.3"),
                );
            });
            ui.add(
                egui::Slider::new(&mut self.config.timeout_secs, 10..=600).suffix(t(lang, "초")),
            );
            ui.separator();
            ui.label(section_title(t(lang, "API 키")));
            ui.add(
                egui::TextEdit::singleline(&mut self.session_key)
                    .password(true)
                    .desired_width(420.0)
                    .hint_text(t(lang, "세션 키 입력 — 메모리에만 보관")),
            );
            ui.checkbox(&mut self.config.remember_key, t(lang, "이 PC에 키 저장"));
            ui.label(subtle(t(
                lang,
                "우선순위: 세션 키 > 저장된 키 > MODEL_API_KEY 환경 변수.",
            )));
            ui.label(subtle(t(
                lang,
                "저장된 키는 설정 파일에 평문으로 보관됩니다.",
            )));
            let (env_text, _) = Self::key_status_text(lang);
            ui.horizontal(|ui| {
                ui.label(subtle(env_text));
                ui.label(subtle(if self.session_key.trim().is_empty() {
                    t(lang, "· 세션 키: 없음")
                } else {
                    t(lang, "· 세션 키: 입력됨")
                }));
                ui.label(subtle(if self.config.saved_key.trim().is_empty() {
                    t(lang, "· 저장된 키: 없음")
                } else {
                    t(lang, "· 저장된 키: 있음")
                }));
            });
            ui.horizontal(|ui| {
                if theme_button(
                    ui,
                    t(lang, "연결 테스트"),
                    palette(self.config.theme).accent,
                )
                .clicked()
                {
                    self.spawn_conn_test();
                }
                if self.test_job.is_some() {
                    ui.spinner();
                }
            });
            if !self.test_status.is_empty() {
                ui.label(&self.test_status);
            }
        } else {
            ui.label(subtle(
                t(lang, "Mock은 네트워크 없이 데모 출력을 만듭니다. 실제 호출은 Meta Muse API를 선택하세요."),
            ));
        }
        ui.separator();
        ui.horizontal(|ui| {
            if theme_button(
                ui,
                t(lang, "API 설정 저장"),
                palette(self.config.theme).accent,
            )
            .clicked()
            {
                self.sync_saved_key();
                self.save_now();
            }
            let armed = self
                .confirm_api_reset
                .is_some_and(|at| at.elapsed() <= Duration::from_secs(4));
            if theme_button(
                ui,
                if armed {
                    t(lang, "초기화 확인")
                } else {
                    t(lang, "API 설정 초기화")
                },
                palette(self.config.theme).accent,
            )
            .clicked()
            {
                if armed {
                    self.reset_api_settings();
                    self.confirm_api_reset = None;
                    self.status = t(lang, "API 설정을 기본값으로 되돌렸습니다.").to_string();
                } else {
                    self.confirm_api_reset = Some(Instant::now());
                    self.status = t(lang, "한 번 더 누르면 API 설정이 초기화됩니다.").to_string();
                }
            }
        });
        ui.separator();
        ui.label(section_title(t(lang, "결과 알람")));
        ui.label(subtle(t(
            lang,
            "전체·선택 실행이 끝나면 켜진 채널로 결과 요약을 보냅니다.",
        )));
        let a = &mut self.config.alarm;
        ui.checkbox(&mut a.telegram_on, t(lang, "텔레그램 봇"));
        ui.horizontal(|ui| {
            ui.label(t(lang, "봇 토큰"));
            ui.add(
                egui::TextEdit::singleline(&mut a.telegram_token)
                    .password(true)
                    .desired_width(280.0)
                    .hint_text("123456:ABC…"),
            );
            ui.label(t(lang, "챗 ID"));
            ui.add(
                egui::TextEdit::singleline(&mut a.telegram_chat)
                    .desired_width(120.0)
                    .hint_text("123456789"),
            );
        });
        ui.checkbox(&mut a.discord_on, t(lang, "디스코드 웹훅"));
        ui.horizontal(|ui| {
            ui.label(t(lang, "웹훅 주소"));
            ui.add(
                egui::TextEdit::singleline(&mut a.discord_webhook)
                    .desired_width(420.0)
                    .hint_text("https://discord.com/api/webhooks/…"),
            );
        });
        ui.checkbox(&mut a.wa_on, t(lang, "왓츠앱 Cloud API"));
        ui.horizontal(|ui| {
            ui.label(t(lang, "토큰"));
            ui.add(
                egui::TextEdit::singleline(&mut a.wa_token)
                    .password(true)
                    .desired_width(200.0),
            );
            ui.label(t(lang, "발신 번호 ID"));
            ui.add(egui::TextEdit::singleline(&mut a.wa_phone_id).desired_width(120.0));
            ui.label(t(lang, "수신 번호"));
            ui.add(
                egui::TextEdit::singleline(&mut a.wa_to)
                    .desired_width(120.0)
                    .hint_text("821012345678"),
            );
        });
        ui.label(subtle(t(
            lang,
            "알람 키·주소는 설정 파일에 평문으로 보관됩니다.",
        )));
        ui.horizontal(|ui| {
            if theme_button(
                ui,
                t(lang, "테스트 알람"),
                palette(self.config.theme).accent,
            )
            .clicked()
            {
                self.send_alarm_test();
            }
            if self.alarm_job.is_some() {
                ui.spinner();
            }
        });
        if !self.alarm_status.is_empty() {
            ui.label(&self.alarm_status);
        }
        ui.separator();
        ui.label(section_title(t(lang, "이미지 표시 크기")));
        ui.add(
            egui::Slider::new(&mut self.config.image_h, 80.0..=600.0)
                .suffix("px")
                .text(t(lang, "출력 이미지 높이")),
        );
        ui.label(subtle(t(
            lang,
            "결과 패널·라이브 창·출력 창의 이미지에 함께 적용됩니다.",
        )));
        self.sync_saved_key();
        if (format!("{:?}", self.config), self.session_key.clone()) != before {
            self.mark_dirty();
        }
    }

    pub(crate) fn settings_mcp(&mut self, ui: &mut egui::Ui) {
        let lang = self.config.lang;

        ui.label(section_title(t(lang, "MCP 서버 (stdio)")));
        ui.label(subtle(t(
            lang,
            "서버 명령어·인자만 저장됩니다. 도구 결과는 노드의 컨텍스트로 첨부됩니다.",
        )));

        let mut connect_req: Option<String> = None;
        let mut remove_req: Option<String> = None;
        for srv in &self.servers {
            let connected = self.mcp.is_connected(&srv.name);
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&srv.name).strong());
                ui.label(subtle(format!("{} {}", srv.command, srv.args.join(" "))));
                ui.label(subtle(if connected {
                    t(lang, "· 연결됨")
                } else {
                    t(lang, "· 끊김")
                }));
                if connected {
                    if ui.button(t(lang, "해제")).clicked() {
                        self.mcp.disconnect(&srv.name);
                    }
                } else if ui.button(t(lang, "연결")).clicked() {
                    connect_req = Some(srv.name.clone());
                }
                if ui.button(t(lang, "삭제")).clicked() {
                    remove_req = Some(srv.name.clone());
                }
            });
            // Per-server last error plus a troubleshooting hint
            if let Some(err) = self.mcp_errors.get(&srv.name) {
                ui.label(
                    egui::RichText::new(err)
                        .color(egui::Color32::from_rgb(220, 38, 38))
                        .small(),
                );
                ui.label(subtle(t(
                    lang,
                    "터미널에서 같은 명령을 직접 실행해 STDIO 응답을 확인하세요.",
                )));
            }
        }
        if let Some(name) = connect_req {
            self.mcp_connect(&name);
        }
        if let Some(name) = remove_req {
            self.mcp.disconnect(&name);
            self.servers.retain(|s| s.name != name);
            self.refresh_tools();
            self.mark_dirty();
        }

        ui.separator();
        ui.label(t(lang, "서버 추가:"));
        ui.horizontal(|ui| {
            ui.label(t(lang, "이름"));
            ui.add(egui::TextEdit::singleline(&mut self.new_srv_name).desired_width(110.0));
            ui.label(t(lang, "명령"));
            ui.add(
                egui::TextEdit::singleline(&mut self.new_srv_cmd)
                    .desired_width(150.0)
                    .hint_text("npx / python"),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t(lang, "인자"));
            ui.add(
                egui::TextEdit::singleline(&mut self.new_srv_args)
                    .desired_width(300.0)
                    .hint_text(t(
                        lang,
                        "공백으로 구분, 예: -y @modelcontextprotocol/server-x",
                    )),
            );
            if ui.button(t(lang, "추가")).clicked() {
                let name = self.new_srv_name.trim().to_string();
                let cmd = self.new_srv_cmd.trim().to_string();
                if name.is_empty() || cmd.is_empty() {
                    self.mcp_status = t(lang, "이름과 명령어를 입력하세요.").to_string();
                } else if self.servers.iter().any(|s| s.name == name) {
                    self.mcp_status = t(lang, "같은 이름의 서버가 이미 있습니다.").to_string();
                } else {
                    self.servers.push(McpServerConfig {
                        name,
                        command: cmd,
                        args: self
                            .new_srv_args
                            .split_whitespace()
                            .map(|s| s.to_string())
                            .collect(),
                    });
                    self.new_srv_name.clear();
                    self.new_srv_cmd.clear();
                    self.new_srv_args.clear();
                    self.mcp_status =
                        t(lang, "서버를 추가했습니다. 연결을 눌러보세요.").to_string();
                    self.mark_dirty();
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(t(lang, "도구 목록 새로고침")).clicked() {
                self.refresh_tools();
            }
            if !self.mcp_status.is_empty() {
                ui.label(subtle(&self.mcp_status));
            }
        });
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                if self.mcp_tools.is_empty() {
                    ui.label(subtle(t(lang, "연결된 서버의 도구가 여기에 표시됩니다.")));
                }
                for tool in &self.mcp_tools {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}/{}", tool.server, tool.name))
                                .strong()
                                .color(META_BLUE),
                        );
                        if !tool.description.is_empty() {
                            ui.label(subtle(&tool.description));
                        }
                    });
                }
            });
    }

    pub(crate) fn render_tool_window(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;

        let Some(node_id) = self.tool_menu_for else {
            return;
        };
        if !self.nodes.iter().any(|n| n.id == node_id) {
            self.tool_menu_for = None;
            return;
        }
        let title = self
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.title.clone())
            .unwrap_or_default();
        self.stage_modal_rect(ctx, "modal-tool");
        let mut open = true;
        egui::Window::new(t(lang, "도구 실행 — {title}").replace("{title}", &title))
            .id(egui::Id::new("modal-tool"))
            .open(&mut open)
            .resizable(true)
            .default_size([380.0, 300.0])
            .show(ctx, |ui| {
                let connected = self.mcp.connected_names();
                if connected.is_empty() {
                    ui.label(subtle(t(
                        lang,
                        "연결된 MCP 서버가 없습니다. 설정 → MCP 연결에서 서버를 연결하세요.",
                    )));
                    return;
                }
                if self.tool_server.is_empty() || !connected.contains(&self.tool_server) {
                    self.tool_server = connected[0].clone();
                    self.tool_name.clear();
                }
                egui::ComboBox::from_label(t(lang, "서버"))
                    .selected_text(&self.tool_server)
                    .show_ui(ui, |ui| {
                        for name in &connected {
                            ui.selectable_value(&mut self.tool_server, name.clone(), name);
                        }
                    });
                let names: Vec<String> = self
                    .mcp_tools
                    .iter()
                    .filter(|t| t.server == self.tool_server)
                    .map(|t| t.name.clone())
                    .collect();
                if names.is_empty() {
                    ui.label(subtle(t(
                        lang,
                        "이 서버의 도구 목록이 없습니다. 설정에서 새로고침하세요.",
                    )));
                } else {
                    if self.tool_name.is_empty() || !names.contains(&self.tool_name) {
                        self.tool_name = names[0].clone();
                    }
                    egui::ComboBox::from_label(t(lang, "도구"))
                        .selected_text(&self.tool_name)
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.tool_name, name.clone(), name);
                            }
                        });
                    if let Some(desc) = self
                        .mcp_tools
                        .iter()
                        .find(|t| t.server == self.tool_server && t.name == self.tool_name)
                        .map(|t| t.description.clone())
                    {
                        if !desc.is_empty() {
                            ui.label(subtle(&desc));
                        }
                    }
                }
                ui.label(t(lang, "인자 (JSON 객체):"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.tool_args)
                        .desired_width(340.0)
                        .hint_text("{}"),
                );
                ui.horizontal(|ui| {
                    if theme_button(ui, t(lang, "도구 실행"), palette(self.config.theme).accent)
                        .clicked()
                        && self.spawn_tool_call(
                            node_id,
                            self.tool_server.clone(),
                            self.tool_name.clone(),
                            self.tool_args.clone(),
                        )
                    {
                        self.tool_menu_for = None;
                    }
                    if ui.button(t(lang, "닫기")).clicked() {
                        self.tool_menu_for = None;
                    }
                });
                ui.label(subtle(t(
                    lang,
                    "결과는 노드의 컨텍스트로 첨부되어 다음 생성에 함께 전송됩니다.",
                )));
            });
        if !open {
            self.tool_menu_for = None;
        }
    }

    /// Blueprint input window: creates several nodes from one paragraph description.
    pub(crate) fn render_blueprint_window(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        if !self.blueprint_open {
            return;
        }
        self.stage_modal_rect(ctx, "modal-blueprint");
        let mut open = true;
        egui::Window::new(t(lang, "블루프린트 — 프롬프트로 노드 만들기"))
            .id(egui::Id::new("modal-blueprint"))
            .open(&mut open)
            .resizable(true)
            .default_size([440.0, 340.0])
            .show(ctx, |ui| {
                ui.label(subtle(t(
                    lang,
                    "만들 노드 구성을 설명하세요. 예: 기사 요약 → 영문 번역 → 검수 3단계",
                )));
                ui.label(subtle(t(lang, "최대 12개 노드까지 만들 수 있습니다.")));
                ui.add(
                    egui::TextEdit::multiline(&mut self.blueprint_text)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .hint_text(t(lang, "블루프린트 설명을 입력하세요.")),
                );
                ui.horizontal(|ui| {
                    let busy = self.blueprint_job.is_some();
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new(egui::RichText::new(t(lang, "노드 만들기")).strong()),
                        )
                        .clicked()
                    {
                        self.spawn_blueprint();
                    }
                    if ui.button(t(lang, "닫기")).clicked() {
                        self.blueprint_open = false;
                    }
                    if busy {
                        ui.spinner();
                    }
                });
                if !self.blueprint_status.is_empty() {
                    ui.label(&self.blueprint_status);
                }
            });
        if !open {
            self.blueprint_open = false;
        }
    }

    /// Opens the node output standalone (OS) window. Called directly by the zoom button.
    pub(crate) fn open_output_window(&mut self, node_id: usize) {
        self.output_window = Some(node_id);
    }

    /// Standalone output window contents: shows the full node text in a large scroll view.
    pub(crate) fn render_output_window(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let lang = self.config.lang;
        let Some(id) = self.output_window else {
            return;
        };
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            self.output_window = None;
            return;
        };
        let (title, output) = (node.title.clone(), node.output.clone());
        let image_path = node.image_path.clone();
        let pal = palette(self.config.theme);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("●").color(pal.accent).size(14.0));
            ui.label(section_title(&title));
        });
        hud_rule(ui, pal.accent);
        // View options: font size and monospace. Shares settings with the live results view.
        let text_cfg_before = (self.config.text_size, self.config.text_mono);
        ui.horizontal_wrapped(|ui| {
            ui.label(subtle(t(lang, "글자 크기")));
            ui.add(egui::Slider::new(&mut self.config.text_size, 10.0..=28.0).suffix("px"));
            ui.checkbox(&mut self.config.text_mono, t(lang, "고정폭"));
        });
        if (self.config.text_size, self.config.text_mono) != text_cfg_before {
            self.mark_dirty();
        }
        // Shows a representative/generated image above the output when present (same spec as the results panel).
        if let Some(path) = image_path {
            if let Some(tex) = load_node_image(ctx, &mut self.node_textures, id, &path) {
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
        let body = output_text(&output, self.config.text_size, self.config.text_mono);
        output_frame(ctx).show(ui, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .scroll_source(text_scroll_source())
                .show(ui, |ui| {
                    ui.add(egui::Label::new(body).wrap().selectable(true));
                });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if theme_button(ui, t(lang, "출력 복사"), pal.accent).clicked() {
                ctx.copy_text(output.clone());
                self.status = t(lang, "출력을 클립보드에 복사했습니다.").to_string();
            }
            if ui.button(t(lang, "닫기")).clicked() {
                self.output_window = None;
            }
        });
    }

    /// Combined-results zoom window: shows the merged full output in a large scroll view.
    pub(crate) fn render_combined_zoom(&mut self, ctx: &egui::Context) {
        let lang = self.config.lang;
        if !self.show_combined_zoom {
            return;
        }
        let text = combined_results(&self.nodes);
        self.stage_modal_rect(ctx, "modal-combined-zoom");
        let mut open = true;
        let mut close_clicked = false;
        egui::Window::new(t(lang, "전체 결과 — 전문"))
            .id(egui::Id::new("modal-combined-zoom"))
            .open(&mut open)
            .resizable(true)
            .default_size([620.0, 440.0])
            .min_size([360.0, 240.0])
            .show(ctx, |ui| {
                // Keeps content inside the scroll area so the window does not fill the screen.
                // Users resize freely by dragging corners (no max size).
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(340.0)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(&text).wrap().selectable(true));
                    });
                ui.horizontal(|ui| {
                    if ui.button(t(lang, "전체 복사")).clicked() {
                        ctx.copy_text(text.clone());
                        self.status = t(lang, "전체 결과를 복사했습니다.").to_string();
                    }
                    if ui.button(t(lang, "닫기")).clicked() {
                        close_clicked = true;
                    }
                });
            });
        if !open || close_clicked {
            self.show_combined_zoom = false;
        }
    }
}
