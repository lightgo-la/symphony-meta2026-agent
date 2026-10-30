//! 34 premium design themes: switch accent, menu, result pane, and canvas at once.

use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, Visuals};

/// Theme selection. Persisted to settings. Compatible with legacy values (light/dark/midnight).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Light,
    Dark,
    Midnight,
    MetaBlue,
    Ocean,
    Forest,
    Sunset,
    Sakura,
    Lavender,
    Mint,
    Sand,
    Paper,
    Slate,
    Crimson,
    Emerald,
    Gold,
    Grape,
    Coffee,
    Arctic,
    Coral,
    Mono,
    Contrast,
    Rose,
    Sky,
    Meadow,
    Peach,
    Teal,
    Indigo,
    Olive,
    Plum,
    Ivory,
    Dusk,
    Storm,
    Charcoal,
}

impl Theme {
    /// Full list of 34 (for the settings combo box).
    pub const ALL: [Theme; 34] = [
        Theme::Light,
        Theme::Dark,
        Theme::Midnight,
        Theme::MetaBlue,
        Theme::Ocean,
        Theme::Forest,
        Theme::Sunset,
        Theme::Sakura,
        Theme::Lavender,
        Theme::Mint,
        Theme::Sand,
        Theme::Paper,
        Theme::Slate,
        Theme::Crimson,
        Theme::Emerald,
        Theme::Gold,
        Theme::Grape,
        Theme::Coffee,
        Theme::Arctic,
        Theme::Coral,
        Theme::Mono,
        Theme::Contrast,
        Theme::Rose,
        Theme::Sky,
        Theme::Meadow,
        Theme::Peach,
        Theme::Teal,
        Theme::Indigo,
        Theme::Olive,
        Theme::Plum,
        Theme::Ivory,
        Theme::Dusk,
        Theme::Storm,
        Theme::Charcoal,
    ];

    pub fn label(&self, lang: crate::i18n::Lang) -> &'static str {
        let ko = matches!(lang, crate::i18n::Lang::Ko);
        match self {
            Self::Light => {
                if ko {
                    "라이트"
                } else {
                    "Light"
                }
            }
            Self::Dark => {
                if ko {
                    "다크"
                } else {
                    "Dark"
                }
            }
            Self::Midnight => {
                if ko {
                    "미드나이트"
                } else {
                    "Midnight"
                }
            }
            Self::MetaBlue => {
                if ko {
                    "메타 블루"
                } else {
                    "Meta Blue"
                }
            }
            Self::Ocean => {
                if ko {
                    "오션"
                } else {
                    "Ocean"
                }
            }
            Self::Forest => {
                if ko {
                    "포레스트"
                } else {
                    "Forest"
                }
            }
            Self::Sunset => {
                if ko {
                    "선셋"
                } else {
                    "Sunset"
                }
            }
            Self::Sakura => {
                if ko {
                    "사쿠라"
                } else {
                    "Sakura"
                }
            }
            Self::Lavender => {
                if ko {
                    "라벤더"
                } else {
                    "Lavender"
                }
            }
            Self::Mint => {
                if ko {
                    "민트"
                } else {
                    "Mint"
                }
            }
            Self::Sand => {
                if ko {
                    "샌드"
                } else {
                    "Sand"
                }
            }
            Self::Paper => {
                if ko {
                    "페이퍼"
                } else {
                    "Paper"
                }
            }
            Self::Slate => {
                if ko {
                    "슬레이트"
                } else {
                    "Slate"
                }
            }
            Self::Crimson => {
                if ko {
                    "크림슨"
                } else {
                    "Crimson"
                }
            }
            Self::Emerald => {
                if ko {
                    "에메랄드"
                } else {
                    "Emerald"
                }
            }
            Self::Gold => {
                if ko {
                    "골드"
                } else {
                    "Gold"
                }
            }
            Self::Grape => {
                if ko {
                    "그레이프"
                } else {
                    "Grape"
                }
            }
            Self::Coffee => {
                if ko {
                    "커피"
                } else {
                    "Coffee"
                }
            }
            Self::Arctic => {
                if ko {
                    "아크틱"
                } else {
                    "Arctic"
                }
            }
            Self::Coral => {
                if ko {
                    "코랄"
                } else {
                    "Coral"
                }
            }
            Self::Mono => {
                if ko {
                    "모노"
                } else {
                    "Mono"
                }
            }
            Self::Contrast => {
                if ko {
                    "고대비"
                } else {
                    "High Contrast"
                }
            }
            Self::Rose => {
                if ko {
                    "로즈"
                } else {
                    "Rose"
                }
            }
            Self::Sky => {
                if ko {
                    "스카이"
                } else {
                    "Sky"
                }
            }
            Self::Meadow => {
                if ko {
                    "메도우"
                } else {
                    "Meadow"
                }
            }
            Self::Peach => {
                if ko {
                    "피치"
                } else {
                    "Peach"
                }
            }
            Self::Teal => {
                if ko {
                    "틸"
                } else {
                    "Teal"
                }
            }
            Self::Indigo => {
                if ko {
                    "인디고"
                } else {
                    "Indigo"
                }
            }
            Self::Olive => {
                if ko {
                    "올리브"
                } else {
                    "Olive"
                }
            }
            Self::Plum => {
                if ko {
                    "플럼"
                } else {
                    "Plum"
                }
            }
            Self::Ivory => {
                if ko {
                    "아이보리"
                } else {
                    "Ivory"
                }
            }
            Self::Dusk => {
                if ko {
                    "더스크"
                } else {
                    "Dusk"
                }
            }
            Self::Storm => {
                if ko {
                    "스톰"
                } else {
                    "Storm"
                }
            }
            Self::Charcoal => {
                if ko {
                    "차콜"
                } else {
                    "Charcoal"
                }
            }
        }
    }
}

