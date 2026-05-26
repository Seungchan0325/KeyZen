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
    Chord {
        modifiers: Vec<Modifier>,
        key: Key,
    },
    LayerWhileHeld(String),
    LayerSwitch(String),
    TapHold {
        tap: Box<Action>,
        hold: Box<Action>,
        timeout_ms: u64,
    },
    TapDance {
        actions: Vec<Action>,
        timeout_ms: u64,
    },
    OneShotModifier {
        modifier: Modifier,
        timeout_ms: u64,
    },
    OneShotLayer {
        layer: String,
        timeout_ms: u64,
    },
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
    #[error("timeout `{0}` must be a positive integer")]
    InvalidTimeout(String),
    #[error("unknown timeout variable `{0}`")]
    UnknownTimeoutVariable(String),
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
    vars: HashMap<String, i64>,
    #[serde(default)]
    aliases: HashMap<String, RawAction>,
    layers: HashMap<String, HashMap<String, RawAction>>,
}

#[derive(Debug, Deserialize)]
struct RawSettings {
    #[serde(default)]
    process_unmapped_keys: bool,
    #[serde(default = "default_startup_layer")]
    startup_layer: String,
    #[serde(default = "default_tap_hold_timeout_ms")]
    tap_hold_timeout_ms: u64,
    #[serde(default = "default_tap_dance_timeout_ms")]
    tap_dance_timeout_ms: u64,
    #[serde(default = "default_one_shot_timeout_ms")]
    one_shot_timeout_ms: u64,
}

