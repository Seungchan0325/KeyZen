use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use thiserror::Error;

use crate::key::{Key, KeyParseError, Modifier};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub process_unmapped_keys: bool,
    pub startup_layer: String,
    pub source_keys: HashSet<Key>,
    pub layers: HashMap<String, HashMap<Key, Action>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Key(Key),
    Chord { modifiers: Vec<Modifier>, key: Key },
    LayerWhileHeld(String),
    LayerSwitch(String),
    Transparent,
    Noop,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    Key(#[from] KeyParseError),
    #[error("startup layer `{0}` does not exist")]
    MissingStartupLayer(String),
    #[error("layer `{0}` referenced by `{1}` does not exist")]
    MissingLayerReference(String, String),
    #[error("chord must contain at least one modifier and exactly one output key")]
    InvalidChord,
    #[error("recursive alias reference: {0}")]
    RecursiveAlias(String),
    #[error("unsupported action string `{0}`")]
    UnsupportedActionString(String),
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default)]
    settings: RawSettings,
    source: RawSource,
    #[serde(default)]
    aliases: HashMap<String, RawAction>,
    layers: HashMap<String, HashMap<String, RawAction>>,
}

#[derive(Debug, Default, Deserialize)]
struct RawSettings {
    #[serde(default)]
    process_unmapped_keys: bool,
    #[serde(default = "default_startup_layer")]
    startup_layer: String,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawAction {
    Shorthand(String),
    Explicit {
        key: Option<String>,
        chord: Option<Vec<String>>,
        layer_while_held: Option<String>,
        layer_switch: Option<String>,
    },
}

impl RuntimeConfig {
    pub fn parse(input: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(input)?;
        let startup_layer = raw.settings.startup_layer;
        if !raw.layers.contains_key(&startup_layer) {
            return Err(ConfigError::MissingStartupLayer(startup_layer));
        }

        let source_keys = raw
            .source
            .keys
            .iter()
            .map(|name| Key::from_name(name))
            .collect::<Result<HashSet<_>, _>>()?;

        let mut layers = HashMap::new();
        for (layer_name, raw_layer) in raw.layers {
            let mut layer = HashMap::new();
            for (input_name, raw_action) in raw_layer {
                let input_key = Key::from_name(&input_name)?;
                let action = parse_action(&raw_action, &raw.aliases)?;
                layer.insert(input_key, action);
            }
            layers.insert(layer_name, layer);
        }

        let config = Self {
            process_unmapped_keys: raw.settings.process_unmapped_keys,
            startup_layer,
            source_keys,
            layers,
        };
        config.validate_layer_references()?;
        Ok(config)
    }

