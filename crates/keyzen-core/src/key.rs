use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyCode(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid key name `{0}`")]
pub struct KeyParseError(String);

impl KeyCode {
    pub fn new(raw: impl AsRef<str>) -> Result<Self, KeyParseError> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(KeyParseError(raw.to_string()));
        }

        Ok(Self(canonical_key_name(raw)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for KeyCode {
    type Err = KeyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl Serialize for KeyCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for KeyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(de::Error::custom)
    }
}

fn canonical_key_name(raw: &str) -> String {
    let compact = raw
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '_')
        .collect::<String>()
        .to_ascii_lowercase();

    if compact.len() == 1 {
        let ch = compact.chars().next().expect("checked len");
        if ch.is_ascii_alphabetic() {
            return ch.to_ascii_uppercase().to_string();
        }
        if ch.is_ascii_digit() {
            return ch.to_string();
        }
    }

    if compact.starts_with('f') && compact.len() <= 3 {
        if let Ok(number) = compact[1..].parse::<u8>() {
            if (1..=24).contains(&number) {
                return format!("F{number}");
            }
        }
    }

    match compact.as_str() {
        "esc" | "escape" => "Escape",
        "caps" | "capslock" => "CapsLock",
        "space" | "spacebar" => "Space",
        "tab" => "Tab",
        "enter" | "return" => "Enter",
        "backspace" | "bksp" => "Backspace",
        "delete" | "del" => "Delete",
        "insert" | "ins" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" | "pgup" => "PageUp",
        "pagedown" | "pgdn" => "PageDown",
        "left" | "arrowleft" => "Left",
        "right" | "arrowright" => "Right",
        "up" | "arrowup" => "Up",
        "down" | "arrowdown" => "Down",
        "shift" => "Shift",
        "leftshift" | "lshift" => "LeftShift",
        "rightshift" | "rshift" => "RightShift",
        "ctrl" | "control" => "Ctrl",
        "leftctrl" | "leftcontrol" | "lctrl" | "lcontrol" => "LeftCtrl",
        "rightctrl" | "rightcontrol" | "rctrl" | "rcontrol" => "RightCtrl",
        "alt" => "Alt",
        "leftalt" | "lalt" => "LeftAlt",
        "rightalt" | "ralt" | "altgr" => "RightAlt",
        "win" | "meta" | "super" => "Win",
        "leftwin" | "lwin" | "leftmeta" => "LeftWin",
        "rightwin" | "rwin" | "rightmeta" => "RightWin",
        "menu" | "apps" | "application" => "Menu",
        "printscreen" | "prtsc" => "PrintScreen",
        "scrolllock" => "ScrollLock",
        "pause" | "break" => "Pause",
        "numlock" => "NumLock",
        "quote" | "apostrophe" | "'" => "Quote",
        "doublequote" | "\"" => "DoubleQuote",
        "semicolon" | ";" => "Semicolon",
        "colon" | ":" => "Colon",
        "comma" | "," => "Comma",
        "less" | "lessthan" | "<" => "LessThan",
        "period" | "dot" | "." => "Period",
        "greater" | "greaterthan" | ">" => "GreaterThan",
        "slash" | "/" => "Slash",
        "question" | "questionmark" | "?" => "Question",
        "backslash" | "\\" => "Backslash",
        "pipe" | "|" => "Pipe",
        "minus" | "hyphen" | "-" => "Minus",
        "underscore" | "_" => "Underscore",
        "equal" | "equals" | "=" => "Equal",
        "plus" | "+" => "Plus",
        "grave" | "backtick" | "`" => "Grave",
        "tilde" | "~" => "Tilde",
        "leftbracket" | "[" => "LeftBracket",
        "leftbrace" | "{" => "LeftBrace",
        "rightbracket" | "]" => "RightBracket",
        "rightbrace" | "}" => "RightBrace",
        "bang" | "exclamation" | "exclamationmark" | "!" => "Exclamation",
        "at" | "atsign" | "@" => "At",
        "hash" | "pound" | "#" => "Hash",
        "dollar" | "$" => "Dollar",
        "percent" | "%" => "Percent",
        "caret" | "^" => "Caret",
        "ampersand" | "&" => "Ampersand",
        "asterisk" | "star" | "*" => "Asterisk",
        "leftparen" | "(" => "LeftParen",
        "rightparen" | ")" => "RightParen",
        _ => raw.trim(),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::KeyCode;

    #[test]
    fn normalizes_common_aliases() {
        assert_eq!(KeyCode::new("esc").unwrap().as_str(), "Escape");
        assert_eq!(KeyCode::new("left-shift").unwrap().as_str(), "LeftShift");
        assert_eq!(KeyCode::new("a").unwrap().as_str(), "A");
        assert_eq!(KeyCode::new("f12").unwrap().as_str(), "F12");
        assert_eq!(
            KeyCode::new("double_quote").unwrap().as_str(),
            "DoubleQuote"
        );
    }
}
