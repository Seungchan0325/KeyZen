use crate::key::KeyCode;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const BASE_LAYER: &str = "base";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub tapping_term_ms: u64,
    pub tap_dance_term_ms: u64,
    pub one_shot_timeout_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tapping_term_ms: 200,
            tap_dance_term_ms: 180,
            one_shot_timeout_ms: 1_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    pub layers: BTreeMap<String, Layer>,
}

pub type Layer = BTreeMap<KeyCode, Action>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Send(KeyCode),
    Chord(Vec<KeyCode>),
    LayerPush(String),
    LayerPop(Option<String>),
    LayerToggle(String),
    TapHold { tap: Box<Action>, hold: Box<Action> },
    TapDance(BTreeMap<u8, Action>),
    OneShotLayer(String),
    OneShotModifier(KeyCode),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("configuration must define a `base` layer")]
    MissingBaseLayer,
    #[error("layer name cannot be empty")]
    EmptyLayerName,
    #[error("layer `{layer}` maps an empty key name")]
    EmptyKeyName { layer: String },
    #[error("setting `{name}` must be greater than zero")]
    InvalidTiming { name: &'static str },
    #[error("action at {path} references unknown layer `{layer}`")]
    UnknownLayer { path: String, layer: String },
    #[error("action at {path} cannot target the base layer")]
    BaseLayerTarget { path: String },
    #[error("action at {path} uses an empty chord")]
    EmptyChord { path: String },
    #[error("tap dance at {path} must define at least one tap count")]
    EmptyTapDance { path: String },
    #[error("tap dance at {path} uses tap count 0")]
    ZeroTapDanceCount { path: String },
    #[error("temporal action cannot be nested at {path}")]
    NestedTemporalAction { path: String },
    #[error("invalid key at {path}: {key}")]
    InvalidKey { path: String, key: String },
}

impl Config {
    pub fn from_yaml_str(input: &str) -> Result<Self, ConfigError> {
        let mut config: Self = serde_yaml::from_str(input)?;
        config.normalize()?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_yaml_string(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.settings.tapping_term_ms == 0 {
            return Err(ConfigError::InvalidTiming {
                name: "tapping_term_ms",
            });
        }
        if self.settings.tap_dance_term_ms == 0 {
            return Err(ConfigError::InvalidTiming {
                name: "tap_dance_term_ms",
            });
        }
        if self.settings.one_shot_timeout_ms == 0 {
            return Err(ConfigError::InvalidTiming {
                name: "one_shot_timeout_ms",
            });
        }
        if !self.layers.contains_key(BASE_LAYER) {
            return Err(ConfigError::MissingBaseLayer);
        }

        let layer_names = self.layers.keys().cloned().collect::<BTreeSet<_>>();
        for (layer_name, layer) in &self.layers {
            if layer_name.trim().is_empty() {
                return Err(ConfigError::EmptyLayerName);
            }
            for (key, action) in layer {
                if key.as_str().trim().is_empty() {
                    return Err(ConfigError::EmptyKeyName {
                        layer: layer_name.clone(),
                    });
                }
                let path = format!("layers.{layer_name}.{}", key.as_str());
                validate_action(action, &path, &layer_names, false)?;
            }
        }
        Ok(())
    }

    fn normalize(&mut self) -> Result<(), ConfigError> {
        let mut normalized_layers = BTreeMap::new();
        for (layer_name, layer) in std::mem::take(&mut self.layers) {
            let layer_name = layer_name.trim().to_string();
            if layer_name.is_empty() {
                return Err(ConfigError::EmptyLayerName);
            }
            let mut normalized_layer = BTreeMap::new();
            for (key, action) in layer {
                normalized_layer.insert(key, action);
            }
            normalized_layers.insert(layer_name, normalized_layer);
        }
        self.layers = normalized_layers;
        Ok(())
    }
}

fn validate_action(
    action: &Action,
    path: &str,
    layer_names: &BTreeSet<String>,
    nested_temporal: bool,
) -> Result<(), ConfigError> {
    match action {
        Action::Send(key) | Action::OneShotModifier(key) => validate_key(key, path),
        Action::Chord(keys) => {
            if keys.is_empty() {
                return Err(ConfigError::EmptyChord {
                    path: path.to_string(),
                });
            }
            for key in keys {
                validate_key(key, path)?;
            }
            Ok(())
        }
        Action::LayerPush(layer) | Action::LayerToggle(layer) | Action::OneShotLayer(layer) => {
            validate_layer_target(layer, path, layer_names)
        }
        Action::LayerPop(layer) => {
            if let Some(layer) = layer {
                validate_layer_target(layer, path, layer_names)?;
            }
            Ok(())
        }
        Action::TapHold { tap, hold } => {
            if nested_temporal {
                return Err(ConfigError::NestedTemporalAction {
                    path: path.to_string(),
                });
            }
            validate_action(tap, &format!("{path}.tap_hold.tap"), layer_names, true)?;
            validate_action(hold, &format!("{path}.tap_hold.hold"), layer_names, true)
        }
        Action::TapDance(actions) => {
            if nested_temporal {
                return Err(ConfigError::NestedTemporalAction {
                    path: path.to_string(),
                });
            }
            if actions.is_empty() {
                return Err(ConfigError::EmptyTapDance {
                    path: path.to_string(),
                });
            }
            for (count, action) in actions {
                if *count == 0 {
                    return Err(ConfigError::ZeroTapDanceCount {
                        path: path.to_string(),
                    });
                }
                validate_action(
                    action,
                    &format!("{path}.tap_dance.{count}"),
                    layer_names,
                    true,
                )?;
            }
            Ok(())
        }
    }
}

fn validate_key(key: &KeyCode, path: &str) -> Result<(), ConfigError> {
    if key.as_str().trim().is_empty() {
        return Err(ConfigError::InvalidKey {
            path: path.to_string(),
            key: key.to_string(),
        });
    }
    Ok(())
}

fn validate_layer_target(
    layer: &str,
    path: &str,
    layer_names: &BTreeSet<String>,
) -> Result<(), ConfigError> {
    if layer == BASE_LAYER {
        return Err(ConfigError::BaseLayerTarget {
            path: path.to_string(),
        });
    }
    if !layer_names.contains(layer) {
        return Err(ConfigError::UnknownLayer {
            path: path.to_string(),
            layer: layer.to_string(),
        });
    }
    Ok(())
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        action_from_value(value).map_err(de::Error::custom)
    }
}

