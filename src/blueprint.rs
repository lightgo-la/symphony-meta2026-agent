//! Blueprint: design and create multiple nodes from a one-paragraph description.
//!
//! Flow: description → LLM (JSON plan) → parse + validate → place on canvas.
//! To rule out cyclic links at the source, inputs may only point at earlier nodes.

use crate::node::NodeMode;

/// Maximum number of nodes creatable at once.
pub(crate) const MAX_BLUEPRINT_NODES: usize = 12;

/// Mode wire keys (same order as the combobox).
const MODE_KEYS: &[(&str, NodeMode)] = &[
    ("normal", NodeMode::Normal),
    ("agent", NodeMode::Agent),
    ("image", NodeMode::Image),
    ("audio", NodeMode::Audio),
    ("python", NodeMode::Python),
    ("summarize", NodeMode::Summarize),
    ("translate_en", NodeMode::TranslateEn),
    ("translate_ko", NodeMode::TranslateKo),
    ("polish", NodeMode::Polish),
    ("bullets", NodeMode::Bullets),
    ("table", NodeMode::Table),
    ("expand", NodeMode::Expand),
    ("critique", NodeMode::Critique),
    ("questions", NodeMode::Questions),
    ("explain_code", NodeMode::ExplainCode),
    ("write_code", NodeMode::WriteCode),
    ("email", NodeMode::Email),
    ("sns", NodeMode::Sns),
    ("title", NodeMode::Title),
    ("keywords", NodeMode::Keywords),
    ("plan", NodeMode::Plan),
    ("pros_cons", NodeMode::ProsCons),
    ("eli5", NodeMode::Eli5),
];

/// Also accepts save-file style (lowercase, no separators) spellings.
const MODE_ALIASES: &[(&str, NodeMode)] = &[
    ("translateen", NodeMode::TranslateEn),
    ("translateko", NodeMode::TranslateKo),
    ("explaincode", NodeMode::ExplainCode),
    ("writecode", NodeMode::WriteCode),
    ("proscons", NodeMode::ProsCons),
];

/// Converts a wire key to a mode. Ignores case and surrounding whitespace.
pub(crate) fn parse_mode_key(s: &str) -> Option<NodeMode> {
    let key = s.trim().to_lowercase();
    MODE_KEYS
        .iter()
        .chain(MODE_ALIASES.iter())
        .find(|(k, _)| *k == key)
        .map(|(_, m)| *m)
}

/// System instructions for plan requests. Pins output to JSON only.
pub(crate) fn plan_system_prompt() -> String {
    let keys: Vec<&str> = MODE_KEYS.iter().map(|(k, _)| *k).collect();
    let keys = keys.join(", ");
    let schema = r#"{"nodes":[{"title":"short","prompt":"full runnable instruction","mode":"key","inputs":[0],"system":"","model":null,"temperature":null,"max_tokens":null}]}"#;
    format!(
        "You are a pipeline planner for a node-canvas app. Output ONLY a JSON object, no fences, no prose.\n\
         Schema: {schema}\n\
         Rules:\n\
         - 1 to {MAX_BLUEPRINT_NODES} nodes.\n\
         - mode is one of: {keys}.\n\
         - inputs are 0-based indices this node reads; only SMALLER indices (first node: []).\n\
         - title <= 20 chars; prompt is the complete instruction (<= 1500 chars); system/model/temperature/max_tokens optional.\n\
         - Write title/prompt in the same language as the user description."
    )
}

#[derive(Debug, serde::Deserialize)]
struct RawPlan {
    nodes: Vec<RawNode>,
}

