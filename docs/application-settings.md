# KeyZen 앱 설정

KeyZen tray 앱은 키 매핑 설정과 별개인 앱 설정 파일을 사용합니다.

```text
%APPDATA%\KeyZen\settings.yaml
```

## 스키마

```yaml
start_at_login: false
key_config_path: C:\Users\user\AppData\Roaming\KeyZen\keyzen.yaml
```

| 필드 | 설명 |
| --- | --- |
| `start_at_login` | Windows 로그인 시 KeyZen tray를 실행할지 지정합니다. |
| `key_config_path` | tray 런타임이 사용할 키 매핑 YAML 파일 경로입니다. 상대 경로는 `settings.yaml`이 있는 폴더를 기준으로 해석합니다. |

앱 설정은 임시 파일을 먼저 작성한 뒤 교체하는 방식으로 저장됩니다. Pause 상태는 현재 실행 세션에서만 유지되며 앱 설정에 저장되지 않습니다.

## 최초 실행

`keyzen` 또는 `keyzen tray`를 처음 실행하면 다음 파일을 자동 생성합니다.

```text
%APPDATA%\KeyZen\settings.yaml
%APPDATA%\KeyZen\keyzen.yaml
```

기본 `keyzen.yaml`은 빈 `base` 레이어를 포함합니다.

```yaml
layers:
  base: {}
```

저장된 키 설정 파일이 없거나 유효하지 않으면 tray는 paused 상태로 실행됩니다. 이 상태에서도 `Choose key config...`로 유효한 설정을 선택해 복구할 수 있습니다.

## Tray 메뉴

메뉴 라벨은 영어로 표시되며 순서는 다음과 같습니다.

```text
Pause
---
Start at login
Choose key config...
---
Quit
```

- `Pause`: 체크된 동안 remap을 중단하고 키를 그대로 통과시킵니다. 전환 시 눌린 합성 키, 레이어, tap-hold, tap dance, one-shot 상태를 초기화합니다.
- `Start at login`: 현재 사용자의 Windows Task Scheduler `KeyZen` 로그인 작업을 등록하거나 삭제합니다. 작업 변경에 성공한 경우에만 앱 설정을 저장합니다.
- `Choose key config...`: `.yaml` 또는 `.yml` 파일을 선택하고 즉시 검증합니다. 유효한 경우 바로 적용하며 현재 Pause 상태는 유지합니다.
- `Quit`: 눌린 합성 키와 엔진 상태를 초기화한 뒤 종료합니다.

tray tooltip은 실행 중 `KeyZen`, 일시정지 중 `KeyZen (Paused)`로 표시됩니다.

## 실행 파일

```powershell
keyzen
keyzen tray
```

두 명령은 같은 폴더의 콘솔 없는 `keyzen-tray.exe`를 background로 실행하고 즉시 종료합니다.

```powershell
keyzen --config path\to\keyzen.yaml
```

`--config`를 직접 지정하면 기존 foreground 콘솔 모드로 실행합니다. `Local\KeyZen.Runtime` named mutex로 tray와 foreground remapper 중 하나만 실행할 수 있습니다.
