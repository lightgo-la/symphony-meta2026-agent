//! Result alarms: announce batch completion via Telegram, Discord, and WhatsApp.
//!
//! All three channels need just one HTTPS POST each. Only enabled channels are sent to,
//! and sending runs on a background thread so the UI never blocks.

use std::time::Duration;

/// Per-channel alarm settings. Stored in the config file as-is.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AlarmConfig {
    #[serde(default)]
    pub telegram_on: bool,
    #[serde(default)]
    pub telegram_token: String,
    #[serde(default)]
    pub telegram_chat: String,
    #[serde(default)]
    pub discord_on: bool,
    #[serde(default)]
    pub discord_webhook: String,
    #[serde(default)]
    pub wa_on: bool,
    #[serde(default)]
    pub wa_token: String,
    #[serde(default)]
    pub wa_phone_id: String,
    #[serde(default)]
    pub wa_to: String,
}

/// One destination: (display name, URL, token, body).
pub struct AlarmTarget {
    pub name: &'static str,
    pub url: String,
    pub bearer: Option<String>,
    pub body: serde_json::Value,
}

impl AlarmConfig {
    /// Collects only enabled channels with complete values. Channels with missing settings are quietly skipped.
    pub fn targets(&self, text: &str) -> Vec<AlarmTarget> {
        let mut out = Vec::new();
        if self.telegram_on
            && !self.telegram_token.trim().is_empty()
            && !self.telegram_chat.trim().is_empty()
        {
            out.push(AlarmTarget {
                name: "텔레그램",
                url: telegram_api(self.telegram_token.trim()),
                bearer: None,
                body: telegram_body(self.telegram_chat.trim(), text),
            });
        }
        if self.discord_on && self.discord_webhook.trim().starts_with("http") {
            out.push(AlarmTarget {
                name: "디스코드",
                url: self.discord_webhook.trim().to_string(),
                bearer: None,
                body: discord_body(text),
            });
        }
        if self.wa_on
            && !self.wa_token.trim().is_empty()
            && !self.wa_phone_id.trim().is_empty()
            && !self.wa_to.trim().is_empty()
        {
            out.push(AlarmTarget {
                name: "왓츠앱",
                url: whatsapp_api(self.wa_phone_id.trim()),
                bearer: Some(self.wa_token.trim().to_string()),
                body: whatsapp_body(self.wa_to.trim(), text),
            });
        }
        out
    }

    /// Whether there is at least one real destination to send to.
    pub fn any_ready(&self) -> bool {
        !self.targets("").is_empty()
    }
}

/// Telegram Bot API send URL.
pub fn telegram_api(token: &str) -> String {
    format!("https://api.telegram.org/bot{token}/sendMessage")
}

/// Telegram send body.
pub fn telegram_body(chat_id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({"chat_id": chat_id, "text": text})
}

/// Discord webhook send body.
pub fn discord_body(text: &str) -> serde_json::Value {
    serde_json::json!({"content": text})
}

/// WhatsApp Cloud API send URL.
pub fn whatsapp_api(phone_id: &str) -> String {
    format!("https://graph.facebook.com/v21.0/{phone_id}/messages")
}

/// WhatsApp send body.
pub fn whatsapp_body(to: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "messaging_product": "whatsapp",
        "to": to,
        "type": "text",
        "text": {"body": text},
    })
}

/// POSTs one JSON payload. The shared base for all channel sends.
pub fn post_json(url: &str, bearer: Option<&str>, body: &serde_json::Value) -> Result<(), String> {
    let client = crate::http::shared_client(Duration::from_secs(15))?;
    let mut req = client.post(url).json(body);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    req.send()
        .map_err(|e| format!("전송 실패: {e}"))?
        .error_for_status()
        .map_err(|e| format!("서버 오류: {e}"))?;
    Ok(())
}

/// Sends to every enabled channel. Returns the sent-to names on success, the reasons on failure.
pub fn send_all(cfg: &AlarmConfig, text: &str) -> Result<String, String> {
    let targets = cfg.targets(text);
    if targets.is_empty() {
        return Err("보낼 곳이 없습니다. 채널을 켜고 값을 입력하세요.".to_string());
    }
    let mut sent = Vec::new();
    let mut errs = Vec::new();
    for t in targets {
        match post_json(&t.url, t.bearer.as_deref(), &t.body) {
            Ok(()) => sent.push(t.name),
            Err(e) => errs.push(format!("{}: {e}", t.name)),
        }
    }
    if sent.is_empty() {
        Err(errs.join("; "))
    } else if errs.is_empty() {
        Ok(sent.join(", "))
    } else {
        Ok(format!("{} (실패: {})", sent.join(", "), errs.join("; ")))
    }
}

/// Truncates the alarm body from the front. Kept short to fit the Discord (2000-char) limit.
pub fn cap_text(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_cfg() -> AlarmConfig {
        AlarmConfig {
            telegram_on: true,
            telegram_token: "AAA".to_string(),
            telegram_chat: "123".to_string(),
            discord_on: true,
            discord_webhook: "https://discord.example/hook".to_string(),
            wa_on: true,
            wa_token: "BBB".to_string(),
            wa_phone_id: "456".to_string(),
            wa_to: "821012345678".to_string(),
        }
    }

    #[test]
    fn targets_skips_incomplete_channels() {
        let cfg = AlarmConfig::default();
        assert!(cfg.targets("hi").is_empty());
        assert!(!cfg.any_ready());
        // An empty token is the same as disabled.
        let mut partial = full_cfg();
        partial.telegram_token.clear();
        partial.discord_webhook = "not-a-url".to_string();
        let names: Vec<&str> = partial.targets("hi").iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["왓츠앱"]);
    }

    #[test]
    fn payload_shapes_match_channel_apis() {
        let cfg = full_cfg();
        let targets = cfg.targets("완료");
        assert_eq!(targets.len(), 3);
        let tg = &targets[0];
        assert!(tg.url.contains("api.telegram.org/botAAA/sendMessage"));
        assert_eq!(tg.body["chat_id"], "123");
        assert_eq!(tg.body["text"], "완료");
        assert!(targets[1].body["content"].as_str().is_some());
        let wa = &targets[2];
        assert!(wa.url.contains("graph.facebook.com/v21.0/456/messages"));
        assert_eq!(wa.body["messaging_product"], "whatsapp");
        assert_eq!(wa.body["text"]["body"], "완료");
        assert_eq!(wa.bearer.as_deref(), Some("BBB"));
    }

    #[test]
    fn send_all_without_targets_errors() {
        let err = send_all(&AlarmConfig::default(), "hi").unwrap_err();
        assert!(err.contains("보낼 곳"));
    }

    #[test]
    fn cap_text_cuts_long_body() {
        assert_eq!(cap_text("abc", 10), "abc");
        assert_eq!(cap_text("abcdef", 3), "abc…");
    }
}