#[derive(Debug, serde::Deserialize)]
struct RawNode {
    #[serde(default)]
    title: String,
    #[serde(default)]
    prompt: String,
    #[serde(default = "default_mode_key")]
    mode: String,
    #[serde(default)]
    inputs: Vec<usize>,
    #[serde(default)]
    system: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

fn default_mode_key() -> String {
    "normal".to_string()
}

/// One validated plan. inputs are indices within the plan.
#[derive(Clone, Debug)]
pub(crate) struct BlueprintPlan {
    pub(crate) nodes: Vec<BlueprintNode>,
}

#[derive(Clone, Debug)]
pub(crate) struct BlueprintNode {
    pub(crate) title: String,
    pub(crate) prompt: String,
    pub(crate) mode: NodeMode,
    pub(crate) inputs: Vec<usize>,
    pub(crate) system: String,
    pub(crate) model: Option<String>,
    pub(crate) temperature: Option<f32>,
    pub(crate) max_tokens: Option<u32>,
}

/// Extracts JSON from the LLM response and builds a validated plan.
pub(crate) fn parse_plan(text: &str) -> Result<BlueprintPlan, String> {
    let json = extract_json(text)?;
    let raw: RawPlan =
        serde_json::from_str(json).map_err(|e| format!("설계 JSON을 읽지 못했습니다: {e}"))?;
    if raw.nodes.is_empty() {
        return Err("노드가 없습니다. 설명을 더 구체적으로 쓰세요.".to_string());
    }
    if raw.nodes.len() > MAX_BLUEPRINT_NODES {
        return Err(format!(
            "노드는 최대 {MAX_BLUEPRINT_NODES}개까지 만들 수 있습니다 ({}개 받음).",
            raw.nodes.len()
        ));
    }
    let total = raw.nodes.len();
    let mut nodes = Vec::with_capacity(total);
    for (i, r) in raw.nodes.into_iter().enumerate() {
        let no = i + 1;
        let mode = parse_mode_key(&r.mode)
            .ok_or_else(|| format!("{no}번 노드: 모드 '{}'를 알 수 없습니다.", r.mode.trim()))?;
        let prompt = r.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(format!("{no}번 노드: 프롬프트가 비어 있습니다."));
        }
        let mut inputs = Vec::with_capacity(r.inputs.len());
        for src in r.inputs {
            if src == i {
                return Err(format!("{no}번 노드: 자기 자신을 입력할 수 없습니다."));
            }
            if src >= total {
                return Err(format!(
                    "{no}번 노드: 입력 {src}번이 범위(0~{})를 벗어났습니다.",
                    total - 1
                ));
            }
            if src > i {
                return Err(format!(
                    "{no}번 노드: 자신보다 앞선 노드만 입력할 수 있습니다."
                ));
            }
            if !inputs.contains(&src) {
                inputs.push(src);
            }
        }
        if let Some(t) = r.temperature {
            if !t.is_finite() || !(0.0..=2.0).contains(&t) {
                return Err(format!("{no}번 노드: temperature는 0~2 사이여야 합니다."));
            }
        }
        if r.max_tokens == Some(0) {
            return Err(format!("{no}번 노드: max_tokens는 1 이상이어야 합니다."));
        }
        let title = r.title.trim();
        nodes.push(BlueprintNode {
            title: if title.is_empty() {
                format!("프롬프트 {no}")
            } else {
                cap(title, 60)
            },
            prompt: cap(&prompt, 8000),
            mode,
            inputs,
            system: cap(r.system.trim(), 4000),
            model: r
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| cap(s, 120)),
            temperature: r.temperature,
            max_tokens: r.max_tokens,
        });
    }
    Ok(BlueprintPlan { nodes })
}

/// Cuts from the first `{` to the last `}` in the response. Tolerates fences and surrounding prose.
fn extract_json(text: &str) -> Result<&str, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "JSON을 찾지 못했습니다. 다시 생성해 보세요.".to_string())?;
    let end = text
        .rfind('}')
        .ok_or_else(|| "JSON을 찾지 못했습니다. 다시 생성해 보세요.".to_string())?;
    if end < start {
        return Err("JSON을 찾지 못했습니다. 다시 생성해 보세요.".to_string());
    }
    Ok(&text[start..=end])
}

