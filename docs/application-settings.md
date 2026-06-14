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
- `Start at login`: 현재 사용자의 Windows Task Scheduler 작업 `\KeyZen\Autorun for <username>`을 등록하거나 삭제합니다. 작업 변경에 성공한 경우에만 앱 설정을 저장합니다.
- `Choose key config...`: `.yaml` 또는 `.yml` 파일을 선택하고 즉시 검증합니다. 유효한 경우 바로 적용하며 현재 Pause 상태는 유지합니다.
- `Quit`: 눌린 합성 키와 엔진 상태를 초기화한 뒤 종료합니다.

등록된 작업은 현재 사용자 로그온 트리거와 3초 지연(`PT03S`)을 사용합니다. 실행 파일은 현재 `keyzen-tray.exe`의 절대 경로이며, 로그온 유형은 `TASK_LOGON_INTERACTIVE_TOKEN`, 실행 권한은 일반 사용자 권한(`TASK_RUNLEVEL_LUA`)입니다.

tray tooltip은 실행 중 `KeyZen`, 일시정지 중 `KeyZen (Paused)`로 표시됩니다. 로그인 직후 Explorer 또는 알림 영역이 아직 준비되지 않아 tray 아이콘 등록이 실패해도 앱은 종료하지 않고, `TaskbarCreated`와 hidden top-level window 메시지에서 아이콘 등록을 다시 시도합니다.

## 진단 로그

tray 앱은 다음 파일에 시작, 트레이 아이콘 등록, 런타임 상태, 종료 요청과 오류를 기록합니다.

```text
%APPDATA%\KeyZen\keyzen.log
```

로그가 시작 시점에 5MB 이상이면 기존 로그를 `keyzen.log.1`로 이동합니다. 실행 중에는
`tray.running` 마커가 존재하며 정상 종료 시 제거됩니다. 강제 종료나 프로세스 충돌처럼 마지막
원인을 기록할 수 없는 경우에는 다음 실행 로그에 이전 세션이 정상 종료되지 않았다고 기록됩니다.

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
