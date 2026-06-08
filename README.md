# KeyZen

KeyZen is a fast, low-resource, predictable keyboard remapper for Windows.

The Rust runtime captures keyboard input with a user-mode low-level keyboard hook, resolves mappings through a stack-based layer engine, and emits remapped input with `SendInput`. It can run from the system tray without a console window or in a foreground console for development and diagnostics.

## Features

- Stack-based layers with top-down fallback to `base`
- Layer while-held and toggle actions
- Tap-hold keys
- Tap dance keys
- One-shot layers and one-shot modifiers
- YAML configuration
- Native Windows tray menu and pause control
- Start at login through Windows Task Scheduler

## Usage

Build both executables:

```powershell
cargo build --workspace
```

Launch the background tray app:

```powershell
target\debug\keyzen.exe
target\debug\keyzen.exe tray
```

`keyzen.exe` launches the sibling `keyzen-tray.exe` and exits. `keyzen-tray.exe` has no console window and owns the tray menu and remapping runtime.

Validate a configuration:

```powershell
cargo run --bin keyzen -- validate --config examples/keyzen.yaml
```

Print the normalized configuration:

```powershell
cargo run --bin keyzen -- dump --config examples/keyzen.yaml
```

Run KeyZen in the foreground:

```powershell
cargo run --bin keyzen -- --config examples/keyzen.yaml
```

Press `Ctrl+C` to stop.

See [docs/configuration.md](docs/configuration.md) for the key mapping configuration guide and [docs/application-settings.md](docs/application-settings.md) for tray and startup settings.

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

## Tray Menu

The tray menu labels are:

```text
Pause
---
Start at login
Choose key config...
---
Quit
```

On first launch, KeyZen creates `%APPDATA%\KeyZen\settings.yaml` and `%APPDATA%\KeyZen\keyzen.yaml`. The default key config contains an empty `base` layer. An invalid or missing selected key config starts the tray in a paused state so it can be repaired with `Choose key config...`.

## Notes

KeyZen is not a driver, service, or installer. It is intended for normal user-session applications. To remap keys inside elevated applications, run KeyZen elevated as well.
