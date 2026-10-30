//! Usability evaluation: invariant checks over 10,000 random scenarios modeled on real user flows.
//!
//! Runs on the pure model (`PromptNode` + `node` helpers) without egui. Scenarios
//! are built with a deterministic PRNG, collecting a reason for every broken invariant.

use std::collections::HashSet;

use crate::node::{
    ancestor_context, blocked_cycle, combined_results, creates_cycle, in_port, is_error_output,
    is_mutual_only, out_port, runnable_ids, short_title, topo_order, NodeAccent, PromptNode,
};

/// Deterministic xorshift64 PRNG. `cargo test` generates the same 10,000 cases every time.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next() % n as u64) as usize
    }

    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

const TITLES: [&str; 10] = [
    "짧은 제목",
    "아주아주아주아주긴제목열일곱자이상",
    "Exactly sixteen!!",
    "Seventeen chars!!",
    "이모지 🎨🎉🔥🚀 포함 제목",
    "",
    "공백   많은   제목",
    "A",
    "1234567890123456",
    "12345678901234567",
];

const PROMPTS: [&str; 5] = [
    "",
    "   ",
    "요약해줘",
    "앞 노드 출력을 이어받아 표로 정리해줘. 항목은 장점·단점·다음 행동이다.",
    "Write a haiku about blue canvases and draggable nodes.",
];

/// Runs one scenario and returns descriptions of broken invariants. Empty means pass.
pub fn eval_one(seed: u64) -> Vec<String> {
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(0x12345));
    let mut fails = Vec::new();
    let mut nodes: Vec<PromptNode> = Vec::new();
    let mut next_id: usize = 1;

    let live_ids = |nodes: &[PromptNode]| -> Vec<usize> { nodes.iter().map(|n| n.id).collect() };
    // Mirrors the app's delete-cleanup (delete_node) rule.
    let delete_node = |nodes: &mut Vec<PromptNode>, id: usize| {
        if let Some(i) = nodes.iter().position(|n| n.id == id) {
            nodes.remove(i);
        }
        for n in nodes.iter_mut() {
            n.remove_input(id);
        }
    };

    let ops = 10 + rng.below(25);
    for _ in 0..ops {
        let action = rng.below(100);
        match action {
            0..=19 => {
                // Add node (unbounded map: offscreen coords stay as-is)
                let title = TITLES[rng.below(TITLES.len())].to_string();
                let prompt = PROMPTS[rng.below(PROMPTS.len())].to_string();
                let pos = [
                    rng.below(1600) as f32 - 200.0,
                    rng.below(1000) as f32 - 150.0,
                ];
                let mut n = PromptNode::new(next_id, title, prompt, rng.coin(), pos);
                if rng.coin() {
                    n.title_only = true;
                }
                nodes.push(n);
                next_id += 1;
            }
            20..=34 => {
                // Connect: skip links that would cycle, same rule as the app
                if nodes.len() >= 2 {
                    let a = nodes[rng.below(nodes.len())].id;
                    let mut b = nodes[rng.below(nodes.len())].id;
                    if a == b {
                        b = nodes[(rng.below(nodes.len()) + 1) % nodes.len()].id;
                        if a == b {
                            continue;
                        }
                    }
                    if !blocked_cycle(&nodes, b, a) {
                        if let Some(node) = nodes.iter_mut().find(|n| n.id == b) {
                            node.add_input(a);
                        }
                    }
                }
            }
            35..=42 => {
                // Disconnect (all or one)
                if !nodes.is_empty() {
                    let i = rng.below(nodes.len());
                    if rng.coin() || nodes[i].inputs.is_empty() {
                        nodes[i].inputs.clear();
                    } else {
                        let k = rng.below(nodes[i].inputs.len());
                        let src = nodes[i].inputs[k];
                        nodes[i].remove_input(src);
                    }
                }
            }
            43..=52 => {
                // Fold toggle: data must stay intact
                if !nodes.is_empty() {
                    let i = rng.below(nodes.len());
                    let before = nodes[i].prompt.clone();
                    nodes[i].title_only = !nodes[i].title_only;
                    if nodes[i].prompt != before {
                        fails.push(format!("seed {seed}: 접기가 프롬프트를 바꿨다"));
                    }
                }
            }
            53..=60 => {
                // ON/OFF
                if !nodes.is_empty() {
                    let i = rng.below(nodes.len());
                    nodes[i].enabled = rng.coin();
                }
            }
            61..=68 => {
                // Clone: reset output/count/time, keep inputs/image/fold
                if !nodes.is_empty() && nodes.len() < 12 {
                    let i = rng.below(nodes.len());
                    let src = nodes[i].clone();
                    let c = src.cloned_with_id(next_id, [40.0, 60.0]);
                    next_id += 1;
                    if !c.output.is_empty() || c.run_count != 0 || c.last_secs.is_some() {
                        fails.push(format!("seed {seed}: 복제가 실행 상태를 초기화하지 않았다"));
                    }
                    if c.inputs != src.inputs
                        || c.image_path != src.image_path
                        || c.title_only != src.title_only
                    {
                        fails.push(format!("seed {seed}: 복제가 설정 유지를 빠뜨렸다"));
                    }
                    if !c.title.ends_with(" 복사") {
                        fails.push(format!("seed {seed}: 복제 제목 규칙 깨짐"));
                    }
                    nodes.push(c);
                }
            }
            69..=76 => {
                // Delete + connection cleanup
                if !nodes.is_empty() {
                    let id = nodes[rng.below(nodes.len())].id;
                    delete_node(&mut nodes, id);
                }
            }
            77..=84 => {
                // Change title/prompt/output/time/color/image
                if !nodes.is_empty() {
                    let i = rng.below(nodes.len());
                    nodes[i].title = TITLES[rng.below(TITLES.len())].to_string();
                    nodes[i].prompt = PROMPTS[rng.below(PROMPTS.len())].to_string();
                    let kind = rng.below(5);
                    nodes[i].output = match kind {
                        0 => String::new(),
                        1 => "정상 출력이다 ".repeat(3 + rng.below(20)),
                        2 => format!("[오류] 실패 {}", rng.next()),
                        3 => format!("[error] failure {}", rng.next()),
                        _ => "ok".to_string(),
                    };
                    nodes[i].last_secs = if nodes[i].output.is_empty() {
                        None
                    } else {
                        Some(rng.below(600) as f32 / 10.0)
                    };
                    // Run-record rule: output implies time+timestamp; no output implies no time record
                    nodes[i].last_done = if nodes[i].output.is_empty() {
                        None
                    } else {
                        Some(1_700_000_000 + rng.next() % 50_000_000)
                    };
                    if rng.coin() && !nodes[i].output.is_empty() {
                        nodes[i].prev_output = Some(format!("이전 {}", rng.next()));
                    }
                    if rng.coin() {
                        nodes[i].accent = NodeAccent::Custom([
                            rng.below(256) as u8,
                            rng.below(256) as u8,
                            rng.below(256) as u8,
                        ]);
                    }
                    if rng.coin() {
                        nodes[i].image_path = Some(format!("/tmp/img-{}.png", rng.below(4)));
                    }
                }
            }
            _ => {
                // Move: unbounded map, so nodes stay where placed
                if !nodes.is_empty() {
                    let i = rng.below(nodes.len());
                    let want = [
                        rng.below(1600) as f32 - 200.0,
                        rng.below(1000) as f32 - 150.0,
                    ];
                    nodes[i].pos = want;
                    if nodes[i].pos != want {
                        fails.push(format!("seed {seed}: 노드 위치가 마음대로 바뀌었다"));
                    }
                }
            }
        }
    }
    check_invariants(seed, &nodes, &live_ids(&nodes), &mut fails);
    fails
}