/// Full color set of one theme. Menu/result panes stay readable:
/// pastel tones for light themes, navy tones for dark themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    pub accent: Color32,
    pub accent_dark: Color32,
    pub soft: Color32,
    pub canvas: Color32,
    pub menu: Color32,
    pub result: Color32,
    pub window: Color32,
    pub faint: Color32,
    pub brand: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            dark: false,
            accent: rgb(0, 100, 224),
            accent_dark: rgb(0, 82, 184),
            soft: rgb(231, 240, 254),
            canvas: rgb(255, 255, 255),
            menu: rgb(186, 212, 249),
            result: rgb(233, 242, 255),
            window: rgb(255, 255, 255),
            faint: rgb(228, 233, 240),
            brand: rgb(0, 100, 224),
        },
        Theme::Dark => Palette {
            dark: true,
            accent: rgb(66, 154, 255),
            accent_dark: rgb(120, 190, 255),
            soft: rgb(28, 58, 104),
            canvas: rgb(16, 20, 28),
            menu: rgb(22, 38, 66),
            result: rgb(20, 34, 60),
            window: rgb(30, 34, 44),
            faint: rgb(44, 50, 64),
            brand: rgb(120, 190, 255),
        },
        Theme::Midnight => Palette {
            dark: true,
            accent: rgb(80, 170, 255),
            accent_dark: rgb(140, 205, 255),
            soft: rgb(24, 66, 128),
            canvas: rgb(5, 12, 24),
            menu: rgb(12, 26, 50),
            result: rgb(14, 28, 52),
            window: rgb(14, 28, 52),
            faint: rgb(20, 38, 68),
            brand: rgb(140, 205, 255),
        },
        Theme::MetaBlue => Palette {
            dark: false,
            accent: rgb(0, 80, 200),
            accent_dark: rgb(0, 60, 160),
            soft: rgb(214, 230, 252),
            canvas: rgb(255, 255, 255),
            menu: rgb(150, 190, 245),
            result: rgb(224, 236, 252),
            window: rgb(255, 255, 255),
            faint: rgb(210, 224, 244),
            brand: rgb(0, 70, 180),
        },
        Theme::Ocean => Palette {
            dark: false,
            accent: rgb(0, 140, 170),
            accent_dark: rgb(0, 110, 135),
            soft: rgb(214, 242, 246),
            canvas: rgb(255, 255, 255),
            menu: rgb(170, 225, 235),
            result: rgb(228, 246, 249),
            window: rgb(255, 255, 255),
            faint: rgb(208, 232, 236),
            brand: rgb(0, 120, 150),
        },
        Theme::Forest => Palette {
            dark: false,
            accent: rgb(22, 140, 70),
            accent_dark: rgb(15, 110, 55),
            soft: rgb(218, 242, 226),
            canvas: rgb(255, 255, 255),
            menu: rgb(175, 225, 190),
            result: rgb(230, 246, 234),
            window: rgb(255, 255, 255),
            faint: rgb(212, 230, 216),
            brand: rgb(18, 120, 60),
        },
        Theme::Sunset => Palette {
            dark: false,
            accent: rgb(220, 100, 20),
            accent_dark: rgb(180, 78, 12),
            soft: rgb(252, 232, 214),
            canvas: rgb(255, 255, 255),
            menu: rgb(248, 205, 165),
            result: rgb(253, 240, 226),
            window: rgb(255, 255, 255),
            faint: rgb(240, 222, 202),
            brand: rgb(195, 85, 15),
        },
        Theme::Sakura => Palette {
            dark: false,
            accent: rgb(210, 80, 135),
            accent_dark: rgb(175, 60, 110),
            soft: rgb(250, 224, 235),
            canvas: rgb(255, 255, 255),
            menu: rgb(244, 190, 210),
            result: rgb(251, 234, 242),
            window: rgb(255, 255, 255),
            faint: rgb(238, 214, 224),
            brand: rgb(190, 65, 120),
        },
        Theme::Lavender => Palette {
            dark: false,
            accent: rgb(120, 90, 200),
            accent_dark: rgb(95, 68, 170),
            soft: rgb(232, 226, 250),
            canvas: rgb(255, 255, 255),
            menu: rgb(200, 185, 242),
            result: rgb(240, 235, 251),
            window: rgb(255, 255, 255),
            faint: rgb(222, 214, 240),
            brand: rgb(100, 72, 180),
        },
        Theme::Mint => Palette {
            dark: false,
            accent: rgb(0, 150, 130),
            accent_dark: rgb(0, 120, 105),
            soft: rgb(214, 244, 236),
            canvas: rgb(255, 255, 255),
            menu: rgb(168, 230, 214),
            result: rgb(228, 248, 242),
            window: rgb(255, 255, 255),
            faint: rgb(206, 232, 224),
            brand: rgb(0, 130, 112),
        },
        Theme::Sand => Palette {
            dark: false,
            accent: rgb(170, 120, 50),
            accent_dark: rgb(140, 96, 38),
            soft: rgb(246, 232, 208),
            canvas: rgb(255, 253, 248),
            menu: rgb(232, 208, 170),
            result: rgb(248, 240, 222),
            window: rgb(255, 253, 248),
            faint: rgb(230, 214, 186),
            brand: rgb(150, 102, 40),
        },
        Theme::Paper => Palette {
            dark: false,
            accent: rgb(130, 95, 60),
            accent_dark: rgb(105, 75, 45),
            soft: rgb(240, 230, 212),
            canvas: rgb(250, 246, 236),
            menu: rgb(226, 210, 182),
            result: rgb(244, 236, 220),
            window: rgb(250, 246, 236),
            faint: rgb(224, 208, 182),
            brand: rgb(112, 80, 48),
        },
        Theme::Slate => Palette {
            dark: true,
            accent: rgb(120, 160, 210),
            accent_dark: rgb(160, 195, 230),
            soft: rgb(48, 64, 88),
            canvas: rgb(20, 26, 36),
            menu: rgb(32, 44, 62),
            result: rgb(28, 40, 56),
            window: rgb(34, 42, 56),
            faint: rgb(52, 64, 84),
            brand: rgb(160, 195, 230),
        },
        Theme::Crimson => Palette {
            dark: true,
            accent: rgb(230, 90, 110),
            accent_dark: rgb(255, 140, 155),
            soft: rgb(88, 32, 44),
            canvas: rgb(28, 14, 18),
            menu: rgb(52, 24, 32),
            result: rgb(46, 22, 30),
            window: rgb(44, 24, 30),
            faint: rgb(66, 32, 42),
            brand: rgb(255, 150, 165),
        },
        Theme::Emerald => Palette {
            dark: true,
            accent: rgb(40, 200, 130),
            accent_dark: rgb(110, 230, 170),
            soft: rgb(24, 80, 58),
            canvas: rgb(10, 26, 20),
            menu: rgb(18, 48, 36),
            result: rgb(16, 44, 34),
            window: rgb(18, 44, 34),
            faint: rgb(28, 62, 48),
            brand: rgb(110, 230, 170),
        },
        Theme::Gold => Palette {
            dark: true,
            accent: rgb(220, 170, 60),
            accent_dark: rgb(245, 205, 120),
            soft: rgb(84, 62, 26),
            canvas: rgb(26, 20, 12),
            menu: rgb(48, 36, 20),
            result: rgb(44, 34, 20),
            window: rgb(42, 32, 20),
            faint: rgb(62, 48, 28),
            brand: rgb(245, 205, 120),
        },
        Theme::Grape => Palette {
            dark: true,
            accent: rgb(170, 120, 235),
            accent_dark: rgb(200, 160, 250),
            soft: rgb(62, 40, 96),
            canvas: rgb(20, 14, 30),
            menu: rgb(38, 26, 58),
            result: rgb(34, 24, 52),
            window: rgb(36, 26, 54),
            faint: rgb(52, 38, 78),
            brand: rgb(200, 160, 250),
        },
        Theme::Coffee => Palette {
            dark: true,
            accent: rgb(200, 150, 100),
            accent_dark: rgb(230, 185, 140),
            soft: rgb(78, 56, 38),
            canvas: rgb(24, 18, 14),
            menu: rgb(44, 32, 24),
            result: rgb(40, 30, 22),
            window: rgb(40, 30, 22),
            faint: rgb(58, 42, 30),
            brand: rgb(230, 185, 140),
        },
        Theme::Arctic => Palette {
            dark: true,
            accent: rgb(110, 210, 240),
            accent_dark: rgb(170, 230, 250),
            soft: rgb(30, 70, 92),
            canvas: rgb(12, 22, 30),
            menu: rgb(22, 42, 56),
            result: rgb(20, 38, 52),
            window: rgb(22, 40, 54),
            faint: rgb(32, 56, 72),
            brand: rgb(170, 230, 250),
        },
        Theme::Coral => Palette {
            dark: false,
            accent: rgb(225, 95, 75),
            accent_dark: rgb(190, 72, 55),
            soft: rgb(250, 226, 220),
            canvas: rgb(255, 255, 255),
            menu: rgb(244, 192, 180),
            result: rgb(251, 236, 230),
            window: rgb(255, 255, 255),
            faint: rgb(236, 210, 200),
            brand: rgb(200, 80, 60),
        },
        Theme::Mono => Palette {
            dark: false,
            accent: rgb(60, 60, 60),
            accent_dark: rgb(30, 30, 30),
            soft: rgb(232, 232, 232),
            canvas: rgb(255, 255, 255),
            menu: rgb(210, 210, 210),
            result: rgb(240, 240, 240),
            window: rgb(255, 255, 255),
            faint: rgb(220, 220, 220),
            brand: rgb(40, 40, 40),
        },
        Theme::Contrast => Palette {
            dark: false,
            accent: rgb(0, 0, 0),
            accent_dark: rgb(0, 0, 0),
            soft: rgb(255, 235, 120),
            canvas: rgb(255, 255, 255),
            menu: rgb(255, 235, 120),
            result: rgb(255, 248, 210),
            window: rgb(255, 255, 255),
            faint: rgb(230, 230, 230),
            brand: rgb(0, 0, 0),
        },
        Theme::Rose => Palette {
            dark: false,
            accent: rgb(219, 39, 119),
            accent_dark: rgb(180, 25, 95),
            soft: rgb(251, 230, 240),
            canvas: rgb(255, 255, 255),
            menu: rgb(250, 210, 225),
            result: rgb(253, 235, 242),
            window: rgb(255, 255, 255),
            faint: rgb(240, 222, 230),
            brand: rgb(219, 39, 119),
        },
        Theme::Sky => Palette {
            dark: false,
            accent: rgb(2, 132, 199),
            accent_dark: rgb(2, 105, 160),
            soft: rgb(224, 242, 254),
            canvas: rgb(255, 255, 255),
            menu: rgb(186, 230, 253),
            result: rgb(228, 244, 255),
            window: rgb(255, 255, 255),
            faint: rgb(214, 228, 240),
            brand: rgb(2, 132, 199),
        },
        Theme::Meadow => Palette {
            dark: false,
            accent: rgb(22, 140, 70),
            accent_dark: rgb(15, 110, 55),
            soft: rgb(220, 244, 228),
            canvas: rgb(255, 255, 255),
            menu: rgb(187, 240, 205),
            result: rgb(224, 250, 233),
            window: rgb(255, 255, 255),
            faint: rgb(212, 230, 218),
            brand: rgb(22, 140, 70),
        },
        Theme::Peach => Palette {
            dark: false,
            accent: rgb(220, 90, 15),
            accent_dark: rgb(180, 70, 10),
            soft: rgb(255, 236, 220),
            canvas: rgb(255, 255, 255),
            menu: rgb(252, 222, 194),
            result: rgb(255, 240, 228),
            window: rgb(255, 255, 255),
            faint: rgb(240, 224, 208),
            brand: rgb(220, 90, 15),
        },
        Theme::Teal => Palette {
            dark: false,
            accent: rgb(13, 140, 130),
            accent_dark: rgb(10, 110, 102),
            soft: rgb(210, 244, 240),
            canvas: rgb(255, 255, 255),
            menu: rgb(160, 235, 226),
            result: rgb(214, 248, 244),
            window: rgb(255, 255, 255),
            faint: rgb(206, 228, 226),
            brand: rgb(13, 140, 130),
        },
        Theme::Indigo => Palette {
            dark: false,
            accent: rgb(79, 70, 229),
            accent_dark: rgb(60, 52, 185),
            soft: rgb(229, 231, 255),
            canvas: rgb(255, 255, 255),
            menu: rgb(199, 210, 254),
            result: rgb(228, 232, 255),
            window: rgb(255, 255, 255),
            faint: rgb(216, 220, 240),
            brand: rgb(79, 70, 229),
        },
        Theme::Olive => Palette {
            dark: false,
            accent: rgb(95, 150, 15),
            accent_dark: rgb(75, 120, 10),
            soft: rgb(236, 248, 210),
            canvas: rgb(255, 255, 255),
            menu: rgb(215, 240, 160),
            result: rgb(238, 250, 214),
            window: rgb(255, 255, 255),
            faint: rgb(222, 232, 200),
            brand: rgb(95, 150, 15),
        },
        Theme::Plum => Palette {
            dark: false,
            accent: rgb(162, 28, 175),
            accent_dark: rgb(130, 20, 140),
            soft: rgb(248, 228, 252),
            canvas: rgb(255, 255, 255),
            menu: rgb(240, 205, 250),
            result: rgb(249, 232, 254),
            window: rgb(255, 255, 255),
            faint: rgb(232, 214, 238),
            brand: rgb(162, 28, 175),
        },
        Theme::Ivory => Palette {
            dark: false,
            accent: rgb(170, 115, 35),
            accent_dark: rgb(140, 92, 25),
            soft: rgb(250, 242, 224),
            canvas: rgb(255, 251, 235),
            menu: rgb(242, 226, 195),
            result: rgb(250, 240, 220),
            window: rgb(255, 253, 245),
            faint: rgb(232, 220, 196),
            brand: rgb(170, 115, 35),
        },
        Theme::Dusk => Palette {
            dark: true,
            accent: rgb(196, 132, 255),
            accent_dark: rgb(220, 175, 255),
            soft: rgb(64, 44, 100),
            canvas: rgb(24, 18, 38),
            menu: rgb(48, 32, 72),
            result: rgb(40, 28, 62),
            window: rgb(30, 24, 48),
            faint: rgb(58, 44, 88),
            brand: rgb(196, 132, 255),
        },
        Theme::Storm => Palette {
            dark: true,
            accent: rgb(96, 165, 250),
            accent_dark: rgb(140, 195, 255),
            soft: rgb(38, 62, 100),
            canvas: rgb(15, 23, 42),
            menu: rgb(35, 50, 75),
            result: rgb(28, 40, 62),
            window: rgb(30, 41, 59),
            faint: rgb(48, 64, 92),
            brand: rgb(96, 165, 250),
        },
        Theme::Charcoal => Palette {
            dark: true,
            accent: rgb(255, 180, 60),
            accent_dark: rgb(255, 205, 120),
            soft: rgb(70, 58, 40),
            canvas: rgb(12, 12, 14),
            menu: rgb(40, 40, 46),
            result: rgb(32, 32, 38),
            window: rgb(24, 24, 27),
            faint: rgb(54, 54, 60),
            brand: rgb(255, 180, 60),
        },
    }
}