impl Serialize for Action {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Action::Send(key) => serializer.serialize_str(key.as_str()),
            Action::Chord(keys) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("chord", keys)?;
                map.end()
            }
            Action::LayerPush(layer) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("layer_push", layer)?;
                map.end()
            }
            Action::LayerPop(layer) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("layer_pop", layer)?;
                map.end()
            }
            Action::LayerToggle(layer) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("layer_toggle", layer)?;
                map.end()
            }
            Action::TapHold { tap, hold } => {
                let mut outer = serializer.serialize_map(Some(1))?;
                outer.serialize_entry("tap_hold", &TapHoldSerialize { tap, hold })?;
                outer.end()
            }
            Action::TapDance(actions) => {
                let mut outer = serializer.serialize_map(Some(1))?;
                outer.serialize_entry("tap_dance", &TapDanceSerialize(actions))?;
                outer.end()
            }
            Action::OneShotLayer(layer) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("one_shot_layer", layer)?;
                map.end()
            }
            Action::OneShotModifier(key) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("one_shot_modifier", key)?;
                map.end()
            }
        }
    }
}

struct TapHoldSerialize<'a> {
    tap: &'a Action,
    hold: &'a Action,
}

impl Serialize for TapHoldSerialize<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("tap", self.tap)?;
        map.serialize_entry("hold", self.hold)?;
        map.end()
    }
}

struct TapDanceSerialize<'a>(&'a BTreeMap<u8, Action>);

impl Serialize for TapDanceSerialize<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (count, action) in self.0 {
            map.serialize_entry(count, action)?;
        }
        map.end()
    }
}

fn action_from_value(value: Value) -> Result<Action, ActionParseError> {
    match value {
        Value::String(key) => Ok(Action::Send(parse_key(&key)?)),
        Value::Mapping(mapping) => action_from_mapping(mapping),
        other => Err(ActionParseError::InvalidShape(format!(
            "expected string or action map, found {other:?}"
        ))),
    }
}

fn action_from_mapping(mapping: Mapping) -> Result<Action, ActionParseError> {
    if mapping.len() != 1 {
        return Err(ActionParseError::InvalidShape(
            "action map must contain exactly one action key".to_string(),
        ));
    }

    let (key, value) = mapping
        .into_iter()
        .next()
        .expect("mapping length checked above");
    let Some(action_name) = key.as_str() else {
        return Err(ActionParseError::InvalidShape(
            "action name must be a string".to_string(),
        ));
    };

    match action_name {
        "send" => string_value(value, "send").and_then(|key| parse_key(&key).map(Action::Send)),
        "chord" => sequence_value(value, "chord").and_then(|values| {
            values
                .into_iter()
                .map(|value| string_value(value, "chord item").and_then(|key| parse_key(&key)))
                .collect::<Result<Vec<_>, _>>()
                .map(Action::Chord)
        }),
        "layer_push" => string_value(value, "layer_push").map(Action::LayerPush),
        "layer_pop" => match value {
            Value::Null => Ok(Action::LayerPop(None)),
            other => string_value(other, "layer_pop").map(|layer| Action::LayerPop(Some(layer))),
        },
        "layer_toggle" => string_value(value, "layer_toggle").map(Action::LayerToggle),
        "one_shot_layer" => string_value(value, "one_shot_layer").map(Action::OneShotLayer),
        "one_shot_modifier" => string_value(value, "one_shot_modifier")
            .and_then(|key| parse_key(&key).map(Action::OneShotModifier)),
        "tap_hold" => tap_hold_from_value(value),
        "tap_dance" => tap_dance_from_value(value),
        other => Err(ActionParseError::UnknownAction(other.to_string())),
    }
}

