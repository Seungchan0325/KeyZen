# KeyZen Keymap 설정 파일 작성 가이드

KeyZen의 keymap 파일은 TOML 형식입니다. 기본 위치는 Windows에서 보통 `%APPDATA%\KeyZen\keyzen.toml`이며, 실제로 사용할 keymap 경로는 `%APPDATA%\KeyZen\config.toml`의 `keymap_path` 값으로 정합니다.

KeyZen은 키보드 레이어 엔진입니다. keymap에는 키 치환, 단축키 조합, 레이어 전환, tap-hold, tap-dance, one-shot 규칙을 적습니다. 텍스트 매크로, 프로그램 실행, 지연 동작, 마우스 자동화는 MVP 범위가 아닙니다.

## 기본 구조

```toml
[settings]
startup_layer = "base"
tap_hold_timeout_ms = 200
tap_dance_timeout_ms = 200
one_shot_timeout_ms = 1000

[vars]
fast_timeout = 150
normal_timeout = 200

[source]
keys = ["CapsLock", "H", "J", "K", "L", "Space"]

[aliases]
nav_left = "Left"
nav_down = "Down"
nav_up = "Up"
nav_right = "Right"

[layers.base]
CapsLock = { layer_while_held = "nav" }
H = "H"
J = "J"
K = "K"
L = "L"
Space = "Space"

[layers.nav]
H = "nav_left"
J = "nav_down"
K = "nav_up"
L = "nav_right"
Space = "transparent"
```

## `[settings]`

`startup_layer`는 KeyZen이 시작할 때 사용할 기본 레이어 이름입니다. 생략하면 `"base"`가 기본값입니다. 지정한 레이어는 반드시 `[layers.<name>]`로 존재해야 합니다.

시간 기반 동작의 기본 timeout도 여기에서 정합니다. 생략하면 `tap_hold_timeout_ms = 200`, `tap_dance_timeout_ms = 200`, `one_shot_timeout_ms = 1000`이 사용됩니다. 값은 양의 정수 밀리초여야 합니다.

```toml
[settings]
tap_hold_timeout_ms = 200
tap_dance_timeout_ms = 200
one_shot_timeout_ms = 1000
```

각 action은 `timeout_ms`로 기본값을 덮어쓸 수 있습니다.

## `[vars]`

`[vars]`에는 timeout에 재사용할 정수 상수를 적습니다. 런타임에 변하는 값이 아니라 keymap을 읽을 때 한 번 치환되는 값입니다.

```toml
[vars]
fast_timeout = 150
normal_timeout = 200
one_shot_short = 700
```

`timeout_ms`에는 숫자를 직접 쓰거나 변수 이름을 문자열로 쓸 수 있습니다.

```toml
Space = { tap_hold = { tap = "Space", hold = { layer_while_held = "nav" }, timeout_ms = "fast_timeout" } }
LeftAlt = { one_shot_modifier = { modifier = "Alt", timeout_ms = 700 } }
```

`[vars]` 값은 양의 정수만 지원합니다. 정의되지 않은 변수 이름, `0`, 음수, 비정수 값은 설정 오류입니다.

## `[source]`

`keys`에는 KeyZen이 가로챌 입력 키를 적습니다.

```toml
[source]
keys = ["CapsLock", "H", "J", "K", "L"]
```

`[source].keys`에 포함된 키만 KeyZen 엔진으로 들어갑니다. source에 없는 키는 Windows hook에서 바로 통과하므로 레이어, tap-hold interrupt, tap-dance, one-shot의 대상이 되지 않습니다. 레이어 키, 이동 키, 특수 동작을 붙일 키를 모두 여기에 포함하세요.

source에 포함된 키가 활성 레이어와 base 레이어 어디에도 정의되어 있지 않으면 원래 키 입력으로 동작합니다. 키를 막고 싶으면 명시적으로 `noop`를 사용하세요.

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

Tap-hold:

