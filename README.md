# KeyZen

KeyZen is a fast, low-resource, predictable keyboard remapper for Windows.

The v1 runtime is a background CLI application written in Rust. It captures keyboard input with a user-mode low-level keyboard hook, resolves mappings through a stack-based layer engine, and emits remapped input with `SendInput`.

## Features

- Stack-based layers with top-down fallback to `base`
- Layer while-held and toggle actions
- Tap-hold keys
- Tap dance keys
- One-shot layers and one-shot modifiers
- YAML configuration

## Usage

Validate a configuration:

```powershell
cargo run -p keyzen-cli -- validate --config examples/keyzen.yaml
```

Print the normalized configuration:

```powershell
cargo run -p keyzen-cli -- dump --config examples/keyzen.yaml
```

Run KeyZen:

```powershell
cargo run -p keyzen-cli -- --config examples/keyzen.yaml
```

Press `Ctrl+C` to stop.

See [docs/configuration.md](docs/configuration.md) for the full YAML configuration guide.

## Example

```yaml
settings:
  tapping_term_ms: 200
  tap_dance_term_ms: 180
  one_shot_timeout_ms: 1000

layers:
  base:
    CapsLock:
      tap_hold:
        tap: Escape
        hold:
          layer_while_held: nav
    Quote:
      tap_dance:
        1: Quote
        2: DoubleQuote
    LeftShift:
      one_shot_modifier: Shift

  nav:
    H: Left
    J: Down
    K: Up
    L: Right
```

## Notes

KeyZen v1 is not a driver, service, installer, or tray application. It is intended for normal user-session applications. To remap keys inside elevated applications, run KeyZen elevated as well.