fn tap_hold_from_value(value: Value) -> Result<Action, ActionParseError> {
    let mapping = mapping_value(value, "tap_hold")?;
    let tap_key = Value::String("tap".to_string());
    let hold_key = Value::String("hold".to_string());
    let tap = mapping
        .get(&tap_key)
        .ok_or_else(|| ActionParseError::MissingField("tap_hold.tap".to_string()))?;
    let hold = mapping
        .get(&hold_key)
        .ok_or_else(|| ActionParseError::MissingField("tap_hold.hold".to_string()))?;

    Ok(Action::TapHold {
        tap: Box::new(action_from_value(tap.clone())?),
        hold: Box::new(action_from_value(hold.clone())?),
    })
}

fn tap_dance_from_value(value: Value) -> Result<Action, ActionParseError> {
    let mapping = mapping_value(value, "tap_dance")?;
    let mut actions = BTreeMap::new();
    for (count, action) in mapping {
        let count = match count {
            Value::Number(number) => number
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or_else(|| {
                    ActionParseError::InvalidShape("tap_dance count must fit in u8".to_string())
                })?,
            Value::String(value) => value.parse::<u8>().map_err(|_| {
                ActionParseError::InvalidShape("tap_dance count must be numeric".to_string())
            })?,
            _ => {
                return Err(ActionParseError::InvalidShape(
                    "tap_dance count must be numeric".to_string(),
                ));
            }
        };
        actions.insert(count, action_from_value(action)?);
    }
    Ok(Action::TapDance(actions))
}

fn parse_key(value: &str) -> Result<KeyCode, ActionParseError> {
    KeyCode::new(value).map_err(|error| ActionParseError::InvalidShape(error.to_string()))
}

fn string_value(value: Value, field: &str) -> Result<String, ActionParseError> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(ActionParseError::InvalidShape(format!(
            "{field} must be a string, found {other:?}"
        ))),
    }
}

fn sequence_value(value: Value, field: &str) -> Result<Vec<Value>, ActionParseError> {
    match value {
        Value::Sequence(values) => Ok(values),
        other => Err(ActionParseError::InvalidShape(format!(
            "{field} must be a sequence, found {other:?}"
        ))),
    }
}

fn mapping_value(value: Value, field: &str) -> Result<Mapping, ActionParseError> {
    match value {
        Value::Mapping(mapping) => Ok(mapping),
        other => Err(ActionParseError::InvalidShape(format!(
            "{field} must be a map, found {other:?}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActionParseError {
    UnknownAction(String),
    MissingField(String),
    InvalidShape(String),
}

impl fmt::Display for ActionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAction(action) => write!(f, "unknown action `{action}`"),
            Self::MissingField(field) => write!(f, "missing field `{field}`"),
            Self::InvalidShape(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ActionParseError {}

#[cfg(test)]
mod tests {
    use super::{Action, Config, ConfigError};

    #[test]
    fn parses_example_yaml() {
        let config = Config::from_yaml_str(
            r#"
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
          layer_push: nav
    Quote:
      tap_dance:
        1: Quote
        2: DoubleQuote
    LeftShift:
      one_shot_modifier: Shift
  nav:
    H: Left
"#,
        )
        .unwrap();

        assert!(config.layers.contains_key("base"));
        assert!(config.layers.contains_key("nav"));
    }

    #[test]
    fn rejects_missing_base_layer() {
        let error = Config::from_yaml_str(
            r#"
layers:
  nav:
    H: Left
"#,
        )
        .unwrap_err();

        assert!(matches!(error, ConfigError::MissingBaseLayer));
    }

    #[test]
    fn rejects_unknown_layer_reference() {
        let error = Config::from_yaml_str(
            r#"
layers:
  base:
    CapsLock:
      layer_push: missing
"#,
        )
        .unwrap_err();

        assert!(matches!(error, ConfigError::UnknownLayer { .. }));
    }

    #[test]
    fn rejects_nested_temporal_actions() {
        let error = Config::from_yaml_str(
            r#"
layers:
  base:
    A:
      tap_hold:
        tap:
          tap_dance:
            1: Escape
        hold: Ctrl
"#,
        )
        .unwrap_err();

        assert!(matches!(error, ConfigError::NestedTemporalAction { .. }));
    }

    #[test]
    fn serializes_normalized_action_names() {
        let config = Config::from_yaml_str(
            r#"
layers:
  base:
    esc:
      send: space
"#,
        )
        .unwrap();

        let layer = config.layers.get("base").unwrap();
        let action = layer.values().next().unwrap();
        assert_eq!(action, &Action::Send("Space".parse().unwrap()));
        assert!(config.to_yaml_string().unwrap().contains("Escape"));
    }
}