```toml
CapsLock = { tap_hold = { tap = "Escape", hold = { layer_while_held = "nav" } } }
Space = { tap_hold = { tap = "Space", hold = { layer_while_held = "symbols" }, timeout_ms = "fast_timeout" } }
```

`tap_hold`는 timeout 전에 떼면 `tap`, timeout이 지나거나 다른 처리 대상 키를 누르면 `hold`로 동작합니다. `timeout_ms`를 생략하면 `[settings].tap_hold_timeout_ms`를 사용합니다.

Tap-dance:

```toml
Semicolon = { tap_dance = { single = "Semicolon", double = "Escape", triple = "Enter" } }
Quote = { tap_dance = { single = "Quote", double = "Enter", timeout_ms = "normal_timeout" } }
```

`tap_dance`는 single, double, triple까지 지원합니다. 마지막으로 뗀 뒤 timeout 안에 추가 tap이 없으면 해당 횟수의 action을 실행합니다. 설정된 최대 tap 수에 도달하면 즉시 실행합니다.

One-shot modifier:

```toml
LeftShift = { one_shot_modifier = "Shift" }
LeftAlt = { one_shot_modifier = { modifier = "Alt", timeout_ms = "one_shot_short" } }
```

`one_shot_modifier`는 다음 키 또는 chord 한 번에 수식어를 적용합니다. timeout이 지나면 대기 상태가 취소됩니다.

One-shot layer:

```toml
F = { one_shot_layer = "nav" }
D = { one_shot_layer = { layer = "symbols", timeout_ms = 800 } }
```

`one_shot_layer`는 다음 처리 대상 key down 하나를 지정 레이어에서 해석합니다. 해당 레이어의 action이 `transparent`이면 아래 레이어로 내려갑니다.

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

## `[aliases]`

`[aliases]`에는 자주 쓰는 동작에 이름을 붙일 수 있습니다. alias는 레이어의 값 위치에서 문자열로 참조합니다.

```toml
[aliases]
prev_word = "Ctrl+Left"
next_word = "Ctrl+Right"
nav_hold = { layer_while_held = "nav" }
disabled = "noop"

[layers.base]
CapsLock = "nav_hold"

[layers.nav]
B = "prev_word"
W = "next_word"
X = "disabled"
```

alias 값에는 일반 동작과 같은 형식을 사용할 수 있습니다. 단일 키, 수식어 조합, `transparent`, `noop`, `layer_while_held`, `layer_switch`, tap-hold, tap-dance, one-shot이 모두 가능합니다.

내장 키 이름과 특수 문자열이 먼저 해석됩니다. 예를 들어 `Left`, `Escape`, `transparent`, `noop` 같은 이름은 alias로 덮어쓸 수 없습니다. alias가 서로 순환 참조하면 설정 로드가 실패합니다.

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

## 예시: 풍부한 Vim 스타일 레이어

아래 예시는 `CapsLock`을 누르고 있는 동안 Vim 느낌의 이동 레이어를 활성화합니다. Vim의 모드 상태나 반복 명령을 흉내 내는 매크로는 아니고, KeyZen이 지원하는 단일 키 출력과 수식어 조합만으로 구성한 실용형 레이어입니다.

