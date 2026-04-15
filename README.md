<p align="center">
  <a href="https://vibekanban.com">
    <picture>
      <source srcset="packages/public/vibe-kanban-logo-dark.svg" media="(prefers-color-scheme: dark)">
      <source srcset="packages/public/vibe-kanban-logo.svg" media="(prefers-color-scheme: light)">
      <img src="packages/public/vibe-kanban-logo.svg" alt="Vibe Kanban Logo" width="200">
    </picture>
  </a>
</p>

<h1 align="center">Vibe Kanban — Local-First AI Agent Orchestrator</h1>

<p align="center">
  Plan tasks on a kanban board, run them with AI agents directly in your local repositories.
</p>

<p align="center">
  <a href="#-getting-started">Getting Started</a> ·
  <a href="#-features">Features</a> ·
  <a href="#-supported-agents">Supported Agents</a> ·
  <a href="#-development">Development</a> ·
  <a href="#-contributing">Contributing</a>
</p>

---

## What is it?

**Vibe Kanban** is a local-first orchestrator that connects your kanban workflow to AI coding agents. You plan tasks visually, then hand them off to agents that run directly in your local repository — no cloud project setup required.

The key insight: the biggest productivity bottleneck for AI-assisted development isn't code generation speed — it's the planning → review → iterate loop. Vibe Kanban shortens that loop.

## ✨ Features

- **Local directories as projects** — point at any folder on your machine and it instantly becomes a kanban project.
- **9+ AI executors** — switch between Claude Code, Gemini CLI, Codex, GitHub Copilot, Cursor, Qwen Code, Amp, OpenCode, Droid and more.
- **Executor Availability panel** — Settings → Agents → Доступность shows which agents are installed, lets you enable/disable them, and provides one-click install instructions for missing ones.
- **Custom API support** — plug in OpenRouter, local Ollama, or any OpenAI-compatible endpoint to minimise costs.
- **Global kanban board** — aggregate tasks across all your local projects in one view.
- **PM-Orchestrator mode** — a managing agent picks up your local `@skills`, breaks work into subtasks, and delegates to other agents automatically.
- **Git-native isolation** — every agent runs in a dedicated git worktree (`vk/…`), keeping your main branch untouched until you approve.
- **Relay & remote hosts** — optionally expose a host over WebRTC relay so teammates can connect to your machine's agents.

## 🤖 Supported Agents

| Agent | Install |
|-------|---------|
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code/getting-started) | `npm install -g @anthropic-ai/claude-code` |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli) | `npm install -g @google/gemini-cli` |
| [Codex](https://github.com/openai/codex) | `npm install -g @openai/codex` |
| [OpenCode](https://opencode.ai) | `npm install -g opencode-ai` |
| [Qwen Code](https://github.com/QwenLM/qwen-code) | `npm install -g qwen-code` |
| [GitHub Copilot](https://github.com/features/copilot) | `npm install -g @github/copilot-language-server` |
| [Cursor](https://cursor.com) | Download from cursor.com |
| [Amp](https://ampcode.com) | Sign up at ampcode.com |
| [Droid](https://factory.ai) | Sign up at factory.ai |

> The **Settings → Agents → Доступность** tab automatically detects which of these are installed on your machine.

## 🚀 Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) ≥ 20
- [pnpm](https://pnpm.io/) ≥ 8

### Run locally

```bash
git clone https://github.com/grisha123invent-official/vibe-kanban.git
cd vibe-kanban
pnpm install
pnpm run dev
```

The dashboard opens at `http://localhost:3000` (or the next available port). Add a local folder as your first project and start creating tasks.

## 🛠 Development

### Project structure

```
crates/          Rust workspace — server, db, executors, services, utils, git …
packages/
  local-web/     Local app entrypoint (Vite + React + Tailwind)
  remote-web/    Remote deployment frontend
  web-core/      Shared React component library
shared/          Generated TypeScript types (do not edit manually)
docs/            Documentation sources
scripts/         Dev helpers
```

### Useful commands

| Command | Description |
|---------|-------------|
| `pnpm run dev` | Start frontend + backend with auto-assigned ports |
| `pnpm run check` | TypeScript + Rust type checks |
| `pnpm run lint` | ESLint + Clippy |
| `pnpm run format` | Prettier + rustfmt |
| `cargo test --workspace` | Run all Rust unit tests |
| `pnpm run generate-types` | Regenerate `shared/types.ts` from Rust structs |
| `pnpm run backend:check` | Cargo check for all backend crates |

### Build frontend only

```bash
cd packages/local-web
pnpm run build
```

## 🤝 Contributing

We prefer to discuss new ideas and changes before opening pull requests — this helps align implementation with the current roadmap.

Open an issue to start a conversation, then send a PR once the approach is agreed.

## 🐛 Support

For bug reports and feature requests, please [open an issue](https://github.com/grisha123invent-official/vibe-kanban/issues) in this repository.

## 📄 License

[MIT](LICENSE)
