# netools

Network diagnostics toolkit.

## Linux build: pure CLI

On Linux, including ARM Linux, the `netools` binary starts as a pure command-line tool instead of opening the Tauri/React desktop window.

```bash
cd src-tauri
cargo build --release
./target/release/netools --help
```

Examples:

```bash
netools ping example.com --count 4
netools tcp example.com 443 --timeout 1500 --json
netools scan 127.0.0.1 22,80,443,8000-8010 --concurrency 64
netools tcp-ping example.com 443 --count 5
netools dns example.com --type A --server 1.1.1.1 --server 8.8.8.8
netools http https://example.com --method GET --json
netools trace example.com --max-hops 20
```

Supported subcommands: `ping`, `tcp`, `scan`, `tcp-ping`, `dns`, `http`, `trace`.

## macOS / Windows desktop mode

Non-Linux builds still run the original Tauri desktop UI:

```bash
npm run tauri dev
npm run tauri build
```

## Development checks

```bash
npm run build
cd src-tauri
cargo test --test cli_args
cargo check
```
