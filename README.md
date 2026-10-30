## 🚀 Native Multi-Prompt AI Canvas

    A lightweight Rust-native no-code canvas for running and comparing 10 AI prompts simultaneously without lag.

## 🎥 1. Demo & Visuals

    ▶️ Watch the 2-min Demo Video (YouTube/Loom link)

    📦 Download the Binary (GitHub Releases) (Runs instantly without building)

(▲ 10 prompt nodes simultaneously invoking Meta Llama models and rendering real-time streaming results)
## 💡 2. Problem & Solution
<img width="2369" height="1278" alt="Tailored Prompt Optimization Using Chain-of-Thought Reasoning" src="https://github.com/user-attachments/assets/3ea85b44-b671-4749-9177-44a46160c0ea" />

    Problem:

    When engineering prompts and testing agent workflows, editing prompts one by one in a single chat window and comparing results is extremely inefficient. Existing web or Electron-based multi-execution tools consume heavy system resources, leading to UI frame drops when multiple streams occur concurrently.

    Solution:

    We built a native desktop application that batch-runs up to 10 prompt pipelines concurrently on a visual canvas—no coding required. By leveraging Rust's powerful asynchronous processing capabilities, it delivers instant responsiveness and a smooth UX without a heavy browser engine.

## ✨ 3. Key Features

<img width="2366" height="1253" alt="00  Multi-Prompting, Prompt Chaining" src="https://github.com/user-attachments/assets/51a661da-4717-499e-b0dd-b9fbd530cce9" />


    ⚡ 10x Parallel Prompt Orchestration: Simultaneously invoke and compare 10 different prompt/parameter combinations asynchronously with a single click, featuring real-time streaming.

    🎨 No-Code Visual Canvas: An intuitive node-based UI designed to build complex prompt chaining and A/B testing effortlessly.

    🦀 Ultra-Lightweight Native Performance: Implemented as a Rust-native GUI, minimizing memory footprint during idle and multi-run states while ensuring a zero-latency UI.

    🦙 Seamless Meta AI Integration: Optimized prompt templates for Meta Llama series endpoints along with a real-time token/speed metrics dashboard.

    🗂️ 3 Batch Execution Modes: Supports Parallel, Sequential, and Chain (topological order along input links) execution modes.

    🤖 Agent & MCP Tool Integration: Equipped with agent nodes that interact with stdio-based MCP servers and automated blueprint generation.

---

## 🏗️ 4. Architecture & Tech Stack

**Stack:** Rust · egui/eframe (native GUI) · reqwest blocking HTTP · MCP stdio (child process + JSON-RPC) · serde JSON persistence

```mermaid
graph LR
    A[egui Native GUI Canvas] -->|spawn_job per node| B(std::thread workers)
    B -->|blocking HTTPS| C[Meta Muse API<br/>chat/completions]
    B <-->|JSON-RPC stdio| D[MCP server subprocess]
    B -->|mpsc JobResult| A
    A -->|autosave JSON| E[(Local disk)]
```

* Execution model: one OS thread per node, results collected over `mpsc` channels. No Tokio or SSE streaming — results arrive as completed outputs (agent progress lines do show live).
* Chain mode wakes nodes in topological order once their inputs finish; cyclic links are blocked at creation.
* LLM integration: Meta Muse API (`muse-spark-1.3`, OpenAI-compatible `chat/completions`) + a Mock provider that works offline.
* Image generation: pollinations.ai; speech generation: Google Translate TTS (public HTTP).

---

## 🚀 5. Getting Started

1. Install Rust: https://www.rust-lang.org/tools/install
2. Run from this folder:

```sh
cargo run --release
```

3. Top bar `Settings` → `AI` tab → provider `Meta Muse API`, then enter a key:
   * `Session key` in the settings window (memory only, gone on exit), or
   * the `MODEL_API_KEY` environment variable.
4. Use `Test connection` to verify the key, model, and endpoint.
5. To explore without a key, select the `Mock` provider (generates demo output).

Defaults: endpoint `https://api.meta.ai/v1/chat/completions`, model `muse-spark-1.3`.

---

## 🖱️ 6. Usage

* `＋ Add node`: a new card appears on the canvas. Drag it by the title bar.
* `✨ Blueprint`: describe the setup you want in a paragraph and nodes are created and wired up.
* Drag empty space: pan the canvas. Manage the viewport with grid layout, fit-to-view, and the minimap.
* Each node: edit title, `ON/OFF`, run-mode picker, prompt input, `▶ Run`, clone/delete, copy/enlarge output.
* Links: drag from an output port, use the card's `→ Link` button, or `L` (link selection). Hover a link for `×` to unlink.
* Left menu: `▶ Run all ON`, `▶ Run selected`, clear outputs, cancel, link-status chips.

---

## 📁 7. Project Structure

```text
src/
├── main.rs       app entry point, icon
├── app.rs        update orchestration, shortcuts, save hooks
├── state.rs      app state, undo, node ops, view helpers
├── run.rs        background jobs (LLM, agent, MCP, batch)
├── node.rs       prompt node data, links, topo sort (pure logic)
├── cards.rs      canvas, node cards, ports, minimap rendering
├── panels.rs     settings, info, tool windows
├── llm.rs        LLM abstraction (Mock + Meta Muse API)
├── agent.rs      agent loop (MCP tool calls)
├── mcp.rs        MCP stdio client
├── blueprint.rs  paragraph description -> node design & creation
├── schedule.rs   start & per-node schedules
├── alarm.rs      Telegram, Discord, WhatsApp alarms
├── share.rs      email, SNS, Markdown sharing
├── media.rs      image & audio generation
├── store.rs      autosave & restore (JSON)
├── theme.rs      34 design themes
├── i18n.rs       KO/EN UI strings
├── zoom.rs       canvas wheel-zoom math
├── fonts.rs      OS system Korean font registration
├── http.rs       blocking HTTP client cache
└── usability.rs  randomized-scenario invariant checks (10k cases)
```

---

## 🛠️ 8. Development

```sh
cargo c             # fast check (alias)
cargo t             # unit tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --profile fast   # optimization measurement (release opts, no LTO)
```

To avoid OneDrive sync contention, build output is redirected to
`C:/_cargo-target/prompt_canvas_gui` via `.cargo/config.toml`.

Release builds ship with LTO + symbol stripping + hidden Windows console.

---

## 📄 9. License

MIT — see [LICENSE](LICENSE).
