# KeyZen

KeyZen is a Windows-first keyboard layer engine for reducing mouse use and hand movement.

The MVP is intentionally small: low latency, low resource use, fast startup, and clear rules. It is not an automation tool or a macro runner.

## MVP Features

- Windows low-level keyboard hook backend, no driver installation required.
- App configuration at the OS config directory, for example `%APPDATA%\KeyZen\config.toml` on Windows.
- Keymap configuration path stored in `config.toml`.
- Layer-based mappings with `layer_while_held`, `layer_switch`, `transparent`, `noop`, single-key output, and modifier chords.
- Tray app with Pause/Resume, Reload Config, Reload Keymap, Open Config Folder, Start at Login toggle, and Exit.
- Bundled official KeyZen icon for the Windows executable and tray icon.

## Known Windows MVP Limits

Because the MVP uses `LowLevelKeyboardHook`, some OS-reserved shortcuts such as `Win+L` may run before KeyZen can suppress them. This is a deliberate v1 tradeoff for a no-install first release.

KeyZen does not support text macros, shell commands, delayed automation, mouse automation, Unicode text insertion, tap-hold, tap-dance, or one-shot keys in the MVP.

## App Config

```toml
start_at_login = false
keymap_path = "C:\\Users\\you\\AppData\\Roaming\\KeyZen\\keyzen.toml"

[logging]
level = "info"
path = "C:\\Users\\you\\AppData\\Roaming\\KeyZen\\keyzen.log"
max_bytes = 1048576
max_files = 3
```

`start_at_login` is kept in sync with the current user's Windows startup registry entry.
`logging.level` accepts `error`, `warn`, `info`, `debug`, or `trace`. When the log file exceeds `logging.max_bytes`, KeyZen rotates it through `keyzen.log.1`, `keyzen.log.2`, and so on up to `logging.max_files`.

Log entries are written as readable single-line records:

```text
2026-05-25 14:32:10 [INFO ] keyzen_win::app - KeyZen keyboard hook installed
2026-05-25 14:33:04 [ERROR] keyzen_win::app - KeyZen keymap reload failed: failed to parse KeyZen config
```

## Example Keymap

```toml
[settings]
process_unmapped_keys = false
startup_layer = "base"

[source]
keys = ["CapsLock", "H", "J", "K", "L", "Space"]

[layers.base]
CapsLock = { layer_while_held = "nav" }
H = "H"
J = "J"
K = "K"
L = "L"
Space = "Space"

[layers.nav]
H = "Left"
J = "Down"
K = "Up"
L = "Right"
Space = "transparent"
```

자세한 작성법은 [Keymap 설정 파일 작성 가이드](docs/keymap-guide.ko.md)를 참고하세요.

## Development

```powershell
cargo test
cargo run -p keyzen
```

## Packaging

Create the local unsigned Windows package:

```powershell
./scripts/package.ps1
```

The script runs formatting checks, tests, a release build, zip packaging, release note generation, and SHA-256 checksum generation under `dist/`.