```toml
[settings]
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
    "R",
    "S",
    "V",
    "Y",
    "O",
    "N",
    "M",
    "X",
    "Z",
    "Q",
    "Escape",
    "Enter",
    "Space",
]

[layers.base]
CapsLock = { layer_while_held = "vim" } # CapsLock을 누르는 동안 Vim 레이어를 활성화합니다.

[aliases]
prev_word = "Ctrl+Left"              # 이전 단어로 이동합니다.
next_word = "Ctrl+Right"             # 다음 단어로 이동합니다.
doc_start = "Ctrl+Home"              # 문서의 처음으로 이동합니다.
doc_end = "Ctrl+End"                 # 문서의 끝으로 이동합니다.
select_right = "Shift+Right"         # 선택 영역을 오른쪽으로 한 글자 확장합니다.
select_line_start = "Shift+Home"     # 선택 영역을 줄 처음까지 확장합니다.
select_line_end = "Shift+End"        # 선택 영역을 줄 끝까지 확장합니다.
select_prev_word = "Ctrl+Shift+Left" # 선택 영역을 이전 단어까지 확장합니다.
select_next_word = "Ctrl+Shift+Right" # 선택 영역을 다음 단어까지 확장합니다.

[layers.vim]
H = "Left"             # 왼쪽으로 이동합니다.
J = "Down"             # 아래로 이동합니다.
K = "Up"               # 위로 이동합니다.
L = "Right"            # 오른쪽으로 이동합니다.
U = "PageUp"           # 한 페이지 위로 이동합니다.
D = "PageDown"         # 한 페이지 아래로 이동합니다.
B = "prev_word"        # 이전 단어로 이동합니다.
W = "next_word"        # 다음 단어로 이동합니다.
G = "Home"             # 현재 줄의 처음으로 이동합니다.
I = "Home"             # 현재 줄의 처음으로 이동합니다.
A = "End"              # 현재 줄의 끝으로 이동합니다.
R = "doc_start"         # 문서의 처음으로 이동합니다.
S = "doc_end"           # 문서의 끝으로 이동합니다.
V = "select_right"      # 선택 영역을 오른쪽으로 한 글자 확장합니다.
Y = "select_line_start" # 선택 영역을 줄 처음까지 확장합니다.
O = "select_line_end"   # 선택 영역을 줄 끝까지 확장합니다.
N = "select_prev_word"  # 선택 영역을 이전 단어까지 확장합니다.
M = "select_next_word"  # 선택 영역을 다음 단어까지 확장합니다.
X = "Delete"           # 다음 글자를 삭제합니다.
Z = "Backspace"        # 이전 글자를 삭제합니다.
Q = "Escape"           # Escape를 보냅니다.
Escape = "Escape"      # Escape를 보냅니다.
Enter = "Enter"        # Enter를 보냅니다.
Space = "transparent"  # 아래 레이어의 동작으로 넘깁니다.
```

이 예시의 키 배치는 다음처럼 읽으면 됩니다.

- `H/J/K/L`: 왼쪽, 아래, 위, 오른쪽 이동
- `B/W`: 이전/다음 단어로 이동
- `U/D`: 페이지 위/아래
- `G/I/A`: 줄 처음, 줄 처음, 줄 끝
- `R/S`: 문서 처음/끝
- `V/Y/O/N/M`: 한 글자, 줄 처음, 줄 끝, 이전 단어, 다음 단어 방향으로 선택 확장
- `X/Z`: Delete와 Backspace
- `Q` 또는 `Escape`: Escape
- `Space`: 아래 레이어로 넘기는 `transparent`

## 작성 팁

- KeyZen이 처리할 키를 `[source].keys`에 명시하세요. source 밖의 키는 엔진으로 들어가지 않습니다.
- `startup_layer`와 `layer_while_held`, `layer_switch`가 가리키는 레이어 이름이 실제로 존재하는지 확인하세요.
- 레이어 전환 키 자체도 `[source].keys`에 넣으세요.
- held layer에서 특정 키를 기본 레이어로 넘기고 싶으면 `transparent`를 사용하세요.
- 키 입력을 막고 싶으면 `noop`를 사용하세요.
- keymap 파일을 바꾼 뒤에는 트레이 메뉴의 `Reload Keymap`으로 다시 읽어오세요. `%APPDATA%\KeyZen\config.toml`의 앱 설정이나 keymap 경로를 바꾼 경우에는 `Reload Config`를 사용하세요. 실패하면 기존 설정이 유지되고 오류 메시지가 표시됩니다.

## 검증

개발 환경에서는 저장소 루트에서 다음 명령으로 전체 테스트를 실행할 수 있습니다.

```powershell
cargo test --workspace
```

더 빠른 컴파일 확인만 필요하면 다음 명령을 사용합니다.

```powershell
cargo check --workspace
```
