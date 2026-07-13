# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repo is

Two unrelated things share one Cargo crate (`async-exercises`, Rust 2024 edition):

1. **Async-learning exercises** in `src/bin/Future*.rs` and `src/bin/block_on*.rs` — hand-rolled `Future`/`Poll`/`Waker` implementations and a minimal `block_on` executor, written pedagogically with heavy Chinese comments.
2. **A Word Web Add-in installer** in `src/main.rs` (default binary) and `src/bin/main_2.rs` — a Windows-only tool that downloads an Office add-in manifest, parses it, writes registry keys, and launches Word. `main.rs` uses the Developer sideload path + generates a `.docx`; `main_2.rs` uses a shared-folder trusted catalog (requires admin / UAC elevation, auto-relaunches itself via PowerShell `Start-Process -Verb RunAs`).

Each `.rs` under `src/bin/` plus `src/main.rs` is its own runnable binary. There is no shared library code — every file is standalone with its own `main`.

## Commands

```bash
# Build everything
cargo build

# Run a specific exercise/binary (each file in src/bin/ is a binary named after the file)
cargo run --bin Future
cargo run --bin block_on
cargo run --bin main_2          # the shared-catalog installer

# Run the default binary (src/main.rs — the Developer-sideload installer)
cargo run

# Release build (installer binaries are meant to be shipped as release exes)
cargo build --release
```

There are no tests, no lints configured, and no CI. `cargo test` will build but finds nothing. The crate has no `[lib]` target.

## Architecture notes that require reading multiple files

- **The async exercises are a learning sequence, not a library.** They reimplement `Future`/`Poll` from scratch in `Future.rs` (note: locally defined `Poll` with variant `Read` instead of `Ready` — a typo to be aware of), then progressively use the *real* `std::future::Future` in `Future2`–`Future4` and `block_on_1`. `block_on.rs` constructs a no-op `RawWaker`/`Waker` via `RawWakerVTable` and polls in a loop with `std::thread::yield_now()`. Do not "fix" the intentional simplifications (e.g. the noop waker, the spurious-wake re-check in `block_on_1.rs`) without understanding they are the lesson.

- **Two installer variants target two different Office add-in registration mechanisms**, and the code is largely duplicated between `main.rs` and `main_2.rs` (manifest download, XML parsing via `quick_xml`, `parse_first_text`/`parse_host`). They diverge in registration strategy:
  - `main.rs` → `HKCU\SOFTWARE\Microsoft\Office\16.0\Wef\Developer`, writes `manifest.xml` path as a value under the add-in Id, and builds a sideload `.docx` with an embedded `webextension` (docx is assembled as a zip via the `zip` crate).
  - `main_2.rs` → `HKCU\SOFTWARE\Microsoft\Office\16.0\WEF\TrustedCatalogs\<GUID>`, creates a Windows network share (`net share`) over `C:\Users\Public\XljOfficeAddinCatalog`, and must run elevated.

- **`main_2.rs` self-elevates.** `is_elevated()` probes via `net session`; if not elevated it re-spawns itself with `Start-Process -Verb RunAs` and exits. When modifying, preserve this pre-flight check before any `net share` / registry write.

- **Manifest parsing is hand-rolled** (`parse_first_text`, `parse_host`) rather than serde — it reads the first `<Id>`/`<Version>` text node and the `<Host Name="...">` attribute. Only `Host = Document` (Word) is accepted; anything else returns an error.

- **The manifest URL and add-in identity are hardcoded** (`https://www.xljsci.com/LTSCOfficeV2/manifest.xml`, share name `XljOfficeAddinCatalog`, catalog GUID `{a81ffdb3-...}`). These are domain-specific constants, not config.

## Platform

Windows only for the installer binaries (`winreg`, `net share`, UAC, registry paths under `Office\16.0`). The async exercises are platform-agnostic but the crate as a whole won't build on non-Windows because `winreg` is a non-optional dependency.