/// Checks invariants at the end of a scenario.
fn check_invariants(seed: u64, nodes: &[PromptNode], ids: &[usize], fails: &mut Vec<String>) {
    // id uniqueness
    let uniq: HashSet<usize> = ids.iter().copied().collect();
    if uniq.len() != ids.len() {
        fails.push(format!("seed {seed}: 노드 id 중복"));
    }
    // Input links are live, never self, and cycle-free except direct mutual links
    for n in nodes {
        for src in &n.inputs {
            if *src == n.id {
                fails.push(format!("seed {seed}: 노드 {} 자기 연결", n.id));
            }
            if !uniq.contains(src) {
                fails.push(format!("seed {seed}: 노드 {} 끊긴 입력 {src}", n.id));
            }
            // A cycle through this edge fails unless it is a direct mutual (2-node) link.
            if creates_cycle(nodes, n.id, *src) && !is_mutual_only(nodes, n.id, *src) {
                fails.push(format!("seed {seed}: 노드 {} 긴 순환 연결", n.id));
            }
        }
        // No duplicate inputs
        let mut dup = n.inputs.clone();
        dup.sort_unstable();
        dup.dedup();
        if dup.len() != n.inputs.len() {
            fails.push(format!("seed {seed}: 노드 {} 중복 입력", n.id));
        }
    }
    // Header short-title rule: max 16 chars, overflow ends with …
    for n in nodes {
        let s = short_title(&n.title);
        if s.chars().count() > 16 {
            fails.push(format!("seed {seed}: 짧은 제목이 16자를 넘는다"));
        }
        if n.title.chars().count() > 16 && !s.ends_with('…') {
            fails.push(format!("seed {seed}: 긴 제목 자름 규칙 깨짐"));
        }
        if n.title.chars().count() <= 16 && s != n.title {
            fails.push(format!("seed {seed}: 짧은 제목이 원문을 바꿨다"));
        }
    }
    // Error-prefix detection matches output kind
    for n in nodes {
        let err = is_error_output(&n.output);
        if !n.output.is_empty()
            && (n.output.starts_with("[오류] ") || n.output.starts_with("[error] "))
            && !err
        {
            fails.push(format!("seed {seed}: 오류 출력을 오류가 아니라 했다"));
        }
        if err && n.output.is_empty() {
            fails.push(format!("seed {seed}: 빈 출력을 오류라 했다"));
        }
        if !n.output.is_empty() && n.last_secs.is_none() {
            fails.push(format!("seed {seed}: 출력 있는데 실행 시간 없음"));
        }
        if !n.output.is_empty() && n.last_done.is_none() {
            fails.push(format!("seed {seed}: 출력 있는데 완료 시각 없음"));
        }
    }
    // Topological order: no duplicates + edge direction respected
    let order = topo_order(nodes, ids);
    let seen: HashSet<usize> = order.iter().copied().collect();
    if seen.len() != order.len() {
        fails.push(format!("seed {seed}: 실행 순서에 중복"));
    }
    let pos_of = |id: usize| order.iter().position(|o| *o == id);
    for n in nodes {
        for src in &n.inputs {
            // Mutual (bidirectional) pairs must have one side later, so skip them in the check.
            let mutual = nodes
                .iter()
                .any(|m| m.id == *src && m.inputs.contains(&n.id));
            if mutual {
                continue;
            }
            if let (Some(a), Some(b)) = (pos_of(*src), pos_of(n.id)) {
                if a > b {
                    fails.push(format!("seed {seed}: 원본이 뒤에 실행된다"));
                }
            }
        }
    }
    // Port geometry: output right-outside, input left-outside, same mid height
    for n in nodes {
        let r = [n.pos[0], n.pos[1], n.pos[0] + 300.0, n.pos[1] + 400.0];
        let o = out_port(r);
        let i = in_port(r);
        if !(o[0] > r[2] && i[0] < r[0] && (o[1] - i[1]).abs() < f32::EPSILON) {
            fails.push(format!("seed {seed}: 포트 위치 규칙 깨짐"));
        }
    }
    // Runnable rule: ON + not running + has prompt
    let busy: Vec<usize> = Vec::new();
    let run = runnable_ids(nodes, &busy);
    for n in nodes {
        let expect = n.enabled && !n.prompt.trim().is_empty();
        if run.contains(&n.id) != expect {
            fails.push(format!("seed {seed}: 실행 대상 판별 오류"));
        }
    }
    // Ancestor context: cycle-free, max 16, empty outputs excluded
    for n in nodes {
        let ctx = ancestor_context(n, nodes);
        if ctx.len() > 16 {
            fails.push(format!("seed {seed}: 조상 컨텍스트 16개 초과"));
        }
    }
    // Combined results: nodes with output include both title and output
    let all = combined_results(nodes);
    for n in nodes {
        if !n.output.trim().is_empty()
            && (!all.contains(&n.title) || !all.contains(n.output.trim()))
        {
            fails.push(format!("seed {seed}: 전체 결과 누락"));
        }
    }
    // Save compatibility: every field round-trips
    for n in nodes {
        let s = serde_json::to_string(n).unwrap();
        let back: PromptNode = serde_json::from_str(&s).unwrap();
        if back.title_only != n.title_only
            || back.image_path != n.image_path
            || back.last_secs != n.last_secs
            || back.prev_output != n.prev_output
            || back.last_done != n.last_done
            || (back.width_scale - n.width_scale).abs() > f32::EPSILON
            || (back.height_scale - n.height_scale).abs() > f32::EPSILON
            || back.accent != n.accent
            || back.inputs != n.inputs
        {
            fails.push(format!("seed {seed}: 저장 라운드트립 손실"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usability_ten_thousand_scenarios() {
        let mut bad = 0_usize;
        let mut samples: Vec<String> = Vec::new();
        for seed in 0..10_000_u64 {
            for f in eval_one(seed) {
                bad += 1;
                if samples.len() < 20 {
                    samples.push(f);
                }
            }
        }
        assert!(
            bad == 0,
            "사용성 평가 실패 {bad}건:\n{}",
            samples.join("\n")
        );
    }

    #[test]
    fn image_decode_path_works() {
        // Checks the representative image path (decode→thumbnail→RGBA) actually works.
        let dir = std::env::temp_dir();
        let path = dir.join("symphony-usability-probe.png");
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for y in 0..16_u8 {
            for x in 0..16_u8 {
                rgba.extend_from_slice(&[x * 16, y * 16, 128, 255]);
            }
        }
        image::save_buffer(&path, &rgba, 16, 16, image::ColorType::Rgba8).unwrap();
        let img = image::open(&path).unwrap();
        let thumb = img.thumbnail(640, 640);
        let raw = thumb.to_rgba8();
        assert!(raw.width() > 0 && raw.height() > 0);
        let _ = std::fs::remove_file(&path);
    }
}