/// Meta Blue (button/link default).
pub const META_BLUE: Color32 = rgb(0, 100, 224);
/// Light blue background (output box, selection area).
pub const META_BLUE_SOFT: Color32 = rgb(231, 240, 254);
/// Card border.
pub const CARD_STROKE: Color32 = rgb(216, 225, 236);
/// Dark navy text.
pub const INK: Color32 = rgb(5, 5, 5);
/// Secondary text.
pub const INK_SUB: Color32 = rgb(101, 103, 107);

/// Called at app startup and on theme change.
pub fn apply_meta_theme(ctx: &egui::Context, theme: Theme) {
    let pal = palette(theme);
    let mut v = if pal.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = pal.dark;
    v.panel_fill = pal.canvas;
    v.window_fill = pal.window;
    v.faint_bg_color = pal.faint;
    v.extreme_bg_color = pal.faint;
    v.code_bg_color = pal.soft;
    v.hyperlink_color = pal.accent;
    v.selection.bg_fill = pal.soft;
    v.selection.stroke = Stroke::new(1.0_f32, pal.accent);
    if pal.dark {
        v.widgets.inactive.bg_fill = pal.window;
        v.widgets.hovered.bg_fill = pal.soft;
        v.widgets.active.bg_fill = pal.soft;
        v.window_stroke = Stroke::new(1.0_f32, pal.faint);
    } else {
        v.widgets.inactive.bg_fill = pal.window;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.hovered.bg_fill = pal.soft;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, pal.accent_dark);
        v.widgets.active.bg_fill = pal.soft;
        v.window_stroke = Stroke::new(1.0_f32, CARD_STROKE);
    }
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    v.button_frame = true;
    ctx.set_visuals(v);
}

