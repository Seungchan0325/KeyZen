# AGENTS.md

## Project

KeyZen is a Windows-first Rust keyboard layer engine. It prioritizes low input latency, low resource usage, fast startup, and explicit input rules. It is not a macro runner or general automation tool.

## Repository Layout

- `crates/keyzen-core`: Pure input engine, config parsing, key normalization, layer state, and unit tests.
- `crates/keyzen-win`: Windows-specific hook, output, tray, and startup integration.
- `crates/keyzen`: Application entrypoint.
- `examples/keyzen.toml`: Default/example user configuration.

## Development Commands

Run these from the repository root:

```powershell
cargo fmt --all --check
cargo test --workspace
```

Use this for a faster compile-only pass:

```powershell
cargo check --workspace
```

Run the Windows app manually with:

```powershell
cargo run -p keyzen
```

Be careful when running the app: it installs a low-level keyboard hook and begins processing configured keys immediately.

## Implementation Rules

- Keep `keyzen-core` platform-independent and side-effect free.
- Put all Win32 API usage in `keyzen-win`.
- Do not add macro, shell command, delayed automation, mouse automation, tap-hold, tap-dance, or one-shot behavior to the MVP unless the product scope changes.
- Prefer explicit config validation errors over silent fallback.
- Avoid adding latency-sensitive behavior to the hook callback. Do minimal work there and keep blocking operations out of the input path.
- Preserve injected-event filtering so KeyZen does not reprocess its own synthetic output.

## Testing Expectations

- Add or update `keyzen-core` unit tests for config syntax, layer resolution, action behavior, and key parsing changes.
- For Windows integration changes, run `cargo check --workspace` at minimum.
- Manual QA should cover tray commands, config reload success/failure, pause/resume, startup toggle, and stuck modifier behavior.
