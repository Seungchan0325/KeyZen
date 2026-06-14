# Changelog

## 1.0.0 - 2026-06-08

- Initial Windows release with foreground CLI and console-less tray runtime.
- YAML configuration for stack-based layers, `layer_while_held`, `layer_toggle`, tap-hold, tap dance, one-shot layer, one-shot modifier, and `noop`.
- Native Windows keyboard capture through `WH_KEYBOARD_LL` and output through `SendInput`.
- Tray pause, key config selection, and per-user Start at login through Windows Task Scheduler.

## 1.0.1 - 2026-06-11

- Fix Tray tooltip

## 1.0.2 - 2026-06-14

- Add persistent tray lifecycle and failure diagnostics.
