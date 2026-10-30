//! Korean font setup: loads OS system fonts and registers them with egui.
//!
//! Uses the runtime environment's system fonts instead of bundling font files in the repo.
//! Windows: probes Malgun Gothic (malgun.ttf), then Noto Sans CJK, in order.

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

/// Returns the installed font name, or `"기본"` on failure.
pub fn install(ctx: &Context) -> String {
    let found = find_system_font();
    let mut fonts = FontDefinitions::default();
    let name = match found {
        Some((label, bytes)) => {
            fonts
                .font_data
                .insert("app-korean".to_string(), FontData::from_owned(bytes).into());
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "app-korean".to_string());
            }
            label
        }
        None => "기본".to_string(),
    };
    ctx.set_fonts(fonts);

    // Default sizes for readability: 15px body, stronger heading hierarchy.
    ctx.style_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Monospace),
        );
    });
    name
}

fn find_system_font() -> Option<(String, Vec<u8>)> {
    let candidates: Vec<(String, std::path::PathBuf)> = {
        let mut list = Vec::new();
        #[cfg(target_os = "windows")]
        {
            if let Ok(windir) = std::env::var("WINDIR") {
                list.push((
                    "맑은고딕".to_string(),
                    std::path::Path::new(&windir)
                        .join("Fonts")
                        .join("malgun.ttf"),
                ));
            }
        }
        // Environments with Noto Sans CJK installed (covers filename variants).
        for dir in [
            #[cfg(target_os = "windows")]
            std::env::var("WINDIR").map_or_else(
                |_| String::new(),
                |w| {
                    std::path::Path::new(&w)
                        .join("Fonts")
                        .to_string_lossy()
                        .into_owned()
                },
            ),
            #[cfg(target_os = "linux")]
            "/usr/share/fonts".to_string(),
            #[cfg(target_os = "macos")]
            "/System/Library/Fonts".to_string(),
        ] {
            if dir.is_empty() {
                continue;
            }
            for file in [
                "NotoSansKR-Regular.otf",
                "NotoSansKR-Regular.ttf",
                "NotoSansCJKkr-Regular.otf",
            ] {
                list.push((
                    "Noto Sans KR".to_string(),
                    std::path::Path::new(&dir).join(file),
                ));
            }
        }
        list
    };
    for (label, path) in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            if !bytes.is_empty() {
                return Some((label, bytes));
            }
        }
    }
    None
}
