# KeyZen Keymap 설정 파일 작성 가이드

KeyZen의 keymap 파일은 TOML 형식입니다. 기본 위치는 Windows에서 보통 `%APPDATA%\KeyZen\keyzen.toml`이며, 실제로 사용할 keymap 경로는 `%APPDATA%\KeyZen\config.toml`의 `keymap_path` 값으로 정합니다.

KeyZen은 키보드 레이어 엔진입니다. keymap에는 키 치환, 단축키 조합, 레이어 전환 규칙만 적습니다. 텍스트 매크로, 프로그램 실행, 지연 동작, 마우스 자동화, tap-hold, tap-dance, one-shot 키는 MVP 범위가 아닙니다.

## 기본 구조

```toml
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
```

## `[settings]`

`process_unmapped_keys`는 keymap에 매핑되지 않은 키를 KeyZen이 처리할지 정합니다.

- `false`: `[source].keys`에 포함된 키만 KeyZen이 관찰하고 처리합니다. 일반적으로 권장되는 기본값입니다.
- `true`: 매핑되지 않은 키도 원래 키 입력으로 통과시키는 대상으로 취급합니다.

`startup_layer`는 KeyZen이 시작할 때 사용할 기본 레이어 이름입니다. 생략하면 `"base"`가 기본값입니다. 지정한 레이어는 반드시 `[layers.<name>]`로 존재해야 합니다.

## `[source]`

`keys`에는 KeyZen이 가로챌 입력 키를 적습니다.

```toml
[source]
keys = ["CapsLock", "H", "J", "K", "L"]
```

레이어에서 어떤 키를 매핑하더라도, `process_unmapped_keys = false`인 경우에는 해당 입력 키가 `[source].keys`에 들어 있어야 안정적으로 처리됩니다. 레이어 키, 이동 키, 특수 동작을 붙일 키를 모두 여기에 포함하세요.

## `[layers.<name>]`

레이어는 입력 키별 동작을 정의합니다. 예를 들어 `[layers.base]`는 `base` 레이어이고 `[layers.nav]`는 `nav` 레이어입니다.

```toml
[layers.base]
CapsLock = { layer_while_held = "nav" }

[layers.nav]
H = "Left"
J = "Down"
K = "Up"
L = "Right"
```

동시에 여러 레이어가 눌려 있을 때는 나중에 눌린 held layer가 먼저 적용됩니다. held layer에서 `transparent`를 만나면 아래 레이어의 매핑을 계속 찾습니다.

## 동작 작성법

단일 키 출력:

```toml
H = "Left"
Space = "Space"
Escape = "Escape"
```

수식어 조합:

```toml
B = "Ctrl+Left"
W = "Ctrl+Right"
Delete = "Ctrl+Alt+Delete"
```

조합은 마지막 항목이 출력 키이고, 앞 항목들이 수식어입니다. 같은 동작을 명시형 배열로도 쓸 수 있습니다.

```toml
B = { chord = ["Ctrl", "Left"] }
```

키를 누르고 있는 동안 레이어 활성화:

```toml
CapsLock = { layer_while_held = "nav" }
```

키를 누르면 기본 레이어 전환:

```toml
F1 = { layer_switch = "base" }
F2 = { layer_switch = "symbols" }
```

투명 처리:

```toml
Space = "transparent"
```

`transparent`는 현재 레이어에서 동작을 정하지 않고 아래 레이어의 매핑을 사용하게 합니다. 주로 held layer에서 일부 키만 원래 레이어 동작으로 돌려보낼 때 씁니다.

아무 동작도 하지 않기:

```toml
CapsLock = "noop"
```

`noop`는 입력을 소비하고 아무 키도 출력하지 않습니다.

명시형 단일 키 출력:

```toml
H = { key = "Left" }
```

명시형 동작은 `key`, `chord`, `layer_while_held`, `layer_switch` 중 정확히 하나만 포함해야 합니다.

## 지원 키 이름

