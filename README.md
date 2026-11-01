# 🚀 Symphony Canvas — Native Multi-Prompt AI Canvas

[![Rust](https://img.shields.io/badge/Built_with-Rust-dca282.svg?logo=rust)](https://www.rust-lang.org/)
[![GUI](https://img.shields.io/badge/GUI-egui_%2F_eframe-007ec6.svg)](https://github.com/emilk/egui)
[![Model](https://img.shields.io/badge/Meta_Muse_API-muse--spark--1.3-0467df.svg)](https://ai.meta.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> **A lightweight, Rust-native no-code canvas for orchestrating, running, and comparing multi-step AI prompt pipelines simultaneously with zero UI lag.**

---

## 🎥 1. Demo & Visuals

▶️ **Watch the Demo Video:** [https://youtu.be/NDbVRu9LoJs](https://youtu.be/NDbVRu9LoJs)  

*(▲ 12 prompt nodes simultaneously invoking Meta `muse-spark-1.3` models with non-blocking live progress and concurrent rendering)*

---

## 💡 2. Problem & Solution

<img width="2365" height="1254" alt="Tailored Prompt Optimization Using Chain-of-Thought Reasoning" src="https://github.com/user-attachments/assets/fa8f55b0-90d9-4751-a37f-4a2f7e606c27" />

### The Problem
Traditional single-window chat interfaces force developers and creators into repetitive manual cycles—copying outputs between prompts, losing track of intermediate experiments, and struggling to orchestrate multi-step LLM pipelines. Existing visual workflow tools are typically bloated Electron/web wrappers that lag under load and lack deep native OS integration.

### The Solution
* **Zero-Lag Rust GUI:** Built entirely in Rust (`egui`/`eframe`) using multi-threaded OS background workers over `mpsc` channels—delivering a lightweight, responsive desktop UI that never freezes during heavy concurrent inference.
* **Topological Prompt Chaining:** Connect nodes intuitively with directed links. Downstream nodes automatically ingest upstream outputs with built-in cycle prevention and topological ordering.
* **3 Batch Orchestration Modes:** Execute workflows at the canvas level via `Parallel` (simultaneous multi-thread runs), `Sequential`, or `Chain` (topological dependency) execution.
* **AI Blueprint Generator:** Transforms a single natural language paragraph into an end-to-end multi-node canvas pipeline (up to 12 auto-wired and automatically laid-out nodes).
* **23 Node Run Modes & Agentic MCP:** Goes beyond standard text generation by combining specialized prompt presets, custom Python execution, multimodal media nodes, and autonomous MCP (Model Context Protocol stdio) agents capable of dynamic tool calling.
* **Real-World Notification Loop:** Native scheduling and automated completion alerts dispatched directly to Telegram, Discord, and WhatsApp.
* **Privacy & Security by Design:** Session API keys are held strictly in volatile memory (`RAM`) by default and are explicitly excluded from local JSON autosaves.

---

## ✨ 3. Key Features

<img width="2366" height="1253" alt="00 Multi-Prompting, Prompt Chaining" src="https://github.com/user-attachments/assets/51a661da-4717-499e-b0dd-b9fbd530cce9" />

* ⚡ **Parallel Prompt Orchestration:** Simultaneously invoke and compare up to 12 different prompt/parameter combinations asynchronously with a single click, complete with live agent progress updates.
* 🎨 **No-Code Visual Canvas:** An intuitive node-based UI designed to build complex prompt chains and A/B tests effortlessly, featuring **34 built-in design themes** and full **KO/EN bilingual UI** support.
* 🦀 **Ultra-Lightweight Native Performance:** Implemented as a pure Rust desktop application, minimizing memory footprint during both idle and multi-run states while maintaining a 60+ FPS zero-latency interface.
* 🦙 **Seamless Meta AI Integration:** Native support and optimized prompt templates for the **Meta Muse API (`muse-spark-1.3`)** and Llama-compatible endpoints, paired with a real-time token/latency metrics dashboard.
* 🤖 **Agent & MCP Tool Integration:** Equipped with autonomous agent nodes that communicate with local `stdio`-based MCP servers via JSON-RPC and support automated blueprint generation.
* 🛡️ **Battle-Tested Reliability:** Verified against **10,000 randomized scenario invariant checks** (`usability.rs`) to guarantee graph integrity, cycle prevention, and state consistency under rapid user interaction.

---

## 🚀 5. Getting Started
Prerequisites

    Install the Rust toolchain: https://www.rust-lang.org/tools/install

Installation & Run

    Clone the repository and build in release mode:

Bash

git clone [https://github.com/lightgo-la/symphony-meta2026-agent.git](https://github.com/lightgo-la/symphony-meta2026-agent.git)
cd symphony-meta2026-agent
cargo run --release

API Configuration

    In the top bar, navigate to Settings → AI tab, select provider Meta Muse API, and configure your credential using one of two methods:

        Session Key (Recommended): Enter your key directly in the settings window (stored in volatile memory only; cleared on exit).

        Environment Variable: Set MODEL_API_KEY in your OS environment before launching.

    Click Test connection to verify your API key, model (muse-spark-1.3), and endpoint (https://api.meta.ai/v1/chat/completions).

    No API Key? No Problem: Select the Mock provider to explore the canvas, test topological chaining, and simulate multi-node execution offline.

## 🖱️ 6. Usage & Node Modes
Canvas Controls

    ＋ Add node: Spawns a new prompt card on the canvas. Drag cards freely by their title bar.

    ✨ Blueprint: Describe your desired workflow in a single natural language paragraph to auto-generate and wire up a complete multi-node pipeline.

    Viewport Navigation: Drag empty space to pan, use the mouse wheel to zoom, and organize workspaces instantly with grid layout, fit-to-view, and the interactive minimap.

    Node Operations: Edit node titles, toggle ON/OFF, select from 23 node run modes, input prompts, click ▶ Run, clone/delete cards, or copy/enlarge generated outputs.

    Wiring Links: Drag from an output port to an input port, use the card's → Link button, or press L to link selected nodes. Hover over any link and click × to disconnect.

    Batch Controls (Left Panel): Trigger ▶ Run all ON, ▶ Run selected, clear outputs, cancel active worker threads, or inspect real-time link-status chips.

Overview of Execution Modes

    3 Canvas Batch Modes: Parallel (all active nodes run concurrently), Sequential (ordered by node index), and Chain (topological data-flow execution).

    23 Node-Level Run Modes: Includes direct LLM inference, structured reasoning templates (Chain-of-Thought, Critique, Summarization, Translation, Coding, etc.), autonomous MCP Agent loops, Python script execution, and Multimodal generation (Image & Audio TTS).

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

🛠️ 8. Development & Quality Assurance

Run standard checks, unit tests, and the 10k-scenario invariant test suite:

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

Distributed under the MIT License. See LICENSE for full details.
