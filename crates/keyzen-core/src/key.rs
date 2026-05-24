use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Escape,
    Tab,
    CapsLock,
    Space,
    Enter,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Grave,
    Comma,
    Dot,
    Slash,
    Function(u8),
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    LeftMeta,
    RightMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    LeftMeta,
    RightMeta,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyParseError {
    #[error("unknown key name `{0}`")]
    UnknownKey(String),
    #[error("unknown modifier name `{0}`")]
    UnknownModifier(String),
}

impl Key {
    pub fn from_name(name: &str) -> Result<Self, KeyParseError> {
        let normalized = normalize_name(name);
        let key = match normalized.as_str() {
            "a" => Self::A,
            "b" => Self::B,
            "c" => Self::C,
            "d" => Self::D,
            "e" => Self::E,
            "f" => Self::F,
            "g" => Self::G,
            "h" => Self::H,
            "i" => Self::I,
            "j" => Self::J,
            "k" => Self::K,
            "l" => Self::L,
            "m" => Self::M,
            "n" => Self::N,
            "o" => Self::O,
            "p" => Self::P,
            "q" => Self::Q,
            "r" => Self::R,
            "s" => Self::S,
            "t" => Self::T,
            "u" => Self::U,
            "v" => Self::V,
            "w" => Self::W,
            "x" => Self::X,
            "y" => Self::Y,
            "z" => Self::Z,
            "0" | "num0" => Self::Num0,
            "1" | "num1" => Self::Num1,
            "2" | "num2" => Self::Num2,
            "3" | "num3" => Self::Num3,
            "4" | "num4" => Self::Num4,
            "5" | "num5" => Self::Num5,
            "6" | "num6" => Self::Num6,
            "7" | "num7" => Self::Num7,
            "8" | "num8" => Self::Num8,
            "9" | "num9" => Self::Num9,
            "esc" | "escape" => Self::Escape,
            "tab" => Self::Tab,
            "caps" | "capslock" => Self::CapsLock,
            "spc" | "space" => Self::Space,
            "ret" | "enter" => Self::Enter,
            "bspc" | "backspace" => Self::Backspace,
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "home" => Self::Home,
            "end" => Self::End,
            "pgup" | "pageup" => Self::PageUp,
            "pgdn" | "pagedown" => Self::PageDown,
            "ins" | "insert" => Self::Insert,
            "del" | "delete" => Self::Delete,
            "-" | "minus" => Self::Minus,
            "=" | "equal" => Self::Equal,
            "[" | "leftbracket" | "lbracket" => Self::LeftBracket,
            "]" | "rightbracket" | "rbracket" => Self::RightBracket,
            "\\" | "backslash" => Self::Backslash,
            ";" | "semicolon" => Self::Semicolon,
            "'" | "quote" => Self::Quote,
            "`" | "grave" | "grv" => Self::Grave,
            "," | "comma" => Self::Comma,
            "." | "dot" | "period" => Self::Dot,
            "/" | "slash" => Self::Slash,
            "ctrl" | "control" | "lctrl" | "leftctrl" | "lctl" => Self::LeftCtrl,
            "rctrl" | "rightctrl" | "rctl" => Self::RightCtrl,
            "alt" | "lalt" | "leftalt" => Self::LeftAlt,
            "ralt" | "rightalt" | "altgr" => Self::RightAlt,
            "shift" | "lshift" | "leftshift" | "lsft" => Self::LeftShift,
            "rshift" | "rightshift" | "rsft" => Self::RightShift,
            "meta" | "win" | "lmeta" | "leftmeta" | "lmet" => Self::LeftMeta,
            "rmeta" | "rightmeta" | "rmet" => Self::RightMeta,
            _ if normalized.starts_with('f') => {
                let number = normalized[1..]
                    .parse::<u8>()
                    .map_err(|_| KeyParseError::UnknownKey(name.to_owned()))?;
                if (1..=24).contains(&number) {
                    Self::Function(number)
                } else {
                    return Err(KeyParseError::UnknownKey(name.to_owned()));
                }
            }
            _ => return Err(KeyParseError::UnknownKey(name.to_owned())),
        };
        Ok(key)
    }
}

impl Modifier {
    pub fn from_name(name: &str) -> Result<Self, KeyParseError> {
        let modifier = match normalize_name(name).as_str() {
            "ctrl" | "control" | "c" => Self::Ctrl,
            "alt" | "a" => Self::Alt,
            "shift" | "s" => Self::Shift,
            "meta" | "win" | "m" => Self::Meta,
            "lctrl" | "leftctrl" | "lctl" => Self::LeftCtrl,
            "rctrl" | "rightctrl" | "rctl" => Self::RightCtrl,
            "lalt" | "leftalt" => Self::LeftAlt,
            "ralt" | "rightalt" | "altgr" => Self::RightAlt,
            "lshift" | "leftshift" | "lsft" => Self::LeftShift,
            "rshift" | "rightshift" | "rsft" => Self::RightShift,
            "lmeta" | "leftmeta" | "lmet" => Self::LeftMeta,
            "rmeta" | "rightmeta" | "rmet" => Self::RightMeta,
            _ => return Err(KeyParseError::UnknownModifier(name.to_owned())),
        };
        Ok(modifier)
    }

    pub fn output_key(self) -> Key {
        match self {
            Self::Ctrl | Self::LeftCtrl => Key::LeftCtrl,
            Self::RightCtrl => Key::RightCtrl,
            Self::Alt | Self::LeftAlt => Key::LeftAlt,
            Self::RightAlt => Key::RightAlt,
            Self::Shift | Self::LeftShift => Key::LeftShift,
            Self::RightShift => Key::RightShift,
            Self::Meta | Self::LeftMeta => Key::LeftMeta,
            Self::RightMeta => Key::RightMeta,
        }
    }
}

impl FromStr for Key {
    type Err = KeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_name(s)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_name(&raw).map_err(serde::de::Error::custom)
    }
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|ch| !matches!(ch, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}