키 이름은 대소문자를 구분하지 않으며 `_`, `-`, 공백은 무시합니다. 예를 들어 `CapsLock`, `caps_lock`, `caps-lock`, `caps lock`은 같은 키로 해석됩니다.

문자와 숫자:

```text
A-Z
0-9 또는 Num0-Num9
```

기본 특수 키:

```text
Escape 또는 Esc
Tab
CapsLock 또는 Caps
Space 또는 Spc
Enter 또는 Ret
Backspace 또는 Bspc
Left, Right, Up, Down
Home, End, PageUp 또는 PgUp, PageDown 또는 PgDn
Insert 또는 Ins
Delete 또는 Del
F1-F24
```

기호 키:

```text
Minus 또는 -
Equal 또는 =
LeftBracket 또는 LBracket 또는 [
RightBracket 또는 RBracket 또는 ]
Backslash 또는 \
Semicolon 또는 ;
Quote 또는 '
Grave 또는 Grv 또는 `
Comma 또는 ,
Dot 또는 Period 또는 .
Slash 또는 /
```

수식어 키를 단일 키로 출력하거나 입력 키로 사용할 때:

```text
LeftCtrl 또는 LCtrl 또는 Ctrl
RightCtrl 또는 RCtrl
LeftAlt 또는 LAlt 또는 Alt
RightAlt 또는 RAlt 또는 AltGr
LeftShift 또는 LShift 또는 Shift
RightShift 또는 RShift
LeftMeta 또는 LMeta 또는 Win 또는 Meta
RightMeta 또는 RMeta
```

조합의 수식어 이름:

```text
Ctrl 또는 Control 또는 C
Alt 또는 A
Shift 또는 S
Meta 또는 Win 또는 M
LeftCtrl 또는 LCtrl
RightCtrl 또는 RCtrl
LeftAlt 또는 LAlt
RightAlt 또는 RAlt 또는 AltGr
LeftShift 또는 LShift
RightShift 또는 RShift
LeftMeta 또는 LMeta
RightMeta 또는 RMeta
```

`Ctrl`, `Alt`, `Shift`, `Meta`처럼 좌우를 지정하지 않은 수식어는 각각 왼쪽 수식어로 출력됩니다.

## 예시: Vim 스타일 이동 레이어

```toml
[settings]
process_unmapped_keys = false
startup_layer = "base"

[source]
keys = [
    "CapsLock",
    "H",
    "J",
    "K",
    "L",
    "U",
    "D",
    "B",
    "W",
    "G",
    "I",
    "A",
    "X",
    "Escape",
    "Space",
]

[layers.base]
CapsLock = { layer_while_held = "vim" }

[layers.vim]
H = "Left"
J = "Down"
K = "Up"
L = "Right"
U = "PageUp"
D = "PageDown"
B = "Ctrl+Left"
W = "Ctrl+Right"
G = "Home"
I = "End"
A = "End"
X = "Delete"
Escape = "Escape"
Space = "transparent"
```

## 작성 팁

- 처음에는 `process_unmapped_keys = false`로 두고, KeyZen이 처리할 키를 `[source].keys`에 명시하세요.
- `startup_layer`와 `layer_while_held`, `layer_switch`가 가리키는 레이어 이름이 실제로 존재하는지 확인하세요.
- 레이어 전환 키 자체도 `[source].keys`에 넣으세요.
- held layer에서 특정 키를 기본 레이어로 넘기고 싶으면 `transparent`를 사용하세요.
- 키 입력을 막고 싶으면 `noop`를 사용하세요.
- 설정을 바꾼 뒤 트레이 메뉴의 `Reload Config`로 다시 읽어오세요. 실패하면 기존 설정이 유지되고 오류 메시지가 표시됩니다.

## 검증

개발 환경에서는 저장소 루트에서 다음 명령으로 전체 테스트를 실행할 수 있습니다.

```powershell
cargo test --workspace
```

더 빠른 컴파일 확인만 필요하면 다음 명령을 사용합니다.

```powershell
cargo check --workspace
```
