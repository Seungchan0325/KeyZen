use crate::config::{Action, BASE_LAYER, Config};
use crate::key::KeyCode;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    KeyDown,
    KeyUp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub key: KeyCode,
    pub kind: EventKind,
    pub time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputCommand {
    KeyDown(KeyCode),
    KeyUp(KeyCode),
    Tap(KeyCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    Warning(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub suppress: bool,
    pub commands: Vec<OutputCommand>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Outcome {
    fn merge_poll(&mut self, poll: Outcome) {
        self.commands.extend(poll.commands);
        self.diagnostics.extend(poll.diagnostics);
    }
}

#[derive(Debug, Clone)]
pub struct Engine {
    config: Config,
    layer_stack: Vec<String>,
    pressed_keys: BTreeSet<KeyCode>,
    suppressed_keys: BTreeSet<KeyCode>,
    repeat_actions: BTreeMap<KeyCode, RepeatAction>,
    held_layers: BTreeMap<KeyCode, String>,
    passthrough_modifiers: BTreeMap<KeyCode, Vec<KeyCode>>,
    pending_tap_holds: BTreeMap<KeyCode, PendingTapHold>,
    dance_down: BTreeMap<KeyCode, DanceDown>,
    pending_tap_dances: BTreeMap<KeyCode, PendingTapDance>,
    one_shots: Vec<PendingOneShot>,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            layer_stack: vec![BASE_LAYER.to_string()],
            pressed_keys: BTreeSet::new(),
            suppressed_keys: BTreeSet::new(),
            repeat_actions: BTreeMap::new(),
            held_layers: BTreeMap::new(),
            passthrough_modifiers: BTreeMap::new(),
            pending_tap_holds: BTreeMap::new(),
            dance_down: BTreeMap::new(),
            pending_tap_dances: BTreeMap::new(),
            one_shots: Vec::new(),
        }
    }

    pub fn active_layers(&self) -> &[String] {
        &self.layer_stack
    }

    pub fn next_deadline_ms(&self) -> Option<u64> {
        let tap_hold_deadline = self
            .pending_tap_holds
            .values()
            .filter(|pending| pending.state == TapHoldState::Pending)
            .map(|pending| pending.deadline_ms)
            .min();
        let tap_dance_deadline = self
            .pending_tap_dances
            .values()
            .map(|pending| pending.deadline_ms)
            .min();
        let one_shot_deadline = self
            .one_shots
            .iter()
            .map(|pending| pending.deadline_ms)
            .min();

        [tap_hold_deadline, tap_dance_deadline, one_shot_deadline]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn poll(&mut self, now_ms: u64) -> Outcome {
        let mut outcome = Outcome::default();
        self.expire_one_shots(now_ms);
        self.resolve_due_tap_holds(now_ms, &mut outcome);
        self.resolve_due_tap_dances(now_ms, &mut outcome);
        outcome
    }

    pub fn handle_event(&mut self, event: Event) -> Outcome {
        let mut outcome = Outcome::default();
        outcome.merge_poll(self.poll(event.time_ms));

        match event.kind {
            EventKind::KeyDown => self.handle_key_down(event.key, event.time_ms, &mut outcome),
            EventKind::KeyUp => self.handle_key_up(event.key, event.time_ms, &mut outcome),
        }

        outcome
    }

    fn handle_key_down(&mut self, key: KeyCode, now_ms: u64, outcome: &mut Outcome) {
        let is_repeat = !self.pressed_keys.insert(key.clone());
        if is_repeat {
            if let Some(repeat) = self.repeat_actions.get(&key).cloned() {
                self.execute_action_with_modifiers(
                    &repeat.action,
                    &repeat.modifiers,
                    now_ms,
                    outcome,
                );
                outcome.suppress = true;
                return;
            }
            if self.passthrough_modifiers.contains_key(&key) {
                outcome.commands.push(OutputCommand::KeyDown(key));
                outcome.suppress = true;
                return;
            }
            if self.suppressed_keys.contains(&key)
                || self.pending_tap_holds.contains_key(&key)
                || self.held_layers.contains_key(&key)
            {
                outcome.suppress = true;
            }
            return;
        }

        self.resolve_interrupted_tap_holds(&key, now_ms, outcome);
        self.resolve_interrupted_tap_dances(&key, now_ms, outcome);

        if let Some(pending) = self.pending_tap_dances.get(&key).cloned() {
            self.dance_down.insert(
                key.clone(),
                DanceDown {
                    actions: pending.actions,
                    modifiers: pending.modifiers,
                },
            );
            self.suppressed_keys.insert(key);
            outcome.suppress = true;
            return;
        }

        let Some(action) = self.resolve_action(&key) else {
            self.handle_unmapped_key_down(key, outcome);
            return;
        };

        if action.is_one_shot_trigger() {
            self.execute_action(&action, now_ms, outcome);
            self.suppressed_keys.insert(key);
            outcome.suppress = true;
            return;
        }

        let modifiers = self.consume_one_shot_modifiers();
        self.consume_one_shot_layers();

        match action {
            Action::Noop => {
                self.suppressed_keys.insert(key);
                outcome.suppress = true;
            }
            Action::TapHold { tap, hold } => {
                self.pending_tap_holds.insert(
                    key.clone(),
                    PendingTapHold {
                        tap: *tap,
                        hold: *hold,
                        modifiers,
                        deadline_ms: now_ms + self.config.settings.tapping_term_ms,
                        state: TapHoldState::Pending,
                        release: HoldRelease::None,
                    },
                );
                self.suppressed_keys.insert(key);
                outcome.suppress = true;
            }
            Action::TapDance(actions) => {
                self.dance_down
                    .insert(key.clone(), DanceDown { actions, modifiers });
                self.suppressed_keys.insert(key);
                outcome.suppress = true;
            }
            action => {
                let held_layer = match &action {
                    Action::LayerWhileHeld(layer) => Some(layer.clone()),
                    _ => None,
                };
                if action.is_repeatable_output() {
                    self.repeat_actions.insert(
                        key.clone(),
                        RepeatAction {
                            action: action.clone(),
                            modifiers: modifiers.clone(),
                        },
                    );
                }
                self.execute_action_with_modifiers(&action, &modifiers, now_ms, outcome);
                if let Some(layer) = held_layer {
                    self.held_layers.insert(key.clone(), layer);
                }
                self.suppressed_keys.insert(key);
                outcome.suppress = true;
            }
        }
    }

    fn handle_key_up(&mut self, key: KeyCode, now_ms: u64, outcome: &mut Outcome) {
        self.pressed_keys.remove(&key);

        if let Some(pending) = self.pending_tap_holds.remove(&key) {
            match pending.state {
                TapHoldState::Pending => {
                    self.execute_action_with_modifiers(
                        &pending.tap,
                        &pending.modifiers,
                        now_ms,
                        outcome,
                    );
                }
                TapHoldState::Holding => {
                    self.release_hold(pending.release, outcome);
                }
            }
            self.suppressed_keys.remove(&key);
            self.repeat_actions.remove(&key);
            outcome.suppress = true;
            return;
        }

        if let Some(down) = self.dance_down.remove(&key) {
            let pending = self
                .pending_tap_dances
                .entry(key.clone())
                .or_insert_with(|| PendingTapDance {
                    count: 0,
                    actions: down.actions.clone(),
                    modifiers: down.modifiers.clone(),
                    deadline_ms: now_ms + self.config.settings.tap_dance_term_ms,
                });
            pending.count = pending.count.saturating_add(1);
            pending.actions = down.actions;
            pending.modifiers = down.modifiers;
            pending.deadline_ms = now_ms + self.config.settings.tap_dance_term_ms;
            outcome.suppress = true;
            return;
        }

        if let Some(modifiers) = self.passthrough_modifiers.remove(&key) {
            outcome.commands.push(OutputCommand::KeyUp(key.clone()));
            for modifier in modifiers.iter().rev() {
                outcome
                    .commands
                    .push(OutputCommand::KeyUp(modifier.clone()));
            }
            outcome.suppress = true;
            return;
        }

        if let Some(layer) = self.held_layers.remove(&key) {
            self.pop_layer(Some(&layer));
            self.suppressed_keys.remove(&key);
            outcome.suppress = true;
            return;
        }

        self.repeat_actions.remove(&key);
        if self.suppressed_keys.remove(&key) {
            outcome.suppress = true;
        }
    }

    fn handle_unmapped_key_down(&mut self, key: KeyCode, outcome: &mut Outcome) {
        let modifiers = self.consume_one_shot_modifiers();
        self.consume_one_shot_layers();

        if modifiers.is_empty() {
            outcome.suppress = false;
            return;
        }

        for modifier in &modifiers {
            outcome
                .commands
                .push(OutputCommand::KeyDown(modifier.clone()));
        }
        outcome.commands.push(OutputCommand::KeyDown(key.clone()));
        self.passthrough_modifiers.insert(key, modifiers);
        outcome.suppress = true;
    }

    fn resolve_action(&self, key: &KeyCode) -> Option<Action> {
        for layer_name in self.active_one_shot_layers().iter().rev() {
            if let Some(action) = self
                .config
                .layers
                .get(layer_name)
                .and_then(|layer| layer.get(key))
            {
                return Some(action.clone());
            }
        }
        for layer_name in self.layer_stack.iter().rev() {
            if let Some(action) = self
                .config
                .layers
                .get(layer_name)
                .and_then(|layer| layer.get(key))
            {
                return Some(action.clone());
            }
        }
        None
    }

    fn resolve_interrupted_tap_holds(
        &mut self,
        interrupting_key: &KeyCode,
        now_ms: u64,
        outcome: &mut Outcome,
    ) {
        let keys = self
            .pending_tap_holds
            .iter()
            .filter(|(key, pending)| {
                *key != interrupting_key && pending.state == TapHoldState::Pending
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();

        for key in keys {
            self.start_tap_hold(&key, now_ms, outcome);
        }
    }

    fn resolve_due_tap_holds(&mut self, now_ms: u64, outcome: &mut Outcome) {
        let keys = self
            .pending_tap_holds
            .iter()
            .filter(|(_, pending)| {
                pending.state == TapHoldState::Pending && pending.deadline_ms <= now_ms
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();

        for key in keys {
            self.start_tap_hold(&key, now_ms, outcome);
        }
    }

    fn start_tap_hold(&mut self, key: &KeyCode, now_ms: u64, outcome: &mut Outcome) {
        let Some(pending) = self.pending_tap_holds.get(key) else {
            return;
        };
        if pending.state != TapHoldState::Pending {
            return;
        }

        let hold = pending.hold.clone();
        let modifiers = pending.modifiers.clone();
        let release = self.execute_hold_start(&hold, &modifiers, now_ms, outcome);

        if let Some(pending) = self.pending_tap_holds.get_mut(key) {
            pending.state = TapHoldState::Holding;
            pending.release = release;
        }
    }

    fn resolve_interrupted_tap_dances(
        &mut self,
        interrupting_key: &KeyCode,
        now_ms: u64,
        outcome: &mut Outcome,
    ) {
        let keys = self
            .pending_tap_dances
            .keys()
            .filter(|key| *key != interrupting_key)
            .cloned()
            .collect::<Vec<_>>();

        for key in keys {
            self.resolve_tap_dance(&key, now_ms, outcome);
        }
    }

    fn resolve_due_tap_dances(&mut self, now_ms: u64, outcome: &mut Outcome) {
        let keys = self
            .pending_tap_dances
            .iter()
            .filter(|(_, pending)| pending.deadline_ms <= now_ms)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();

        for key in keys {
            self.resolve_tap_dance(&key, now_ms, outcome);
        }
    }

    fn resolve_tap_dance(&mut self, key: &KeyCode, now_ms: u64, outcome: &mut Outcome) {
        let Some(pending) = self.pending_tap_dances.remove(key) else {
            return;
        };

        if let Some(action) = pending.actions.get(&pending.count) {
            self.execute_action_with_modifiers(action, &pending.modifiers, now_ms, outcome);
        } else {
            outcome.diagnostics.push(Diagnostic::Warning(format!(
                "tap dance on `{key}` has no action for {} taps",
                pending.count
            )));
        }

        self.suppressed_keys.remove(key);
    }

    fn execute_action_with_modifiers(
        &mut self,
        action: &Action,
        modifiers: &[KeyCode],
        now_ms: u64,
        outcome: &mut Outcome,
    ) {
        for modifier in modifiers {
            outcome
                .commands
                .push(OutputCommand::KeyDown(modifier.clone()));
        }
        self.execute_action(action, now_ms, outcome);
        for modifier in modifiers.iter().rev() {
            outcome
                .commands
                .push(OutputCommand::KeyUp(modifier.clone()));
        }
    }

    fn execute_action(&mut self, action: &Action, now_ms: u64, outcome: &mut Outcome) {
        match action {
            Action::Noop => {}
            Action::Send(key) => outcome.commands.push(OutputCommand::Tap(key.clone())),
            Action::Chord(keys) => {
                for key in keys {
                    outcome.commands.push(OutputCommand::KeyDown(key.clone()));
                }
                for key in keys.iter().rev() {
                    outcome.commands.push(OutputCommand::KeyUp(key.clone()));
                }
            }
            Action::LayerWhileHeld(layer) => self.push_layer(layer),
            Action::LayerToggle(layer) => self.toggle_layer(layer),
            Action::TapHold { .. } | Action::TapDance(_) => {
                outcome.diagnostics.push(Diagnostic::Warning(
                    "nested temporal action ignored at runtime".to_string(),
                ));
            }
            Action::OneShotLayer(layer) => self.add_one_shot(
                OneShotKind::Layer(layer.clone()),
                now_ms + self.config.settings.one_shot_timeout_ms,
            ),
            Action::OneShotModifier(key) => self.add_one_shot(
                OneShotKind::Modifier(key.clone()),
                now_ms + self.config.settings.one_shot_timeout_ms,
            ),
        }
    }

    fn execute_hold_start(
        &mut self,
        action: &Action,
        modifiers: &[KeyCode],
        now_ms: u64,
        outcome: &mut Outcome,
    ) -> HoldRelease {
        match action {
            Action::Noop => HoldRelease::None,
            Action::Send(key) => {
                for modifier in modifiers {
                    outcome
                        .commands
                        .push(OutputCommand::KeyDown(modifier.clone()));
                }
                outcome.commands.push(OutputCommand::KeyDown(key.clone()));
                let mut release = Vec::with_capacity(modifiers.len() + 1);
                release.push(OutputCommand::KeyUp(key.clone()));
                for modifier in modifiers.iter().rev() {
                    release.push(OutputCommand::KeyUp(modifier.clone()));
                }
                HoldRelease::Commands(release)
            }
            Action::Chord(keys) => {
                for modifier in modifiers {
                    outcome
                        .commands
                        .push(OutputCommand::KeyDown(modifier.clone()));
                }
                for key in keys {
                    outcome.commands.push(OutputCommand::KeyDown(key.clone()));
                }
                let mut release = Vec::with_capacity(modifiers.len() + keys.len());
                for key in keys.iter().rev() {
                    release.push(OutputCommand::KeyUp(key.clone()));
                }
                for modifier in modifiers.iter().rev() {
                    release.push(OutputCommand::KeyUp(modifier.clone()));
                }
                HoldRelease::Commands(release)
            }
            Action::LayerWhileHeld(layer) => {
                self.push_layer(layer);
                HoldRelease::Layer(layer.clone())
            }
            Action::LayerToggle(layer) => {
                self.toggle_layer(layer);
                HoldRelease::None
            }
            Action::OneShotLayer(_) | Action::OneShotModifier(_) => {
                self.execute_action(action, now_ms, outcome);
                HoldRelease::None
            }
            Action::TapHold { .. } | Action::TapDance(_) => {
                outcome.diagnostics.push(Diagnostic::Warning(
                    "nested temporal hold action ignored at runtime".to_string(),
                ));
                HoldRelease::None
            }
        }
    }

    fn release_hold(&mut self, release: HoldRelease, outcome: &mut Outcome) {
        match release {
            HoldRelease::None => {}
            HoldRelease::Layer(layer) => self.pop_layer(Some(&layer)),
            HoldRelease::Commands(commands) => outcome.commands.extend(commands),
        }
    }

    fn push_layer(&mut self, layer: &str) {
        if layer != BASE_LAYER {
            self.layer_stack.push(layer.to_string());
        }
    }

    fn pop_layer(&mut self, layer: Option<&str>) {
        match layer {
            Some(BASE_LAYER) => {}
            Some(layer) => {
                if let Some(index) = self.layer_stack.iter().rposition(|entry| entry == layer) {
                    if index > 0 {
                        self.layer_stack.remove(index);
                    }
                }
            }
            None => {
                if self.layer_stack.len() > 1 {
                    self.layer_stack.pop();
                }
            }
        }
    }

    fn toggle_layer(&mut self, layer: &str) {
        if layer == BASE_LAYER {
            return;
        }

        let before = self.layer_stack.len();
        self.layer_stack.retain(|entry| entry != layer);
        if self.layer_stack.len() == before {
            self.layer_stack.push(layer.to_string());
        }
    }

    fn active_one_shot_layers(&self) -> Vec<String> {
        self.one_shots
            .iter()
            .filter_map(|pending| match &pending.kind {
                OneShotKind::Layer(layer) => Some(layer.clone()),
                OneShotKind::Modifier(_) => None,
            })
            .collect()
    }

    fn consume_one_shot_modifiers(&mut self) -> Vec<KeyCode> {
        let mut modifiers = Vec::new();
        self.one_shots.retain(|pending| match &pending.kind {
            OneShotKind::Modifier(key) => {
                modifiers.push(key.clone());
                false
            }
            OneShotKind::Layer(_) => true,
        });
        modifiers
    }

    fn consume_one_shot_layers(&mut self) {
        self.one_shots
            .retain(|pending| !matches!(pending.kind, OneShotKind::Layer(_)));
    }

    fn add_one_shot(&mut self, kind: OneShotKind, deadline_ms: u64) {
        self.one_shots.retain(|pending| pending.kind != kind);
        self.one_shots.push(PendingOneShot { kind, deadline_ms });
    }

    fn expire_one_shots(&mut self, now_ms: u64) {
        self.one_shots
            .retain(|pending| pending.deadline_ms > now_ms);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepeatAction {
    action: Action,
    modifiers: Vec<KeyCode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTapHold {
    tap: Action,
    hold: Action,
    modifiers: Vec<KeyCode>,
    deadline_ms: u64,
    state: TapHoldState,
    release: HoldRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapHoldState {
    Pending,
    Holding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HoldRelease {
    None,
    Layer(String),
    Commands(Vec<OutputCommand>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DanceDown {
    actions: BTreeMap<u8, Action>,
    modifiers: Vec<KeyCode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTapDance {
    count: u8,
    actions: BTreeMap<u8, Action>,
    modifiers: Vec<KeyCode>,
    deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingOneShot {
    kind: OneShotKind,
    deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OneShotKind {
    Layer(String),
    Modifier(KeyCode),
}

impl Action {
    fn is_one_shot_trigger(&self) -> bool {
        matches!(self, Self::OneShotLayer(_) | Self::OneShotModifier(_))
    }

    fn is_repeatable_output(&self) -> bool {
        matches!(self, Self::Send(_) | Self::Chord(_))
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Engine, Event, EventKind, OutputCommand};
    use crate::Config;

    fn config(input: &str) -> Config {
        Config::from_yaml_str(input).unwrap()
    }

    fn key(input: &str) -> crate::KeyCode {
        input.parse().unwrap()
    }

    fn down(input: &str, time_ms: u64) -> Event {
        Event {
            key: key(input),
            kind: EventKind::KeyDown,
            time_ms,
        }
    }

    fn up(input: &str, time_ms: u64) -> Event {
        Event {
            key: key(input),
            kind: EventKind::KeyUp,
            time_ms,
        }
    }

    #[test]
    fn resolves_layer_stack_from_top_down_and_falls_back_to_base() {
        let mut engine = Engine::new(config(
            r#"
layers:
  base:
    A: B
    CapsLock:
      layer_while_held: nav
  nav:
    A: Left
"#,
        ));

        assert_eq!(
            engine.handle_event(down("CapsLock", 0)).commands,
            Vec::<OutputCommand>::new()
        );
        assert_eq!(
            engine.active_layers(),
            &["base".to_string(), "nav".to_string()]
        );

        let nav = engine.handle_event(down("A", 10));
        assert_eq!(nav.commands, vec![OutputCommand::Tap(key("Left"))]);
        assert!(nav.suppress);
        assert!(engine.handle_event(up("A", 20)).suppress);

        engine.handle_event(up("CapsLock", 30));
        assert_eq!(engine.active_layers(), &["base".to_string()]);

        let base = engine.handle_event(down("A", 40));
        assert_eq!(base.commands, vec![OutputCommand::Tap(key("B"))]);
    }

    #[test]
    fn toggles_layers_without_duplicates() {
        let mut engine = Engine::new(config(
            r#"
layers:
  base:
    T:
      layer_toggle: nav
  nav:
    A: Left
"#,
        ));

        engine.handle_event(down("T", 0));
        engine.handle_event(up("T", 1));
        engine.handle_event(down("T", 2));
        engine.handle_event(up("T", 3));
        engine.handle_event(down("T", 4));

        assert_eq!(
            engine.active_layers(),
            &["base".to_string(), "nav".to_string()]
        );
    }

    #[test]
    fn noop_blocks_fallback_and_suppresses_key_events() {
        let mut engine = Engine::new(config(
            r#"
layers:
  base:
    A: B
    C: D
    CapsLock:
      layer_while_held: nav
  nav:
    A:
      noop: true
"#,
        ));

        engine.handle_event(down("CapsLock", 0));

        let blocked_down = engine.handle_event(down("A", 10));
        assert!(blocked_down.suppress);
        assert!(blocked_down.commands.is_empty());

        let blocked_up = engine.handle_event(up("A", 20));
        assert!(blocked_up.suppress);
        assert!(blocked_up.commands.is_empty());

        let fallback = engine.handle_event(down("C", 30));
        assert!(fallback.suppress);
        assert_eq!(fallback.commands, vec![OutputCommand::Tap(key("D"))]);
    }

    #[test]
    fn noop_can_be_used_inside_temporal_actions() {
        let mut engine = Engine::new(config(
            r#"
settings:
  tap_dance_term_ms: 100
layers:
  base:
    A:
      tap_hold:
        tap:
          noop: true
        hold:
          noop: true
    B:
      tap_dance:
        1:
          noop: true
"#,
        ));

        engine.handle_event(down("A", 0));
        let tap_hold = engine.handle_event(up("A", 50));
        assert!(tap_hold.suppress);
        assert!(tap_hold.commands.is_empty());

        engine.handle_event(down("B", 100));
        engine.handle_event(up("B", 110));
        let tap_dance = engine.poll(211);
        assert!(tap_dance.commands.is_empty());
    }

    #[test]
    fn tap_hold_taps_when_released_before_deadline() {
        let mut engine = Engine::new(config(
            r#"
layers:
  base:
    CapsLock:
      tap_hold:
        tap: Escape
        hold:
          layer_while_held: nav
  nav:
    H: Left
"#,
        ));

        let down = engine.handle_event(down("CapsLock", 0));
        assert!(down.suppress);
        assert!(down.commands.is_empty());

        let up = engine.handle_event(up("CapsLock", 100));
        assert_eq!(up.commands, vec![OutputCommand::Tap(key("Escape"))]);
        assert_eq!(engine.active_layers(), &["base".to_string()]);
    }

    #[test]
    fn tap_hold_holds_when_timeout_expires() {
        let mut engine = Engine::new(config(
            r#"
settings:
  tapping_term_ms: 200
layers:
  base:
    CapsLock:
      tap_hold:
        tap: Escape
        hold:
          layer_while_held: nav
  nav:
    H: Left
"#,
        ));

        engine.handle_event(down("CapsLock", 0));
        engine.poll(201);
        assert_eq!(
            engine.active_layers(),
            &["base".to_string(), "nav".to_string()]
        );

        engine.handle_event(up("CapsLock", 250));
        assert_eq!(engine.active_layers(), &["base".to_string()]);
    }

    #[test]
    fn tap_hold_holds_when_interrupted_by_another_key() {
        let mut engine = Engine::new(config(
            r#"
settings:
  tapping_term_ms: 200
layers:
  base:
    CapsLock:
      tap_hold:
        tap: Escape
        hold:
          layer_while_held: nav
  nav:
    H: Left
"#,
        ));

        engine.handle_event(down("CapsLock", 0));
        let outcome = engine.handle_event(down("H", 50));
        assert_eq!(outcome.commands, vec![OutputCommand::Tap(key("Left"))]);
        assert!(outcome.suppress);
    }

    #[test]
    fn tap_dance_resolves_on_timeout() {
        let mut engine = Engine::new(config(
            r#"
settings:
  tap_dance_term_ms: 180
layers:
  base:
    Quote:
      tap_dance:
        1: Quote
        2: DoubleQuote
"#,
        ));

        engine.handle_event(down("Quote", 0));
        engine.handle_event(up("Quote", 20));
        engine.handle_event(down("Quote", 60));
        engine.handle_event(up("Quote", 80));

        let outcome = engine.poll(261);
        assert_eq!(
            outcome.commands,
            vec![OutputCommand::Tap(key("DoubleQuote"))]
        );
    }

    #[test]
    fn tap_dance_warns_on_unmatched_count() {
        let mut engine = Engine::new(config(
            r#"
layers:
  base:
    Quote:
      tap_dance:
        2: DoubleQuote
"#,
        ));

        engine.handle_event(down("Quote", 0));
        engine.handle_event(up("Quote", 10));
        let outcome = engine.poll(1_000);

        assert_eq!(outcome.commands, Vec::<OutputCommand>::new());
        assert!(matches!(
            outcome.diagnostics.as_slice(),
            [Diagnostic::Warning(message)] if message.contains("no action")
        ));
    }

    #[test]
    fn one_shot_modifier_applies_to_next_mapped_key() {
        let mut engine = Engine::new(config(
            r#"
layers:
  base:
    LeftShift:
      one_shot_modifier: Shift
    A: A
"#,
        ));

        engine.handle_event(down("LeftShift", 0));
        engine.handle_event(up("LeftShift", 1));
        let outcome = engine.handle_event(down("A", 100));

        assert_eq!(
            outcome.commands,
            vec![
                OutputCommand::KeyDown(key("Shift")),
                OutputCommand::Tap(key("A")),
                OutputCommand::KeyUp(key("Shift")),
            ]
        );
    }

    #[test]
    fn one_shot_layer_applies_to_next_key_and_expires() {
        let mut engine = Engine::new(config(
            r#"
settings:
  one_shot_timeout_ms: 100
layers:
  base:
    O:
      one_shot_layer: nav
    H: H
  nav:
    H: Left
"#,
        ));

        engine.handle_event(down("O", 0));
        engine.handle_event(up("O", 1));
        assert_eq!(
            engine.handle_event(down("H", 50)).commands,
            vec![OutputCommand::Tap(key("Left"))]
        );
        engine.handle_event(up("H", 51));

        engine.handle_event(down("O", 200));
        engine.handle_event(up("O", 201));
        engine.poll(301);
        assert_eq!(
            engine.handle_event(down("H", 350)).commands,
            vec![OutputCommand::Tap(key("H"))]
        );
    }
}