/// Theme accent button (white bold text).
pub fn theme_button(ui: &mut egui::Ui, text: &str, accent: Color32) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(accent))
}

/// HUD panel frame: flat fill + 1px hairline + square corners.
/// Accents via an inner accent bar instead of a thick outline.
pub fn hud_panel_frame(fill: Color32, edge: Color32) -> egui::Frame {
    egui::Frame::default()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, edge))
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(Margin::same(10))
}

/// HUD hairline: thin divider + short left accent tick.
pub fn hud_rule(ui: &mut egui::Ui, accent: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 10.0), egui::Sense::hover());
    let base = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let y = rect.center().y;
    ui.painter().line_segment(
        [
            egui::Pos2::new(rect.min.x, y),
            egui::Pos2::new(rect.max.x, y),
        ],
        Stroke::new(1.0_f32, base),
    );
    ui.painter().line_segment(
        [
            egui::Pos2::new(rect.min.x, y),
            egui::Pos2::new(rect.min.x + 28.0, y),
        ],
        Stroke::new(2.5_f32, accent),
    );
}

/// Status pill chip. Placed inline, side by side.
pub fn pill(ui: &mut egui::Ui, text: impl Into<String>, fill: Color32, fg: Color32) {
    egui::Frame::default()
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin {
            left: 9,
            right: 9,
            top: 3,
            bottom: 3,
        })
        .show(ui, |ui| {
            ui.label(RichText::new(text.into()).size(12.5).strong().color(fg));
        });
}

