use crate::{
    config::{Action, RuntimeConfig},
    key::Key,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineEvent {
    pub key: Key,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputEvent {
    pub key: Key,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputPlan {
    pub events: Vec<OutputEvent>,
    pub consume_input: bool,
}

#[derive(Debug, Clone)]
struct ActivePress {
    action: Action,
}

#[derive(Debug, Clone)]
pub struct Engine {
    config: RuntimeConfig,
    base_layer: String,
    held_layers: Vec<(Key, String)>,
    active_presses: std::collections::HashMap<Key, ActivePress>,
}

impl Engine {
    pub fn new(config: RuntimeConfig) -> Self {
        let base_layer = config.startup_layer.clone();
        Self {
            config,
            base_layer,
            held_layers: Vec::new(),
            active_presses: std::collections::HashMap::new(),
        }
    }

    pub fn reload(&mut self, config: RuntimeConfig) {
        self.base_layer = config.startup_layer.clone();
        self.config = config;
        self.held_layers.clear();
        self.active_presses.clear();
    }

    pub fn handle_event(&mut self, event: EngineEvent) -> OutputPlan {
        if !self.should_process(event.key) {
            return OutputPlan::default();
        }

        match event.kind {
            EventKind::Down => self.handle_down(event.key),
            EventKind::Up => self.handle_up(event.key),
        }
    }

    pub fn base_layer(&self) -> &str {
        &self.base_layer
    }

    pub fn held_layer_names(&self) -> Vec<&str> {
        self.held_layers
            .iter()
            .map(|(_, layer)| layer.as_str())
            .collect()
    }

    fn should_process(&self, key: Key) -> bool {
        self.config.process_unmapped_keys || self.config.source_keys.contains(&key)
    }

    fn handle_down(&mut self, key: Key) -> OutputPlan {
        let action = self.resolve_action(key);
        let Some(action) = action else {
            return OutputPlan::default();
        };

        let mut plan = OutputPlan {
            events: Vec::new(),
            consume_input: true,
        };

        match &action {
            Action::Key(output) => plan.events.push(OutputEvent {
                key: *output,
                kind: EventKind::Down,
            }),
            Action::Chord { modifiers, key } => {
                for modifier in modifiers {
                    plan.events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Down,
                    });
                }
                plan.events.push(OutputEvent {
                    key: *key,
                    kind: EventKind::Down,
                });
            }
            Action::LayerWhileHeld(layer) => self.held_layers.push((key, layer.clone())),
            Action::LayerSwitch(layer) => self.base_layer = layer.clone(),
            Action::Transparent => return OutputPlan::default(),
            Action::Noop => {}
        }

        self.active_presses.insert(key, ActivePress { action });
        plan
    }

    fn handle_up(&mut self, key: Key) -> OutputPlan {
        self.held_layers.retain(|(held_key, _)| *held_key != key);
        let Some(active) = self.active_presses.remove(&key) else {
            return OutputPlan::default();
        };

        let mut plan = OutputPlan {
            events: Vec::new(),
            consume_input: true,
        };

        match active.action {
            Action::Key(output) => plan.events.push(OutputEvent {
                key: output,
                kind: EventKind::Up,
            }),
            Action::Chord { modifiers, key } => {
                plan.events.push(OutputEvent {
                    key,
                    kind: EventKind::Up,
                });
                for modifier in modifiers.iter().rev() {
                    plan.events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Up,
                    });
                }
            }
            Action::LayerWhileHeld(_) | Action::LayerSwitch(_) | Action::Noop => {}
            Action::Transparent => return OutputPlan::default(),
        }

        plan
    }

    fn resolve_action(&self, key: Key) -> Option<Action> {
        for (_, layer_name) in self.held_layers.iter().rev() {
            if let Some(action) = self.action_from_layer(layer_name, key) {
                if action == Action::Transparent {
                    continue;
                }
                return Some(action);
            }
        }

        if let Some(action) = self.action_from_layer(&self.base_layer, key) {
            if action != Action::Transparent {
                return Some(action);
            }
        }

        if self.config.process_unmapped_keys {
            Some(Action::Key(key))
        } else {
            None
        }
    }

    fn action_from_layer(&self, layer_name: &str, key: Key) -> Option<Action> {
        self.config
            .layers
            .get(layer_name)
            .and_then(|layer| layer.get(&key))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_engine() -> Engine {
        let config = RuntimeConfig::parse(
            r#"
            [settings]
            startup_layer = "base"
            [source]
            keys = ["CapsLock", "H", "J", "Space"]
            [layers.base]
            CapsLock = { layer_while_held = "nav" }
            H = "H"
            J = "Ctrl+J"
            Space = "Space"
            [layers.nav]
            H = "Left"
            Space = "transparent"
        "#,
        )
        .unwrap();
        Engine::new(config)
    }

    #[test]
    fn layer_while_held_pushes_and_pops() {
        let mut engine = sample_engine();
        let plan = engine.handle_event(EngineEvent {
            key: Key::CapsLock,
            kind: EventKind::Down,
        });
        assert!(plan.consume_input);
        assert!(plan.events.is_empty());
        assert_eq!(engine.held_layer_names(), vec!["nav"]);

        engine.handle_event(EngineEvent {
            key: Key::CapsLock,
            kind: EventKind::Up,
        });
        assert!(engine.held_layer_names().is_empty());
    }

    #[test]
    fn held_layer_overrides_base() {
        let mut engine = sample_engine();
        engine.handle_event(EngineEvent {
            key: Key::CapsLock,
            kind: EventKind::Down,
        });
        let plan = engine.handle_event(EngineEvent {
            key: Key::H,
            kind: EventKind::Down,
        });
        assert_eq!(
            plan.events,
            vec![OutputEvent {
                key: Key::Left,
                kind: EventKind::Down
            }]
        );
    }

    #[test]
    fn transparent_falls_back_to_base() {
        let mut engine = sample_engine();
        engine.handle_event(EngineEvent {
            key: Key::CapsLock,
            kind: EventKind::Down,
        });
        let plan = engine.handle_event(EngineEvent {
            key: Key::Space,
            kind: EventKind::Down,
        });
        assert_eq!(
            plan.events,
            vec![OutputEvent {
                key: Key::Space,
                kind: EventKind::Down
            }]
        );
    }

    #[test]
    fn chord_releases_in_reverse_order() {
        let mut engine = sample_engine();
        let down = engine.handle_event(EngineEvent {
            key: Key::J,
            kind: EventKind::Down,
        });
        assert_eq!(
            down.events,
            vec![
                OutputEvent {
                    key: Key::LeftCtrl,
                    kind: EventKind::Down
                },
                OutputEvent {
                    key: Key::J,
                    kind: EventKind::Down
                }
            ]
        );

        let up = engine.handle_event(EngineEvent {
            key: Key::J,
            kind: EventKind::Up,
        });
        assert_eq!(
            up.events,
            vec![
                OutputEvent {
                    key: Key::J,
                    kind: EventKind::Up
                },
                OutputEvent {
                    key: Key::LeftCtrl,
                    kind: EventKind::Up
                }
            ]
        );
    }
}