/// Truncates by character count (characters, not bytes).
fn cap(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_CHAIN: &str = r#"{"nodes":[
        {"title":"요약","prompt":"핵심을 3줄로","mode":"summarize","inputs":[]},
        {"title":"번역","prompt":"영어로 번역","mode":"translate_en","inputs":[0],"temperature":0.3,"max_tokens":500}
    ]}"#;

    #[test]
    fn parses_chained_plan() {
        let plan = parse_plan(TWO_CHAIN).unwrap();
        assert_eq!(plan.nodes.len(), 2);
        assert_eq!(plan.nodes[0].mode, NodeMode::Summarize);
        assert_eq!(plan.nodes[1].mode, NodeMode::TranslateEn);
        assert_eq!(plan.nodes[1].inputs, vec![0]);
        assert_eq!(plan.nodes[1].temperature, Some(0.3));
        assert_eq!(plan.nodes[1].max_tokens, Some(500));
        assert!(plan.nodes[0].model.is_none());
    }

    #[test]
    fn strips_fences_and_prose() {
        let wrapped = format!("물론이죠!\n```json\n{TWO_CHAIN}\n```\n위 설계를 쓰세요.");
        let plan = parse_plan(&wrapped).unwrap();
        assert_eq!(plan.nodes.len(), 2);
    }

    #[test]
    fn rejects_unknown_mode() {
        let bad = r#"{"nodes":[{"title":"a","prompt":"p","mode":"teleport","inputs":[]}]}"#;
        let err = parse_plan(bad).unwrap_err();
        assert!(err.contains("teleport"), "모드명이 보여야 함: {err}");
    }

    #[test]
    fn rejects_bad_inputs() {
        // self-reference
        let me = r#"{"nodes":[{"title":"a","prompt":"p","inputs":[0]}]}"#;
        assert!(parse_plan(me).unwrap_err().contains("자기 자신"));
        // out of range
        let oob = r#"{"nodes":[{"title":"a","prompt":"p","inputs":[7]}]}"#;
        assert!(parse_plan(oob).unwrap_err().contains("범위"));
        // not an earlier node (node 0 points at node 1)
        let fwd = r#"{"nodes":[
            {"title":"a","prompt":"p","inputs":[1]},
            {"title":"b","prompt":"q","inputs":[]}
        ]}"#;
        assert!(parse_plan(fwd).unwrap_err().contains("앞선 노드"));
    }

    #[test]
    fn rejects_empty_and_too_many() {
        assert!(parse_plan(r#"{"nodes":[]}"#).is_err());
        let many: Vec<String> = (0..MAX_BLUEPRINT_NODES + 1)
            .map(|i| format!(r#"{{"title":"t{i}","prompt":"p","inputs":[]}}"#))
            .collect();
        let err = parse_plan(&format!(r#"{{"nodes":[{}]}}"#, many.join(","))).unwrap_err();
        assert!(err.contains("최대"), "{err}");
    }

    #[test]
    fn truncates_long_fields() {
        let long = "가".repeat(100);
        let json = format!(r#"{{"nodes":[{{"title":"{long}","prompt":"p","inputs":[]}}]}}"#);
        let plan = parse_plan(&json).unwrap();
        assert_eq!(plan.nodes[0].title.chars().count(), 60);
    }

    #[test]
    fn mode_aliases_parse() {
        assert_eq!(parse_mode_key("translateen"), Some(NodeMode::TranslateEn));
        assert_eq!(
            parse_mode_key(" translate_ko "),
            Some(NodeMode::TranslateKo)
        );
        assert_eq!(parse_mode_key("PROSCONS"), Some(NodeMode::ProsCons));
        assert_eq!(parse_mode_key("없음"), None);
    }

    #[test]
    fn system_prompt_lists_all_modes() {
        let sys = plan_system_prompt();
        for mode in NodeMode::ALL {
            let key = MODE_KEYS
                .iter()
                .find(|(_, m)| *m == mode)
                .map(|(k, _)| *k)
                .unwrap_or_else(|| panic!("{mode:?} 키 누락"));
            assert!(sys.contains(key), "{key} 누락");
        }
    }
}