/// Full-width HUD primary button.
pub fn hud_button(ui: &mut egui::Ui, text: &str, fill: Color32) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), 30.0],
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(fill),
    )
}

/// Inspector card frame: one step above the background + rounded corners.
pub fn hud_card_frame(fill: Color32, edge: Color32) -> egui::Frame {
    egui::Frame::default()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, edge))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
}

/// Brand logo plate: accent square + one contrasting glyph.
pub fn logo_plate(ui: &mut egui::Ui, accent: Color32, glyph: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(7), accent);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(19.0),
        Color32::WHITE,
    );
}

/// Styled card frame. Blends a touch of accent into the border.
pub fn card_frame_styled(ctx: &egui::Context, accent: Color32, radius: u8) -> egui::Frame {
    let visuals = ctx.style().visuals.clone();
    let base = visuals.window_stroke.color;
    let mixed = Color32::from_rgb(
        mix_u8(base.r(), accent.r(), 0.35),
        mix_u8(base.g(), accent.g(), 0.35),
        mix_u8(base.b(), accent.b(), 0.35),
    );
    egui::Frame::default()
        .fill(visuals.window_fill)
        .stroke(Stroke::new(visuals.window_stroke.width.max(1.0), mixed))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(Margin::same(10))
        .shadow(visuals.window_shadow)
}

