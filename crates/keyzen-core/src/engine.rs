use crate::{
    config::{Action, RuntimeConfig},
    key::{Key, Modifier},
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
    pub next_deadline_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ActivePress {
    action: Action,
    one_shot_modifiers: Vec<Modifier>,
    suppress_repeated_down: bool,
}

#[derive(Debug, Clone)]
struct PendingTapHold {
    key: Key,
    tap: Action,
    hold: Action,
    deadline_ms: u64,
}

#[derive(Debug, Clone)]
struct PendingTapDance {
    key: Key,
    actions: Vec<Action>,
    timeout_ms: u64,
    count: usize,
    deadline_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct PendingOneShotModifier {
    modifier: Modifier,
    deadline_ms: u64,
}

#[derive(Debug, Clone)]
struct PendingOneShotLayer {
    layer: String,
    deadline_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Engine {
    config: RuntimeConfig,
    base_layer: String,
    held_layers: Vec<(Key, String)>,
    active_presses: std::collections::HashMap<Key, ActivePress>,
    pending_tap_holds: Vec<PendingTapHold>,
    pending_tap_dances: Vec<PendingTapDance>,
    pending_one_shot_modifiers: Vec<PendingOneShotModifier>,
    pending_one_shot_layer: Option<PendingOneShotLayer>,
}

impl Engine {
    pub fn new(config: RuntimeConfig) -> Self {
        let base_layer = config.startup_layer.clone();
        Self {
            config,
            base_layer,
            held_layers: Vec::new(),
            active_presses: std::collections::HashMap::new(),
            pending_tap_holds: Vec::new(),
            pending_tap_dances: Vec::new(),
            pending_one_shot_modifiers: Vec::new(),
            pending_one_shot_layer: None,
        }
    }

    pub fn reload(&mut self, config: RuntimeConfig) -> OutputPlan {
        let mut plan = self.reset();
        self.base_layer = config.startup_layer.clone();
        self.config = config;
        plan.next_deadline_ms = self.next_deadline_ms();
        plan
    }

    pub fn reset(&mut self) -> OutputPlan {
        let mut plan = OutputPlan {
            events: Vec::new(),
            consume_input: false,
            next_deadline_ms: None,
        };
        let active = std::mem::take(&mut self.active_presses);
        for active in active.into_values() {
            self.release_action(&active.action, &active.one_shot_modifiers, &mut plan.events);
        }
        self.held_layers.clear();
        self.pending_tap_holds.clear();
        self.pending_tap_dances.clear();
        self.pending_one_shot_modifiers.clear();
        self.pending_one_shot_layer = None;
        plan
    }

    pub fn handle_event(&mut self, event: EngineEvent) -> OutputPlan {
        self.handle_event_at(event, 0)
    }

    pub fn handle_event_at(&mut self, event: EngineEvent, now_ms: u64) -> OutputPlan {
        let mut plan = self.handle_time(now_ms);
        if !self.should_process(event.key) {
            plan.next_deadline_ms = self.next_deadline_ms();
            return plan;
        }

        let event_plan = match event.kind {
            EventKind::Down => self.handle_down(event.key, now_ms),
            EventKind::Up => self.handle_up(event.key, now_ms),
        };
        plan.events.extend(event_plan.events);
        plan.consume_input |= event_plan.consume_input;
        plan.next_deadline_ms = self.next_deadline_ms();
        plan
    }

    pub fn handle_time(&mut self, now_ms: u64) -> OutputPlan {
        let mut plan = OutputPlan::default();
        self.expire_one_shots(now_ms);
        self.force_due_tap_holds(now_ms, &mut plan);
        self.finalize_due_tap_dances(now_ms, &mut plan);
        plan.next_deadline_ms = self.next_deadline_ms();
        plan
    }

    pub fn next_deadline_ms(&self) -> Option<u64> {
        let tap_hold = self
            .pending_tap_holds
            .iter()
            .map(|pending| pending.deadline_ms);
        let tap_dance = self
            .pending_tap_dances
            .iter()
            .filter_map(|pending| pending.deadline_ms);
        let one_shot_modifiers = self
            .pending_one_shot_modifiers
            .iter()
            .map(|pending| pending.deadline_ms);
        let one_shot_layer = self
            .pending_one_shot_layer
            .iter()
            .map(|pending| pending.deadline_ms);
        tap_hold
            .chain(tap_dance)
            .chain(one_shot_modifiers)
            .chain(one_shot_layer)
            .min()
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

    fn handle_down(&mut self, key: Key, now_ms: u64) -> OutputPlan {
        let mut plan = OutputPlan {
            events: Vec::new(),
            consume_input: true,
            next_deadline_ms: None,
        };

        if self
            .pending_tap_holds
            .iter()
            .any(|pending| pending.key == key)
            || self
                .active_presses
                .get(&key)
                .is_some_and(|active| active.suppress_repeated_down)
        {
            return plan;
        }

        self.force_interrupted_tap_holds(key, now_ms, &mut plan);
        self.finalize_interrupted_tap_dances(key, &mut plan);

        let action = self.resolve_action(key);
        let one_shot_layer_used = self.pending_one_shot_layer.take().is_some();
        let Some(action) = action else {
            return if one_shot_layer_used {
                plan
            } else {
                OutputPlan::default()
            };
        };

        self.press_action_for_key(key, action, now_ms, false, &mut plan);
        plan
    }

    fn handle_up(&mut self, key: Key, now_ms: u64) -> OutputPlan {
        if let Some(index) = self
            .pending_tap_holds
            .iter()
            .position(|pending| pending.key == key)
        {
            let pending = self.pending_tap_holds.remove(index);
            let mut plan = OutputPlan {
                events: Vec::new(),
                consume_input: true,
                next_deadline_ms: None,
            };
            if now_ms >= pending.deadline_ms {
                self.press_action_for_key(key, pending.hold, now_ms, true, &mut plan);
                self.release_active_key(key, &mut plan);
            } else {
                self.tap_action(&pending.tap, &mut plan.events);
            }
            return plan;
        }

        if let Some(index) = self
            .pending_tap_dances
            .iter()
            .position(|pending| pending.key == key)
        {
            let mut plan = OutputPlan {
                events: Vec::new(),
                consume_input: true,
                next_deadline_ms: None,
            };
            let pending = &mut self.pending_tap_dances[index];
            pending.count += 1;
            if pending.count >= pending.actions.len() {
                let pending = self.pending_tap_dances.remove(index);
                self.tap_action(&pending.actions[pending.count - 1], &mut plan.events);
            } else {
                pending.deadline_ms = Some(now_ms + pending.timeout_ms);
            }
            return plan;
        }

        let mut plan = OutputPlan::default_with_consume();
        self.release_active_key(key, &mut plan);
        plan
    }

    fn release_active_key(&mut self, key: Key, plan: &mut OutputPlan) {
        self.held_layers.retain(|(held_key, _)| *held_key != key);
        let Some(active) = self.active_presses.remove(&key) else {
            return;
        };

        plan.consume_input = true;
        self.release_action(&active.action, &active.one_shot_modifiers, &mut plan.events);
    }

    fn resolve_action(&self, key: Key) -> Option<Action> {
        if let Some(pending) = &self.pending_one_shot_layer {
            if let Some(action) = self.action_from_layer(&pending.layer, key) {
                if action != Action::Transparent {
                    return Some(action);
                }
            }
        }

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

    fn press_action_for_key(
        &mut self,
        key: Key,
        action: Action,
        now_ms: u64,
        suppress_repeated_down: bool,
        plan: &mut OutputPlan,
    ) {
        match &action {
            Action::TapHold {
                tap,
                hold,
                timeout_ms,
            } => {
                self.pending_tap_holds.push(PendingTapHold {
                    key,
                    tap: tap.as_ref().clone(),
                    hold: hold.as_ref().clone(),
                    deadline_ms: now_ms + timeout_ms,
                });
            }
            Action::TapDance {
                actions,
                timeout_ms,
            } => {
                self.pending_tap_dances.push(PendingTapDance {
                    key,
                    actions: actions.clone(),
                    timeout_ms: *timeout_ms,
                    count: 0,
                    deadline_ms: None,
                });
            }
            Action::OneShotModifier {
                modifier,
                timeout_ms,
            } => {
                self.pending_one_shot_modifiers
                    .push(PendingOneShotModifier {
                        modifier: *modifier,
                        deadline_ms: now_ms + timeout_ms,
                    });
                self.active_presses.insert(
                    key,
                    ActivePress {
                        action: Action::Noop,
                        one_shot_modifiers: Vec::new(),
                        suppress_repeated_down: true,
                    },
                );
            }
            Action::OneShotLayer { layer, timeout_ms } => {
                self.pending_one_shot_layer = Some(PendingOneShotLayer {
                    layer: layer.clone(),
                    deadline_ms: now_ms + timeout_ms,
                });
                self.active_presses.insert(
                    key,
                    ActivePress {
                        action: Action::Noop,
                        one_shot_modifiers: Vec::new(),
                        suppress_repeated_down: true,
                    },
                );
            }
            _ => {
                let one_shot_modifiers = if self.action_has_output(&action) {
                    std::mem::take(&mut self.pending_one_shot_modifiers)
                        .into_iter()
                        .map(|pending| pending.modifier)
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                self.press_action(&action, &one_shot_modifiers, key, &mut plan.events);
                self.active_presses.insert(
                    key,
                    ActivePress {
                        action,
                        one_shot_modifiers,
                        suppress_repeated_down,
                    },
                );
            }
        }
    }

    fn press_action(
        &mut self,
        action: &Action,
        one_shot_modifiers: &[Modifier],
        physical_key: Key,
        events: &mut Vec<OutputEvent>,
    ) {
        match action {
            Action::Key(output) => {
                for modifier in one_shot_modifiers {
                    events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Down,
                    });
                }
                events.push(OutputEvent {
                    key: *output,
                    kind: EventKind::Down,
                });
            }
            Action::Chord { modifiers, key } => {
                for modifier in one_shot_modifiers {
                    events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Down,
                    });
                }
                for modifier in modifiers {
                    events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Down,
                    });
                }
                events.push(OutputEvent {
                    key: *key,
                    kind: EventKind::Down,
                });
            }
            Action::LayerWhileHeld(layer) => self.held_layers.push((physical_key, layer.clone())),
            Action::LayerSwitch(layer) => self.base_layer = layer.clone(),
            Action::Transparent | Action::Noop => {}
            Action::TapHold { .. }
            | Action::TapDance { .. }
            | Action::OneShotModifier { .. }
            | Action::OneShotLayer { .. } => {}
        }
    }

    fn release_action(
        &mut self,
        action: &Action,
        one_shot_modifiers: &[Modifier],
        events: &mut Vec<OutputEvent>,
    ) {
        match action {
            Action::Key(output) => {
                events.push(OutputEvent {
                    key: *output,
                    kind: EventKind::Up,
                });
                for modifier in one_shot_modifiers.iter().rev() {
                    events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Up,
                    });
                }
            }
            Action::Chord { modifiers, key } => {
                events.push(OutputEvent {
                    key: *key,
                    kind: EventKind::Up,
                });
                for modifier in modifiers.iter().rev() {
                    events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Up,
                    });
                }
                for modifier in one_shot_modifiers.iter().rev() {
                    events.push(OutputEvent {
                        key: modifier.output_key(),
                        kind: EventKind::Up,
                    });
                }
            }
            Action::LayerWhileHeld(_)
            | Action::LayerSwitch(_)
            | Action::Transparent
            | Action::Noop
            | Action::TapHold { .. }
            | Action::TapDance { .. }
            | Action::OneShotModifier { .. }
            | Action::OneShotLayer { .. } => {}
        }
    }

    fn tap_action(&mut self, action: &Action, events: &mut Vec<OutputEvent>) {
        let one_shot_modifiers = if self.action_has_output(action) {
            std::mem::take(&mut self.pending_one_shot_modifiers)
                .into_iter()
                .map(|pending| pending.modifier)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        match action {
            Action::LayerWhileHeld(_) => {}
            _ => {
                self.press_action(action, &one_shot_modifiers, Key::CapsLock, events);
                self.release_action(action, &one_shot_modifiers, events);
            }
        }
    }

    fn action_has_output(&self, action: &Action) -> bool {
        matches!(action, Action::Key(_) | Action::Chord { .. })
    }

    fn expire_one_shots(&mut self, now_ms: u64) {
        self.pending_one_shot_modifiers
            .retain(|pending| pending.deadline_ms > now_ms);
        if self
            .pending_one_shot_layer
            .as_ref()
            .is_some_and(|pending| pending.deadline_ms <= now_ms)
        {
            self.pending_one_shot_layer = None;
        }
    }

    fn force_due_tap_holds(&mut self, now_ms: u64, plan: &mut OutputPlan) {
        let mut index = 0;
        while index < self.pending_tap_holds.len() {
            if self.pending_tap_holds[index].deadline_ms <= now_ms {
                let pending = self.pending_tap_holds.remove(index);
                self.press_action_for_key(pending.key, pending.hold, now_ms, true, plan);
            } else {
                index += 1;
            }
        }
    }

    fn force_interrupted_tap_holds(
        &mut self,
        interrupting_key: Key,
        now_ms: u64,
        plan: &mut OutputPlan,
    ) {
        let mut index = 0;
        while index < self.pending_tap_holds.len() {
            if self.pending_tap_holds[index].key != interrupting_key {
                let pending = self.pending_tap_holds.remove(index);
                self.press_action_for_key(pending.key, pending.hold, now_ms, true, plan);
            } else {
                index += 1;
            }
        }
    }

    fn finalize_due_tap_dances(&mut self, now_ms: u64, plan: &mut OutputPlan) {
        let mut index = 0;
        while index < self.pending_tap_dances.len() {
            let due = self.pending_tap_dances[index]
                .deadline_ms
                .is_some_and(|deadline| deadline <= now_ms);
            if due {
                let pending = self.pending_tap_dances.remove(index);
                if pending.count > 0 {
                    self.tap_action(&pending.actions[pending.count - 1], &mut plan.events);
                }
            } else {
                index += 1;
            }
        }
    }

    fn finalize_interrupted_tap_dances(&mut self, interrupting_key: Key, plan: &mut OutputPlan) {
        let mut index = 0;
        while index < self.pending_tap_dances.len() {
            if self.pending_tap_dances[index].key != interrupting_key
                && self.pending_tap_dances[index].count > 0
            {
                let pending = self.pending_tap_dances.remove(index);
                self.tap_action(&pending.actions[pending.count - 1], &mut plan.events);
            } else {
                index += 1;
            }
        }
    }
}

