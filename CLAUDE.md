# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A network-diagnostics app built on **Tauri 2** with a **React 19 + TypeScript** frontend (Vite) and a **Rust** backend. Non-Linux builds use the desktop GUI. **Linux builds (including ARM Linux) are pure CLI** via `src-tauri/src/cli.rs`; they do not open the Tauri window. Five tools: ping, tcp, dns, http, traceroute — exposed as Tauri commands for GUI builds and CLI subcommands for Linux.

## Commands

```bash
# Run the app on macOS/Windows (launches Vite on :1420, then the Tauri shell)
npm run tauri dev

# Linux / Linux ARM pure CLI build and help
cd src-tauri && cargo build --release
./target/release/netools --help

# Type-check + Vite build (no Tauri shell)
npm run build

# Full bundled app
npm run tauri build

# Rust-only checks (run from src-tauri/)
cargo check
cargo clippy
```

Vite dev port `1420` is fixed (`strictPort: true` in `vite.config.ts`); Tauri expects exactly that. The frontend watch ignores `src-tauri/**`, so Rust edits do not trigger Vite HMR — `tauri dev` recompiles them itself.

### Ping backend selection (runtime env var)

`ping` has two implementations chosen at runtime by `NETOOLS_PING_BACKEND`:

- **`subprocess`** (default) — shells out to the system `ping` binary, no privileges needed.
- **`raw`** — `surge-ping` ICMP sockets; **requires `sudo`** on macOS/Linux. Run as `sudo NETOOLS_PING_BACKEND=raw npm run tauri dev`.

The frontend shows which one is active by calling the `ping_backend` command.

## Architecture

### Tauri command surface

All Rust commands are registered in [src-tauri/src/lib.rs](src-tauri/src/lib.rs) in the `invoke_handler!` macro. Each `probe::*` module owns one tool. Tauri converts camelCase JS args to snake_case Rust params automatically (e.g. JS `timeoutMs` ↔ Rust `timeout_ms`).

Commands fall into two patterns:

1. **One-shot**: `tcp_probe`, `tcp_scan`, `dns_query`, `http_request` — invoke returns the full result.
2. **Streaming** (long-running, partial results matter): `ping_host`, `tcp_ping`, `traceroute_run` — backend `emit`s per-result events on the `AppHandle`; the panel uses `listen()` from `@tauri-apps/api/event` to accumulate them in state. The command's awaited return value is usually ignored (or for traceroute, is just `Ok(())` plus a final `trace-done` event).

Event names: `ping-result`, `tcp-ping-result`, `trace-hop`, `trace-done`. When adding a streaming command, follow the same pattern — emit one event per increment and keep the command awaitable for completion/error signaling.

### Per-probe notes

- **`probe/ping`** — Two backends (`subprocess.rs`, `raw.rs`) behind a `mod.rs` dispatcher gated on `NETOOLS_PING_BACKEND`. `subprocess.rs` parses `ping` output across macOS/Linux/Windows with `#[cfg(target_os = ...)]` blocks because flag names differ (`-W ms` vs `-W s` vs `-w ms`, `-c` vs `-n`).
- **`probe/tcp`** — `connect.rs` does TCP connect + timeout. `fingerprint.rs` runs an optional 3-step service detector (passive banner read → active HTTP HEAD on likely web ports → port-number guess). `tcp_scan` uses `futures::stream::buffer_unordered` with a hardcoded **256 max concurrency** clamp to avoid fd exhaustion.
- **`probe/dns`** — `hickory-resolver` 0.26. Queries multiple upstream servers **in parallel** with `future::join_all`. Empty `servers` list → builds a system resolver via `TokioResolver::builder_tokio()`. Per-server custom resolver via `NameServerConfig::udp_and_tcp(ip)` → `ResolverConfig::from_parts` → `Resolver::builder_with_config`. The hickory API changed across recent versions — keep using the 0.26 shapes documented in the header comment of [dns.rs](src-tauri/src/probe/dns.rs).
- **`probe/http`** — `reqwest` with rustls. Redirects are captured by `Policy::custom` writing into an `Arc<Mutex<Vec<String>>>` (closure captures, can't borrow a Vec). `tls_info(true)` exposes the peer cert DER, which `x509-parser` decodes for subject/issuer/SAN/validity. Body preview is capped at `BODY_PREVIEW_LIMIT = 4096` bytes.
- **`probe/traceroute`** — Spawns system `traceroute` / Windows `tracert` and **streams parsed hops by reading stdout line-by-line** with `tokio::io::BufReader::lines()`, emitting `trace-hop` per hop. Two parsers (`parse_unix_line`, `parse_windows_line`) selected by `#[cfg]`. Args also differ per OS (timeout in seconds vs ms; `-h` vs `-m`).

### Frontend layout

- `src/App.tsx` — sidebar nav, renders the selected panel; tools are just a `useState<Tool>` switch.
- `src/panels/*Panel.tsx` — one panel per tool, each is self-contained (own state, own `invoke` + `listen` calls). When adding a tool: add a new panel file, add it to the `TOOLS` array and the switch in `App.tsx`.
- `src/components/LatencyChart.tsx` — shared Recharts line chart for any RTT-over-sequence series; takes `LatencyPoint[]` with `rtt_ms: number | null` (null = failed probe, renders as a break in the line).
- `src/utils/ports.ts` — `parsePorts("80,443,8000-8010")` → sorted unique port list; used by `TcpPanel`.

TS is strict, with `noUnusedLocals` and `noUnusedParameters` on — the build will fail on dead identifiers.

### Cross-platform conventions

Anything that shells out (`ping`, `traceroute`) **must** branch on `#[cfg(target_os = ...)]` for arg shape and output format. Both modules already do this — copy that pattern for any new subprocess-based probe.

### Code style notes

- In-code comments throughout the Rust modules are in Chinese, mixing high-level rationale with API gotchas. Match that style when editing existing files; new files can use whichever language fits.
- Tauri capabilities are minimal — only `core:default` and `opener:default` ([capabilities/default.json](src-tauri/capabilities/default.json)). If you add a command that needs new Tauri permissions (e.g. shell, fs), update that file.
