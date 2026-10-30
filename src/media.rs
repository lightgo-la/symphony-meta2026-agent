//! Image/audio generation. Uses public keyless HTTP endpoints.
//!
//! - Images: pollinations.ai (prompt → PNG)
//! - Audio: Google Translate TTS (sentence → MP3; unofficial endpoint, so a block is reported as an error)

use std::time::Duration;

/// Kind of media to create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Audio,
}

impl MediaKind {
    pub fn ext(&self) -> &'static str {
        match self {
            MediaKind::Image => "png",
            MediaKind::Audio => "mp3",
        }
    }
}

/// Minimal URL encoding (keeps alphanumerics and -_.~; everything else becomes %XX).
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else if b == b' ' {
            out.push_str("%20");
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Uses Korean TTS if any Hangul syllable (U+AC00-U+D7A3) is present, English TTS otherwise.
pub fn detect_lang(text: &str) -> &'static str {
    if text.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)) {
        "ko"
    } else {
        "en"
    }
}

/// Image-generation URL.
pub fn image_url(prompt: &str, seed: u32) -> String {
    format!(
        "https://image.pollinations.ai/prompt/{}?width=1024&height=1024&nologo=true&seed={seed}",
        url_encode(prompt)
    )
}

/// TTS only accepts short input per call, so split on whitespace (within 180 chars).
pub fn tts_chunks(text: &str, max: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.chars().count() + word.chars().count() + 1 > max && !cur.is_empty() {
            chunks.push(cur);
            cur = String::new();
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() || chunks.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// URL for one TTS chunk.
pub fn tts_url(chunk: &str, lang: &str) -> String {
    format!(
        "https://translate.google.com/translate_tts?ie=UTF-8&tl={lang}&client=gtx&q={}",
        url_encode(chunk)
    )
}

/// Fetches bytes from a URL. Empty means an error.
pub fn fetch_bytes(url: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let bytes = crate::http::shared_client(timeout)?
        .get(url)
        .send()
        .map_err(|e| format!("다운로드 실패: {e}"))?
        .error_for_status()
        .map_err(|e| format!("서버 오류: {e}"))?
        .bytes()
        .map_err(|e| format!("본문 읽기 실패: {e}"))?
        .to_vec();
    if bytes.is_empty() {
        return Err("빈 응답".to_string());
    }
    Ok(bytes)
}

/// Detects the extension from image magic bytes (PNG/JPEG).
pub fn sniff_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes[0..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        Some("png")
    } else if bytes.len() >= 3 && bytes[0..3] == [255, 216, 255] {
        Some("jpg")
    } else {
        None
    }
}

/// Saves media bytes to the temp folder and returns the path.
pub fn save_media(stem: &str, ext: &str, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("prompt_canvas_media");
    std::fs::create_dir_all(&dir).map_err(|e| format!("폴더 만들기 실패: {e}"))?;
    let path = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&path, bytes).map_err(|e| format!("파일 쓰기 실패: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_keeps_safe_chars() {
        assert_eq!(url_encode("abc-_.~"), "abc-_.~");
        assert_eq!(url_encode("a b"), "a%20b");
        assert!(url_encode("가").starts_with('%'));
    }

    #[test]
    fn lang_detects_korean() {
        assert_eq!(detect_lang("안녕 world"), "ko");
        assert_eq!(detect_lang("hello"), "en");
        assert_eq!(detect_lang("123"), "en");
    }

    #[test]
    fn chunks_stay_within_limit() {
        let text = "word ".repeat(100);
        let chunks = tts_chunks(&text, 180);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(c.chars().count() <= 180, "chunk 초과: {c}");
        }
        assert_eq!(
            chunks.join(" "),
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        );
    }

    /// Real-download smoke test. Requires network access.
    /// cargo test -- --ignored live_media_download
    #[test]
    #[ignore]
    fn live_media_download() {
        let img = fetch_bytes(
            &image_url("a red apple on a table", 7),
            Duration::from_secs(180),
        )
        .expect("이미지 다운로드 실패");
        assert!(
            img.len() > 10_000,
            "이미지가 너무 작다: {}바이트",
            img.len()
        );
        let ext = sniff_image_ext(&img).expect("이미지 형식을 알 수 없음");
        println!("live image: {}바이트 {ext}", img.len());
        let tts = fetch_bytes(&tts_url("hello world", "en"), Duration::from_secs(120))
            .expect("TTS 다운로드 실패");
        assert!(tts.len() > 1_000, "오디오가 너무 작다: {}바이트", tts.len());
        println!("live audio: {}바이트", tts.len());
    }

    #[test]
    fn sniff_tells_png_from_jpeg() {
        assert_eq!(
            sniff_image_ext(&[137, 80, 78, 71, 13, 10, 26, 10, 0]),
            Some("png")
        );
        assert_eq!(sniff_image_ext(&[255, 216, 255, 224]), Some("jpg"));
        assert_eq!(sniff_image_ext(&[0, 1, 2]), None);
        assert_eq!(sniff_image_ext(&[]), None);
    }

    #[test]
    fn urls_contain_params() {
        let u = image_url("cat", 7);
        assert!(u.contains("prompt/cat"));
        assert!(u.contains("seed=7"));
        let t = tts_url("hi", "ko");
        assert!(t.contains("tl=ko"));
        assert!(t.contains("q=hi"));
    }
}