impl OutputPlan {
    fn default_with_consume() -> Self {
        Self {
            events: Vec::new(),
            consume_input: true,
            next_deadline_ms: None,
        }
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

    fn feature_engine() -> Engine {
        let config = RuntimeConfig::parse(
            r#"
            [settings]
            startup_layer = "base"
            tap_hold_timeout_ms = 200
            tap_dance_timeout_ms = 200
            one_shot_timeout_ms = 1000

            [source]
            keys = ["CapsLock", "Semicolon", "LeftShift", "F", "H", "Space", "A"]

            [layers.base]
            CapsLock = { tap_hold = { tap = "Escape", hold = { layer_while_held = "nav" } } }
            Semicolon = { tap_dance = { single = "Semicolon", double = "Escape", triple = "Enter" } }
            LeftShift = { one_shot_modifier = "Shift" }
            F = { one_shot_layer = "nav" }
            H = "H"
            Space = "Space"
            A = "A"

            [layers.nav]
            H = "Left"
            Space = "transparent"
        "#,
        )
        .unwrap();
        Engine::new(config)
    }

    fn down(key: Key) -> EngineEvent {
        EngineEvent {
            key,
            kind: EventKind::Down,
        }
    }

    fn up(key: Key) -> EngineEvent {
        EngineEvent {
            key,
            kind: EventKind::Up,
        }
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

    #[test]
    fn tap_hold_taps_before_timeout() {
        let mut engine = feature_engine();
        let down_plan = engine.handle_event_at(down(Key::CapsLock), 0);
        assert!(down_plan.consume_input);
        assert_eq!(down_plan.events, vec![]);
        assert_eq!(down_plan.next_deadline_ms, Some(200));

        let up_plan = engine.handle_event_at(up(Key::CapsLock), 100);
        assert_eq!(
            up_plan.events,
            vec![
                OutputEvent {
                    key: Key::Escape,
                    kind: EventKind::Down,
                },
                OutputEvent {
                    key: Key::Escape,
                    kind: EventKind::Up,
                },
            ]
        );
    }

    #[test]
    fn tap_hold_holds_on_timeout() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::CapsLock), 0);
        let timeout = engine.handle_time(200);
        assert!(timeout.events.is_empty());
        assert_eq!(engine.held_layer_names(), vec!["nav"]);

