//! Output sharing: mail, SNS, text, documents.
//!
//! Channels without text-link sharing (Instagram, etc.)
//! copy the caption to the clipboard, then open the site.

/// Percent-encodes a UTF-8 string.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Truncates by chars (avoids breaking emoji/Hangul).
pub fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

/// Truncates by encoded length. Never splits a %XX sequence.
fn truncate_encoded(text: &str, limit: usize) -> String {
    let enc = percent_encode(text);
    if enc.len() <= limit {
        return enc;
    }
    let bytes = enc.as_bytes();
    let mut end = limit.min(bytes.len());
    while end > 0 {
        if bytes[end - 1] == b'%' {
            end -= 1;
        } else if end >= 2 && bytes[end - 2] == b'%' {
            end -= 2;
        } else {
            break;
        }
    }
    enc[..end].to_string()
}

/// mailto link that opens the mail app. Truncates by post-encoding length.
/// Hangul grows up to 9x when encoded, so truncating by pre-encoding
/// char count would far exceed the mail app limit.
pub fn mailto_url(subject: &str, body: &str) -> String {
    format!(
        "mailto:?subject={}&body={}",
        truncate_encoded(subject, 500),
        truncate_encoded(body, 1800)
    )
}

/// Facebook has no text prefill, so this opens home.
/// The caption is copied to the clipboard first (same flow as Instagram).
pub fn facebook_url() -> &'static str {
    "https://www.facebook.com/"
}

/// Threads post-intent URL.
pub fn threads_url(text: &str) -> String {
    format!(
        "https://www.threads.com/intent/post?text={}",
        percent_encode(&truncate_chars(text, 1500))
    )
}

/// Instagram has no text prefill, so this opens home.
pub fn instagram_url() -> &'static str {
    "https://www.instagram.com/"
}

/// Builds markdown for saving a document. Title newlines would break
/// document structure, so they are flattened to spaces.
pub fn to_markdown(title: &str, body: &str) -> String {
    let title = title.replace(['\r', '\n'], " ");
    format!("# {title}\n\n{body}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_korean_and_symbols() {
        assert_eq!(percent_encode("가 나"), "%EA%B0%80%20%EB%82%98");
        assert_eq!(percent_encode("abc-_.~"), "abc-_.~");
    }

    #[test]
    fn truncates_by_chars_not_bytes() {
        assert_eq!(truncate_chars("가나다라", 2), "가나");
        assert_eq!(truncate_chars("abc", 10), "abc");
    }

    #[test]
    fn mailto_has_subject_and_body() {
        let url = mailto_url("제목", "본문");
        assert!(url.starts_with("mailto:?subject="));
        assert!(url.contains("&body="));
        assert!(!url.contains(' '));
    }

    #[test]
    fn sns_urls_are_https() {
        assert!(facebook_url().starts_with("https://"));
        assert!(threads_url("hi").starts_with("https://"));
        assert!(instagram_url().starts_with("https://"));
    }

    #[test]
    fn mailto_stays_within_encoded_limit() {
        // 1000 Hangul chars exceed 9000 chars when encoded. Must be truncated.
        let long = "가".repeat(1000);
        let url = mailto_url("제", &long);
        let body = url.split("&body=").nth(1).unwrap();
        assert!(body.len() <= 1800);
        // The cut end must not be mid-%XX.
        assert!(!body.ends_with('%'));
        assert!(
            body.len() < 2
                || body.as_bytes()[body.len() - 2] != b'%'
                || body.len() >= 3 && body.as_bytes()[body.len() - 3] == b'%'
        );
    }

    #[test]
    fn markdown_has_title_first() {
        let md = to_markdown("T", "B");
        assert!(md.starts_with("# T\n\nB"));
    }

    #[test]
    fn markdown_flattens_title_newlines() {
        let md = to_markdown("A\nB", "C");
        assert!(md.starts_with("# A B\n\nC"));
    }
}