    fn validate_layer_references(&self) -> Result<(), ConfigError> {
        for (owner, layer) in &self.layers {
            for action in layer.values() {
                let target = match action {
                    Action::LayerWhileHeld(target) | Action::LayerSwitch(target) => target,
                    _ => continue,
                };
                if !self.layers.contains_key(target) {
                    return Err(ConfigError::MissingLayerReference(
                        target.clone(),
                        owner.clone(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn parse_action(
    raw: &RawAction,
    aliases: &HashMap<String, RawAction>,
) -> Result<Action, ConfigError> {
    parse_action_inner(raw, aliases, &mut Vec::new())
}

fn parse_action_inner(
    raw: &RawAction,
    aliases: &HashMap<String, RawAction>,
    alias_stack: &mut Vec<String>,
) -> Result<Action, ConfigError> {
    match raw {
        RawAction::Shorthand(value) => parse_action_string(value, aliases, alias_stack),
        RawAction::Explicit {
            key,
            chord,
            layer_while_held,
            layer_switch,
        } => {
            let specified = [
                key.is_some(),
                chord.is_some(),
                layer_while_held.is_some(),
                layer_switch.is_some(),
            ]
            .into_iter()
            .filter(|is_some| *is_some)
            .count();

            if specified != 1 {
                return Err(ConfigError::UnsupportedActionString(
                    "explicit action must contain exactly one field".to_owned(),
                ));
            }

            if let Some(key) = key {
                Ok(Action::Key(Key::from_name(&key)?))
            } else if let Some(chord) = chord {
                parse_chord_parts(&chord)
            } else if let Some(layer) = layer_while_held {
                Ok(Action::LayerWhileHeld(layer.clone()))
            } else if let Some(layer) = layer_switch {
                Ok(Action::LayerSwitch(layer.clone()))
            } else {
                unreachable!("specified count checked above")
            }
        }
    }
}

fn parse_action_string(
    value: &str,
    aliases: &HashMap<String, RawAction>,
    alias_stack: &mut Vec<String>,
) -> Result<Action, ConfigError> {
    match value {
        "transparent" => Ok(Action::Transparent),
        "noop" => Ok(Action::Noop),
        _ if value.contains('+') => {
            let parts = value.split('+').map(str::trim).collect::<Vec<_>>();
            parse_chord_parts(&parts)
        }
        _ => match Key::from_name(value) {
            Ok(key) => Ok(Action::Key(key)),
            Err(error) => {
                if let Some(alias) = aliases.get(value) {
                    resolve_alias(value, alias, aliases, alias_stack)
                } else {
                    Err(error.into())
                }
            }
        },
    }
}

fn resolve_alias(
    name: &str,
    raw: &RawAction,
    aliases: &HashMap<String, RawAction>,
    alias_stack: &mut Vec<String>,
) -> Result<Action, ConfigError> {
    if alias_stack.iter().any(|alias| alias == name) {
        alias_stack.push(name.to_owned());
        return Err(ConfigError::RecursiveAlias(alias_stack.join(" -> ")));
    }

    alias_stack.push(name.to_owned());
    let action = parse_action_inner(raw, aliases, alias_stack);
    alias_stack.pop();
    action
}

fn parse_chord_parts<S: AsRef<str>>(parts: &[S]) -> Result<Action, ConfigError> {
    let (key_name, modifier_names) = parts.split_last().ok_or(ConfigError::InvalidChord)?;
    if modifier_names.is_empty() {
        return Err(ConfigError::InvalidChord);
    }

    let key = Key::from_name(key_name.as_ref())?;
    let modifiers = modifier_names
        .iter()
        .map(|part| Modifier::from_name(part.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Action::Chord { modifiers, key })
}

fn default_startup_layer() -> String {
    "base".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
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
    "#;

    #[test]
    fn parses_sample_config() {
        let config = RuntimeConfig::parse(SAMPLE).unwrap();
        assert_eq!(config.startup_layer, "base");
        assert!(config.source_keys.contains(&Key::CapsLock));
        assert_eq!(
            config.layers["nav"].get(&Key::H),
            Some(&Action::Key(Key::Left))
        );
    }

    #[test]
    fn parses_chord_shorthand() {
        let action =
            parse_action_string("Ctrl+Alt+Delete", &HashMap::new(), &mut Vec::new()).unwrap();
        assert_eq!(
            action,
            Action::Chord {
                modifiers: vec![Modifier::Ctrl, Modifier::Alt],
                key: Key::Delete
            }
        );
    }

    #[test]
    fn rejects_missing_layer_reference() {
        let input = r#"
            [settings]
            startup_layer = "base"
            [source]
            keys = ["CapsLock"]
            [layers.base]
            CapsLock = { layer_while_held = "missing" }
        "#;
        let error = RuntimeConfig::parse(input).unwrap_err();
        assert!(matches!(error, ConfigError::MissingLayerReference(_, _)));
    }

    #[test]
    fn parses_action_aliases() {
        let input = r#"
            [settings]
            startup_layer = "base"

            [source]
            keys = ["CapsLock", "H", "Space"]

            [aliases]
            nav_hold = { layer_while_held = "nav" }
            prev_word = "Ctrl+Left"
            disabled = "noop"

            [layers.base]
            CapsLock = "nav_hold"
            H = "prev_word"
            Space = "disabled"

            [layers.nav]
            H = "Left"
        "#;

        let config = RuntimeConfig::parse(input).unwrap();
        assert_eq!(
            config.layers["base"].get(&Key::CapsLock),
            Some(&Action::LayerWhileHeld("nav".to_owned()))
        );
        assert_eq!(
            config.layers["base"].get(&Key::H),
            Some(&Action::Chord {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Left,
            })
        );
        assert_eq!(config.layers["base"].get(&Key::Space), Some(&Action::Noop));
    }

    #[test]
    fn parses_vim_example() {
        let config = RuntimeConfig::parse(include_str!("../../../examples/vim.toml")).unwrap();
        assert_eq!(
            config.layers["vim"].get(&Key::B),
            Some(&Action::Chord {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Left,
            })
        );
        assert_eq!(
            config.layers["vim"].get(&Key::M),
            Some(&Action::Chord {
                modifiers: vec![Modifier::Ctrl, Modifier::Shift],
                key: Key::Right,
            })
        );
    }

    #[test]
    fn rejects_recursive_aliases() {
        let input = r#"
            [settings]
            startup_layer = "base"

            [source]
            keys = ["H"]

            [aliases]
            first = "second"
            second = "first"

            [layers.base]
            H = "first"
        "#;

        let error = RuntimeConfig::parse(input).unwrap_err();
        assert!(matches!(error, ConfigError::RecursiveAlias(_)));
    }
}