impl Default for RawSettings {
    fn default() -> Self {
        Self {
            process_unmapped_keys: false,
            startup_layer: default_startup_layer(),
            tap_hold_timeout_ms: default_tap_hold_timeout_ms(),
            tap_dance_timeout_ms: default_tap_dance_timeout_ms(),
            one_shot_timeout_ms: default_one_shot_timeout_ms(),
        }
    }
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
        tap_hold: Option<RawTapHold>,
        tap_dance: Option<RawTapDance>,
        one_shot_modifier: Option<RawOneShotModifier>,
        one_shot_layer: Option<RawOneShotLayer>,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct RawTapHold {
    tap: Box<RawAction>,
    hold: Box<RawAction>,
    timeout_ms: Option<RawTimeout>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTapDance {
    single: Box<RawAction>,
    double: Option<Box<RawAction>>,
    triple: Option<Box<RawAction>>,
    timeout_ms: Option<RawTimeout>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawOneShotModifier {
    Shorthand(String),
    Explicit {
        modifier: String,
        timeout_ms: Option<RawTimeout>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawOneShotLayer {
    Shorthand(String),
    Explicit {
        layer: String,
        timeout_ms: Option<RawTimeout>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawTimeout {
    Literal(i64),
    Variable(String),
}

impl RuntimeConfig {
    pub fn parse(input: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(input)?;
        validate_settings(&raw.settings)?;
        validate_vars(&raw.vars)?;
        let startup_layer = raw.settings.startup_layer.clone();
        if !raw.layers.contains_key(&startup_layer) {
            return Err(ConfigError::MissingStartupLayer(startup_layer));
        }
        let context = ParseContext {
            settings: &raw.settings,
            vars: &raw.vars,
            aliases: &raw.aliases,
        };

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
                let action = parse_action(&raw_action, &context)?;
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
                self.validate_action_layer_references(owner, action)?;
            }
        }
        Ok(())
    }

    fn validate_action_layer_references(
        &self,
        owner: &str,
        action: &Action,
    ) -> Result<(), ConfigError> {
        match action {
            Action::LayerWhileHeld(target)
            | Action::LayerSwitch(target)
            | Action::OneShotLayer { layer: target, .. } => {
                if !self.layers.contains_key(target) {
                    return Err(ConfigError::MissingLayerReference(
                        target.clone(),
                        owner.to_owned(),
                    ));
                }
            }
            Action::TapHold { tap, hold, .. } => {
                self.validate_action_layer_references(owner, tap)?;
                self.validate_action_layer_references(owner, hold)?;
            }
            Action::TapDance { actions, .. } => {
                for action in actions {
                    self.validate_action_layer_references(owner, action)?;
                }
            }
            Action::Key(_)
            | Action::Chord { .. }
            | Action::OneShotModifier { .. }
            | Action::Transparent
            | Action::Noop => {}
        }
        Ok(())
    }
}

struct ParseContext<'a> {
    settings: &'a RawSettings,
    vars: &'a HashMap<String, i64>,
    aliases: &'a HashMap<String, RawAction>,
}

fn parse_action(raw: &RawAction, context: &ParseContext<'_>) -> Result<Action, ConfigError> {
    parse_action_inner(raw, context, &mut Vec::new())
}

fn parse_action_inner(
    raw: &RawAction,
    context: &ParseContext<'_>,
    alias_stack: &mut Vec<String>,
) -> Result<Action, ConfigError> {
    match raw {
        RawAction::Shorthand(value) => parse_action_string(value, context, alias_stack),
        RawAction::Explicit {
            key,
            chord,
            layer_while_held,
            layer_switch,
            tap_hold,
            tap_dance,
            one_shot_modifier,
            one_shot_layer,
        } => {
            let specified = [
                key.is_some(),
                chord.is_some(),
                layer_while_held.is_some(),
                layer_switch.is_some(),
                tap_hold.is_some(),
                tap_dance.is_some(),
                one_shot_modifier.is_some(),
                one_shot_layer.is_some(),
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
            } else if let Some(tap_hold) = tap_hold {
                Ok(Action::TapHold {
                    tap: Box::new(parse_action_inner(&tap_hold.tap, context, alias_stack)?),
                    hold: Box::new(parse_action_inner(&tap_hold.hold, context, alias_stack)?),
                    timeout_ms: resolve_timeout(
                        tap_hold.timeout_ms.as_ref(),
                        context.settings.tap_hold_timeout_ms,
                        context,
                    )?,
                })
            } else if let Some(tap_dance) = tap_dance {
                let mut actions =
                    vec![parse_action_inner(&tap_dance.single, context, alias_stack)?];
                if let Some(action) = &tap_dance.double {
                    actions.push(parse_action_inner(action, context, alias_stack)?);
                }
                if let Some(action) = &tap_dance.triple {
                    actions.push(parse_action_inner(action, context, alias_stack)?);
                }
                Ok(Action::TapDance {
                    actions,
                    timeout_ms: resolve_timeout(
                        tap_dance.timeout_ms.as_ref(),
                        context.settings.tap_dance_timeout_ms,
                        context,
                    )?,
                })
            } else if let Some(one_shot_modifier) = one_shot_modifier {
                parse_one_shot_modifier(one_shot_modifier, context)
            } else if let Some(one_shot_layer) = one_shot_layer {
                parse_one_shot_layer(one_shot_layer, context)
            } else {
                unreachable!("specified count checked above")
            }
        }
    }
}

fn parse_action_string(
    value: &str,
    context: &ParseContext<'_>,
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
                if let Some(alias) = context.aliases.get(value) {
                    resolve_alias(value, alias, context, alias_stack)
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
    context: &ParseContext<'_>,
    alias_stack: &mut Vec<String>,
) -> Result<Action, ConfigError> {
    if alias_stack.iter().any(|alias| alias == name) {
        alias_stack.push(name.to_owned());
        return Err(ConfigError::RecursiveAlias(alias_stack.join(" -> ")));
    }

    alias_stack.push(name.to_owned());
    let action = parse_action_inner(raw, context, alias_stack);
    alias_stack.pop();
    action
}

fn parse_one_shot_modifier(
    raw: &RawOneShotModifier,
    context: &ParseContext<'_>,
) -> Result<Action, ConfigError> {
    match raw {
        RawOneShotModifier::Shorthand(modifier) => Ok(Action::OneShotModifier {
            modifier: Modifier::from_name(modifier)?,
            timeout_ms: context.settings.one_shot_timeout_ms,
        }),
        RawOneShotModifier::Explicit {
            modifier,
            timeout_ms,
        } => Ok(Action::OneShotModifier {
            modifier: Modifier::from_name(modifier)?,
            timeout_ms: resolve_timeout(
                timeout_ms.as_ref(),
                context.settings.one_shot_timeout_ms,
                context,
            )?,
        }),
    }
}

fn parse_one_shot_layer(
    raw: &RawOneShotLayer,
    context: &ParseContext<'_>,
) -> Result<Action, ConfigError> {
    match raw {
        RawOneShotLayer::Shorthand(layer) => Ok(Action::OneShotLayer {
            layer: layer.clone(),
            timeout_ms: context.settings.one_shot_timeout_ms,
        }),
        RawOneShotLayer::Explicit { layer, timeout_ms } => Ok(Action::OneShotLayer {
            layer: layer.clone(),
            timeout_ms: resolve_timeout(
                timeout_ms.as_ref(),
                context.settings.one_shot_timeout_ms,
                context,
            )?,
        }),
    }
}

fn resolve_timeout(
    raw: Option<&RawTimeout>,
    default: u64,
    context: &ParseContext<'_>,
) -> Result<u64, ConfigError> {
    match raw {
        None => Ok(default),
        Some(RawTimeout::Literal(value)) => positive_timeout(*value, "timeout_ms"),
        Some(RawTimeout::Variable(name)) => {
            let value = context
                .vars
                .get(name)
                .ok_or_else(|| ConfigError::UnknownTimeoutVariable(name.clone()))?;
            positive_timeout(*value, name)
        }
    }
}

fn validate_settings(settings: &RawSettings) -> Result<(), ConfigError> {
    positive_timeout(settings.tap_hold_timeout_ms as i64, "tap_hold_timeout_ms")?;
    positive_timeout(settings.tap_dance_timeout_ms as i64, "tap_dance_timeout_ms")?;
    positive_timeout(settings.one_shot_timeout_ms as i64, "one_shot_timeout_ms")?;
    Ok(())
}

fn validate_vars(vars: &HashMap<String, i64>) -> Result<(), ConfigError> {
    for (name, value) in vars {
        positive_timeout(*value, name)?;
    }
    Ok(())
}

fn positive_timeout(value: i64, name: impl Into<String>) -> Result<u64, ConfigError> {
    if value <= 0 {
        return Err(ConfigError::InvalidTimeout(name.into()));
    }
    Ok(value as u64)
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

fn default_tap_hold_timeout_ms() -> u64 {
    200
}

fn default_tap_dance_timeout_ms() -> u64 {
    200
}

fn default_one_shot_timeout_ms() -> u64 {
    1000
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
        let config = RuntimeConfig::parse(
            r#"
            [source]
            keys = ["A"]

            [layers.base]
            A = "Ctrl+Alt+Delete"
        "#,
        )
        .unwrap();
        assert_eq!(
            config.layers["base"].get(&Key::A),
            Some(&Action::Chord {
                modifiers: vec![Modifier::Ctrl, Modifier::Alt],
                key: Key::Delete
            })
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

    #[test]
    fn parses_tap_feature_timeouts_and_vars() {
        let input = r#"
            [settings]
            startup_layer = "base"
            tap_hold_timeout_ms = 210
            tap_dance_timeout_ms = 220
            one_shot_timeout_ms = 900

            [vars]
            fast_timeout = 150
            one_shot_short = 700

            [aliases]
            aliased_dance = { tap_dance = { single = "Escape", double = "Enter", timeout_ms = "fast_timeout" } }

            [source]
            keys = ["CapsLock", "Space", "Semicolon", "LeftShift", "F"]

            [layers.base]
            CapsLock = { tap_hold = { tap = "Escape", hold = { layer_while_held = "nav" } } }
            Space = { tap_hold = { tap = "Space", hold = { layer_while_held = "symbols" }, timeout_ms = "fast_timeout" } }
            Semicolon = "aliased_dance"
            LeftShift = { one_shot_modifier = { modifier = "Shift", timeout_ms = "one_shot_short" } }
            F = { one_shot_layer = "nav" }

            [layers.nav]
            H = "Left"

            [layers.symbols]
            H = "Semicolon"
        "#;

        let config = RuntimeConfig::parse(input).unwrap();
        assert_eq!(
            config.layers["base"].get(&Key::CapsLock),
            Some(&Action::TapHold {
                tap: Box::new(Action::Key(Key::Escape)),
                hold: Box::new(Action::LayerWhileHeld("nav".to_owned())),
                timeout_ms: 210,
            })
        );
        assert_eq!(
            config.layers["base"].get(&Key::Space),
            Some(&Action::TapHold {
                tap: Box::new(Action::Key(Key::Space)),
                hold: Box::new(Action::LayerWhileHeld("symbols".to_owned())),
                timeout_ms: 150,
            })
        );
        assert_eq!(
            config.layers["base"].get(&Key::Semicolon),
            Some(&Action::TapDance {
                actions: vec![Action::Key(Key::Escape), Action::Key(Key::Enter)],
                timeout_ms: 150,
            })
        );
        assert_eq!(
            config.layers["base"].get(&Key::LeftShift),
            Some(&Action::OneShotModifier {
                modifier: Modifier::Shift,
                timeout_ms: 700,
            })
        );
        assert_eq!(
            config.layers["base"].get(&Key::F),
            Some(&Action::OneShotLayer {
                layer: "nav".to_owned(),
                timeout_ms: 900,
            })
        );
    }

    #[test]
    fn rejects_invalid_timeout_vars() {
        let input = r#"
            [vars]
            none = 0

            [source]
            keys = ["A"]

            [layers.base]
            A = { tap_dance = { single = "A", timeout_ms = "none" } }
        "#;

        let error = RuntimeConfig::parse(input).unwrap_err();
        assert!(matches!(error, ConfigError::InvalidTimeout(_)));
    }

    #[test]
    fn rejects_unknown_timeout_var() {
        let input = r#"
            [source]
            keys = ["A"]

            [layers.base]
            A = { tap_dance = { single = "A", timeout_ms = "missing" } }
        "#;

        let error = RuntimeConfig::parse(input).unwrap_err();
        assert!(matches!(error, ConfigError::UnknownTimeoutVariable(_)));
    }

    #[test]
    fn rejects_non_integer_timeout_var() {
        let input = r#"
            [vars]
            fast = "150"

            [source]
            keys = ["A"]

            [layers.base]
            A = "A"
        "#;

        let error = RuntimeConfig::parse(input).unwrap_err();
        assert!(matches!(error, ConfigError::Toml(_)));
    }

    #[test]
    fn validates_nested_layer_references() {
        let input = r#"
            [source]
            keys = ["A"]

            [layers.base]
            A = { tap_hold = { tap = "A", hold = { layer_while_held = "missing" } } }
        "#;

        let error = RuntimeConfig::parse(input).unwrap_err();
        assert!(matches!(error, ConfigError::MissingLayerReference(_, _)));
    }
}