        let h_down = engine.handle_event_at(down(Key::H), 210);
        assert_eq!(
            h_down.events,
            vec![OutputEvent {
                key: Key::Left,
                kind: EventKind::Down,
            }]
        );

        engine.handle_event_at(up(Key::CapsLock), 220);
        assert!(engine.held_layer_names().is_empty());
    }

    #[test]
    fn tap_hold_holds_on_interrupting_key_down() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::CapsLock), 0);
        let h_down = engine.handle_event_at(down(Key::H), 100);
        assert_eq!(
            h_down.events,
            vec![OutputEvent {
                key: Key::Left,
                kind: EventKind::Down,
            }]
        );
        assert_eq!(engine.held_layer_names(), vec!["nav"]);
    }

    #[test]
    fn tap_hold_ignores_repeated_trigger_down_after_hold() {
        let config = RuntimeConfig::parse(
            r#"
            [settings]
            tap_hold_timeout_ms = 200

            [source]
            keys = ["A"]

            [layers.base]
            A = { tap_hold = { tap = "A", hold = "B" } }
        "#,
        )
        .unwrap();
        let mut engine = Engine::new(config);

        engine.handle_event_at(down(Key::A), 0);
        let timeout = engine.handle_time(200);
        assert_eq!(
            timeout.events,
            vec![OutputEvent {
                key: Key::B,
                kind: EventKind::Down,
            }]
        );

        for now in [210, 240, 270, 300] {
            let repeat = engine.handle_event_at(down(Key::A), now);
            assert!(repeat.consume_input);
            assert!(repeat.events.is_empty());
        }

        let release = engine.handle_event_at(up(Key::A), 320);
        assert_eq!(
            release.events,
            vec![OutputEvent {
                key: Key::B,
                kind: EventKind::Up,
            }]
        );
    }

    #[test]
    fn tap_dance_single_double_and_triple() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::Semicolon), 0);
        let first_up = engine.handle_event_at(up(Key::Semicolon), 50);
        assert_eq!(first_up.next_deadline_ms, Some(250));
        let single = engine.handle_time(250);
        assert_eq!(
            single.events,
            vec![
                OutputEvent {
                    key: Key::Semicolon,
                    kind: EventKind::Down,
                },
                OutputEvent {
                    key: Key::Semicolon,
                    kind: EventKind::Up,
                },
            ]
        );

        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::Semicolon), 0);
        engine.handle_event_at(up(Key::Semicolon), 20);
        engine.handle_event_at(down(Key::Semicolon), 80);
        engine.handle_event_at(up(Key::Semicolon), 100);
        engine.handle_event_at(down(Key::Semicolon), 140);
        let triple = engine.handle_event_at(up(Key::Semicolon), 160);
        assert_eq!(
            triple.events,
            vec![
                OutputEvent {
                    key: Key::Enter,
                    kind: EventKind::Down,
                },
                OutputEvent {
                    key: Key::Enter,
                    kind: EventKind::Up,
                },
            ]
        );
    }

    #[test]
    fn one_shot_modifier_applies_to_next_key() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::LeftShift), 0);
        engine.handle_event_at(up(Key::LeftShift), 10);

        let a_down = engine.handle_event_at(down(Key::A), 20);
        assert_eq!(
            a_down.events,
            vec![
                OutputEvent {
                    key: Key::LeftShift,
                    kind: EventKind::Down,
                },
                OutputEvent {
                    key: Key::A,
                    kind: EventKind::Down,
                },
            ]
        );

        let a_up = engine.handle_event_at(up(Key::A), 30);
        assert_eq!(
            a_up.events,
            vec![
                OutputEvent {
                    key: Key::A,
                    kind: EventKind::Up,
                },
                OutputEvent {
                    key: Key::LeftShift,
                    kind: EventKind::Up,
                },
            ]
        );
    }

    #[test]
    fn one_shot_modifier_expires() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::LeftShift), 0);
        engine.handle_event_at(up(Key::LeftShift), 10);
        engine.handle_time(1000);

        let a_down = engine.handle_event_at(down(Key::A), 1010);
        assert_eq!(
            a_down.events,
            vec![OutputEvent {
                key: Key::A,
                kind: EventKind::Down,
            }]
        );
    }

    #[test]
    fn one_shot_layer_applies_once_and_falls_back_through_transparent() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::F), 0);
        engine.handle_event_at(up(Key::F), 10);

        let h_down = engine.handle_event_at(down(Key::H), 20);
        assert_eq!(
            h_down.events,
            vec![OutputEvent {
                key: Key::Left,
                kind: EventKind::Down,
            }]
        );
        let h_up = engine.handle_event_at(up(Key::H), 30);
        assert_eq!(
            h_up.events,
            vec![OutputEvent {
                key: Key::Left,
                kind: EventKind::Up,
            }]
        );

        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::F), 0);
        engine.handle_event_at(up(Key::F), 10);
        let space_down = engine.handle_event_at(down(Key::Space), 20);
        assert_eq!(
            space_down.events,
            vec![OutputEvent {
                key: Key::Space,
                kind: EventKind::Down,
            }]
        );
    }

    #[test]
    fn reset_releases_active_outputs() {
        let mut engine = feature_engine();
        engine.handle_event_at(down(Key::LeftShift), 0);
        engine.handle_event_at(up(Key::LeftShift), 10);
        engine.handle_event_at(down(Key::A), 20);

        let reset = engine.reset();
        assert_eq!(
            reset.events,
            vec![
                OutputEvent {
                    key: Key::A,
                    kind: EventKind::Up,
                },
                OutputEvent {
                    key: Key::LeftShift,
                    kind: EventKind::Up,
                },
            ]
        );
    }
}