fn mix_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Output area frame.
pub fn output_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::default()
        .fill(ctx.style().visuals.code_bg_color)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(8))
}

/// Brand title.
pub fn brand_title(text: &str, color: Color32) -> RichText {
    RichText::new(text).size(19.0).strong().color(color)
}

/// Section subtitle. Color comes from the theme.
pub fn section_title(text: &str) -> RichText {
    RichText::new(text).size(14.0).strong()
}

/// Helper text. Mid gray readable on both light and dark themes.
/// Scroll setup for text selection: drag yields to selection; scroll via wheel/scrollbar.
/// With drag-scroll on, dragging pans the view so text cannot be selected.
pub fn text_scroll_source() -> egui::containers::scroll_area::ScrollSource {
    egui::containers::scroll_area::ScrollSource {
        drag: false,
        ..Default::default()
    }
}

/// Output text: shared body for the live/output windows. Size/monospace come from settings.
pub fn output_text(text: &str, size: f32, mono: bool) -> RichText {
    let rt = RichText::new(text).size(size.clamp(8.0, 72.0));
    if mono {
        rt.monospace()
    } else {
        rt
    }
}

pub fn subtle(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(Color32::from_rgb(136, 136, 136))
}

/// Dotted vertical line for section boundaries.
pub fn dotted_vline(painter: &egui::Painter, x: f32, y0: f32, y1: f32, color: Color32) {
    let mut y = y0;
    while y < y1 {
        painter.circle_filled(egui::Pos2::new(x, y), 1.4, color);
        y += 7.0;
    }
}

