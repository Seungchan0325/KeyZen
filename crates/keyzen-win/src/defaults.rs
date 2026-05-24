pub const DEFAULT_KEYMAP: &str = r#"[settings]
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
"#;

#[cfg(test)]
mod tests {
    use keyzen_core::RuntimeConfig;

    use super::*;

    #[test]
    fn default_keymap_is_valid() {
        RuntimeConfig::parse(DEFAULT_KEYMAP).unwrap();
    }
}