/// Effective canvas background: the custom color when set, else the theme palette.
pub fn canvas_color(theme: Theme, bg: crate::store::CanvasBg) -> Color32 {
    match bg {
        crate::store::CanvasBg::Theme => palette(theme).canvas,
        crate::store::CanvasBg::Custom([r, g, b]) => Color32::from_rgb(r, g, b),
    }
}

/// Grid color matched to background brightness: cool gray when light, bright slate when dark.
pub fn grid_color(bg: Color32) -> Color32 {
    let lum = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if lum > 128.0 {
        Color32::from_rgb(203, 213, 225)
    } else {
        Color32::from_rgb(110, 130, 160)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_thirty_four_themes_roundtrip() {
        assert_eq!(Theme::ALL.len(), 34);
        for th in Theme::ALL {
            let s = serde_json::to_string(&th).unwrap();
            let back: Theme = serde_json::from_str(&s).unwrap();
            assert_eq!(back, th);
            assert!(!th.label(crate::i18n::Lang::Ko).is_empty());
            assert!(!th.label(crate::i18n::Lang::En).is_empty());
        }
    }

    #[test]
    fn text_scroll_source_leaves_drag_to_selection() {
        let s = text_scroll_source();
        assert!(!s.drag);
        assert!(s.mouse_wheel);
        assert!(s.scroll_bar);
    }

    #[test]
    fn old_theme_names_still_load() {
        let light: Theme = serde_json::from_str("\"light\"").unwrap();
        assert_eq!(light, Theme::Light);
        let dark: Theme = serde_json::from_str("\"dark\"").unwrap();
        assert_eq!(dark, Theme::Dark);
        let mid: Theme = serde_json::from_str("\"midnight\"").unwrap();
        assert_eq!(mid, Theme::Midnight);
    }

    #[test]
    fn canvas_helpers_behave() {
        use crate::store::CanvasBg;
        assert_eq!(
            canvas_color(Theme::Light, CanvasBg::Theme),
            palette(Theme::Light).canvas
        );
        assert_eq!(
            canvas_color(Theme::Dark, CanvasBg::Custom([1, 2, 3])),
            Color32::from_rgb(1, 2, 3)
        );
        assert_eq!(grid_color(Color32::WHITE), Color32::from_rgb(203, 213, 225));
        assert_eq!(grid_color(Color32::BLACK), Color32::from_rgb(110, 130, 160));
    }

    #[test]
    fn palettes_have_visible_accents() {
        for th in Theme::ALL {
            let p = palette(th);
            // Checks the accent does not blend into the background (luminance gap).
            let lum =
                |c: Color32| 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
            assert!(
                (lum(p.accent) - lum(p.window)).abs() > 20.0,
                "{th:?} accent too close to window"
            );
        }
    }
}
