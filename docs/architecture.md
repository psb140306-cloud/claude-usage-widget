# 아키텍처 설계

> 작성일: 2026-07-27 (`/bootstrap`)
> 기반: [prd.md](prd.md) · [api-schema.md](api-schema.md) · [tasks.md](tasks.md)

## 1. 기술스택

| 레이어 | 선택 | 버전 | 이유 |
|---|---|---|---|
| 앱 프레임워크 | **Tauri 2** | 2.11 | 트레이·always-on-top·프레임리스·자동시작·토스트·NSIS를 공식 플러그인으로 제공. WebView2 재사용으로 상주 메모리 유리 |
| 코어 로직 | **Rust** | 1.95 (edition 2021) | 자격증명·HTTPS·폴링을 네이티브에서 처리 → 토큰이 웹뷰로 넘어가지 않음 |
| 위젯 UI | **Svelte 5 (runes)** | 5.56 | 런타임·VDOM diff가 없어 상시 갱신되는 위젯에 유리. 코드량 최소 |
| 번들러 | **Vite** | 8.1 | 멀티 엔트리(위젯/설정)로 창별 번들 분리 |
| 타입 | **TypeScript** | 7.0 | Rust 모델과 1:1 대응 타입 유지 |
| HTTP | **reqwest** (rustls) | 0.13 | OpenSSL 무의존. `rustls-native-certs` 로 OS 신뢰 저장소 사용 |
| 히스토리 | **SQLite** (rusqlite bundled) | 0.40 | 90일×60초 = 최대 13만 행. prune·집계·기간조회가 단일 쿼리 |
| 설정 | tauri-plugin-store | 2.4 | 로컬 JSON, 즉시 적용 |
| 알림 | tauri-plugin-notification | 2.3 | Windows 토스트 |
| 자동시작 | tauri-plugin-autostart | 2.5 | 등록/해제 |
| 차트 | uPlot | 1.6 | 40KB급. 시계열 전용으로 가볍고 빠름 |
| 패키징 | Tauri NSIS 번들러 | — | `currentUser` 설치 (관리자 권한 불필요) |

**채택하지 않은 것**
- SvelteKit — 창 2개짜리 앱에 라우터/프리렌더링은 과함. 순수 Vite 멀티 엔트리가 시작 시간·구조 모두 유리
- tauri-plugin-sql — JS 측 API 중심인데 폴링·저장이 Rust 쪽에서 일어남. rusqlite 직접 사용이 자연스러움
- Electron / WPF / egui — planning.md §3 비교 참조

## 2. 모듈 구조

PRD 5.3 의 모듈 경계를 파일 구조로 그대로 옮겼다.

```
src-tauri/src/
├─ main.rs           진입점 (릴리스에서 콘솔 창 억제)
├─ lib.rs            조립: 플러그인 등록, 커맨드, 트레이, 창 이벤트
├─ model.rs          공용 타입 + usage 응답 정규화 (테스트 있음)
├─ credentials.rs    FR-1 자격증명 로드 (어댑터 경계)
├─ usage_client.rs   FR-2 usage 조회 (비공식 API 격리 경계)
├─ poller.rs         FR-3 주기 폴링 / 수동 새로고침 / 브로드캐스트
├─ history.rs        FR-7 SQLite 저장소
├─ notifier.rs       FR-6 임계값 평가 + 중복 억제
└─ settings.rs       FR-8 설정 모델

src/
├─ main.ts            위젯 엔트리      → index.html
├─ settings-main.ts   설정 엔트리      → settings.html
├─ app.css            테마 토큰 (라이트/다크, Windows 11 스타일)
├─ lib/
│  ├─ types.ts        ⚠️ model.rs 와 1:1 대응 (camelCase)
│  ├─ ipc.ts          invoke/listen 래퍼 — 프론트의 유일한 백엔드 접점
│  ├─ format.ts       % 표시·구간 색상·카운트다운·스테일 문구
│  └─ state.svelte.ts runes 전역 상태 + 1분 tick 시계
├─ widget/            Widget.svelte, Gauge.svelte
└─ settings/          Settings.svelte
```

### 데이터 흐름

```
.credentials.json ──(읽기전용)──▶ credentials
                                      │ AccessToken
                                      ▼
                                 usage_client ──HTTPS──▶ /api/oauth/usage
                                      │ UsageSnapshot | UsageError
                                      ▼
        ┌──────────────────────── poller ────────────────────────┐
        │                           │                            │
        ▼                           ▼                            ▼
     history                    notifier                   emit("usage://state")
   (SQLite append)          (토스트, 중복억제)                     │
                                                                  ▼
                                                    트레이 아이콘/툴팁 · 위젯 UI
```

**토큰은 Rust 경계를 넘지 않는다.** 프론트로 가는 것은 `AppState`(사용률·리셋시각)뿐이다.

## 3. 핵심 설계 결정

### 3.1 상태 머신 (크래시 대신 강등)

`AppState` 는 프론트가 그려야 할 4가지 상태 + 로딩을 그대로 표현한다. 파싱 실패·네트워크 오류·토큰 만료 어느 것도 패닉이 아니라 상태 전이로 처리한다 (PRD 6 안정성).

| 상태 | 트리거 | 위젯 표시 |
|---|---|---|
| `loading` | 최초 기동 | "불러오는 중…" |
| `ok` | 정상 응답 | 색상 게이지 |
| `stale` | 네트워크 실패 but 이전 값 보유 | 회색 게이지 + "N분 전 기준" |
| `needsReauth` | 토큰 만료 / HTTP 401 | "Claude Code를 한 번 실행해 주세요" |
| `unavailable` | 파일 없음 / 스키마 불일치 / 재시도 소진 | 회색 + 재시도 버튼 |

### 3.2 폴링 루프 (M2)

주기만큼 한 번에 자지 않고 **5초 단위로 깨어 벽시계를 비교**한다.

- `tokio::time::sleep` 은 단조 시계 기반이라 OS 절전 중 동작이 플랫폼마다 다르다. 벽시계(`Utc::now()`)를 기준으로 판단하면 절전 복귀 시의 점프가 그대로 보인다.
- 확인 간격(`gap`)이 15초 이상 벌어졌으면 그 사이 절전에 들어갔다고 보고, 남은 주기를 기다리지 않고 즉시 조회한다 (FR-3 "절전 복귀 시 즉시 1회 조회").
- 수동 새로고침은 `Notify` 로 즉시 깨운다. 5초 스로틀은 `Poller` 에 있다.
- 폴링 주기는 FR-8 범위(30초~10분)로 clamp 한다. 상한은 UX 뿐 아니라 안전장치이기도 하다 — 대기 루프가 주기를 `i64` 로 변환하므로 터무니없이 큰 값은 부호가 뒤집혀 5초마다 폴링하게 된다.

판정은 `should_poll(elapsed, gap, interval)` 순수 함수로 분리해 테스트한다. 깨어나 뺄셈만 하므로 유휴 CPU 는 측정 한계 이하(10초간 0ms)다.

**동기 I/O 격리**: `credentials::load()` 는 파일을 동기로 읽는다. 폴링 태스크에서 그대로 부르면 백신 검사·로밍 프로필로 지연될 때 런타임 워커를 막아 타이머와 커맨드까지 밀리므로 `spawn_blocking` 으로 넘긴다.

**상태코드는 본문보다 먼저 판정한다.** 본문을 먼저 읽으면 401 응답의 본문 수신이 끊겼을 때 `Auth` 가 아니라 재시도 대상인 `Network` 로 잘못 분류되어, 재인증 안내 대신 헛된 재시도를 한다.

**스키마 오류는 스테일로 덮지 않는다.** 네트워크 오류·429 는 마지막 값을 스테일로 계속 보여주지만(PRD 시나리오 9), 스키마 불일치는 API 가 바뀌었다는 뜻이라 값이 영영 갱신되지 않는다. 마지막 값을 계속 보여주면 고장을 숨기게 되므로 조회 불가로 간다 (FR-2).

### 3.3 계정·세션 정보 (usage API 밖의 데이터)

플랜·계정·현재 모델·effort·thinking 은 usage 엔드포인트에 없다. `environment.rs` 가 로컬 파일에서 읽는다.

| 항목 | 출처 |
|---|---|
| 이름·이메일·플랜 | `~/.claude.json` → `oauthAccount` |
| 모델 / effort / thinking | `~/.claude/projects/**/*.jsonl` 중 **가장 최근 수정된** 파일의 꼬리 256KB |

- Claude Code 세션이 여러 개일 수 있으므로 "가장 최근 활동한 세션"을 현재 세션으로 본다.
- thinking 은 최근 20개 응답 중 `type:"thinking"` 블록이 하나라도 있으면 활성으로 본다. 도구 호출만 있는 응답에는 thinking 블록이 없어 한 개만 보면 값이 흔들린다.
- **개인정보 원칙**: 트랜스크립트에는 대화 내용이 들어 있다. 이 모듈은 메타데이터 필드만 역직렬화하고 본문은 읽지도·보관하지도·내보내지도 않는다.
- 자격증명과 같은 이유로 어댑터로 격리한다. 어떤 항목이든 못 읽으면 `None` 이고, 그 때문에 사용량 표시가 막히지 않는다.

### 3.4 설정 저장 (FR-8)

`tauri-plugin-store` 를 쓰지 않고 `settings.rs` 가 직접 처리한다 — 폴링·검증이 전부 Rust 쪽이라 굳이 JS 경유가 필요 없다.

- 저장 위치: `app_config_dir()/settings.json`
- **부분 갱신**: 프론트가 보낸 키만 재귀 병합한다. `{"colors":{"text":"#fff"}}` 를 보내도 나머지 색이 지워지지 않는다.
- **범위 강제**: 폴링 주기 30~600초, 투명도 0.3~1.0, 임계값 0~100. 설정 파일은 사람이 손댈 수 있으므로 앱 안쪽으로 이상값이 새지 않게 한 번 막는다.
- **원자적 쓰기**: 임시 파일에 쓰고 rename. 저장 도중 종료돼도 설정이 반쯤 쓰이지 않는다.
- 파일이 없거나 깨졌으면 기본값으로 시작한다 — 설정 때문에 앱이 못 뜨면 안 된다.

변경은 `settings://changed` 이벤트로 열려 있는 창들에 즉시 방송되고, 폴링 주기는 돌고 있는 루프에도 반영한다(저장만 해서는 안 바뀐다).

### 3.5 창 수명주기에서 실제로 밟은 지뢰 3개

M2 UI 확장 중 발견해 고친 것들이다. 전부 "조용히 안 되는" 부류라 기록해 둔다.

1. **설정 창 X 버튼 먹통** — `on_window_event` 에서 `CloseRequested` 를 모든 창에 대해 가로채 숨김 처리했다. FR-5 는 *위젯* 창에만 해당한다. 설정 창은 그대로 닫아야 WebView2 렌더러도 함께 반환된다.
2. **창 생성이 워커 스레드에서 실패** — Tauri 커맨드는 워커 스레드에서 실행될 수 있고, 그 상태로 `WebviewWindowBuilder::build()` 를 부르면 Windows 에서 창은 뜨지만 웹뷰가 백지가 된다. `app.run_on_main_thread()` 안에서 만들어야 한다.
3. **⚙ 버튼이 정상 상태에서만 렌더링** — 헤더를 상태 분기 안에 두어, 조회 실패 중에는 설정을 열 수 없었다. 정작 그때 폴링 주기를 바꾸고 싶을 수 있다. 헤더는 상태와 무관하게 항상 그린다.

2번은 실패가 로그에도 남지 않아 "버튼이 안 눌린다"로 보였다. 창 생성 실패는 반드시 로그로 드러낸다.

### 3.6 비공식 API 격리

`usage_client.rs` 하나만 엔드포인트를 안다. URL·베타 헤더·타임아웃·재시도 횟수가 전부 이 파일의 상수다. Anthropic이 스펙을 바꾸면 수정 범위가 이 파일 + `model.rs` 의 `Raw*` 로 한정된다.

`model.rs` 의 `Raw*` 타입은 모든 필드가 `Option` 이고 미지의 필드는 serde 가 무시한다. 서버가 실험 버킷(`tangelo`, `nimbus_quill` …)을 늘려도 파싱이 깨지지 않는다.

### 3.7 토큰 노출 방지

`AccessToken` 은 뉴타입이며 `Debug` 구현이 값을 `<redacted>` 로 가린다. 값을 꺼내려면 `expose()` 를 명시적으로 호출해야 하므로, 구조체를 통째로 로깅하다 토큰이 새는 사고를 막는다. 테스트로 고정해 두었다 (`credentials::tests::token_debug_is_redacted`).

### 3.8 설정 창 지연 생성

설정 창을 시작 시 미리 만들면(숨김 상태여도) WebView2 렌더러가 하나 더 붙는다. 스캐폴딩 스모크 테스트 실측(debug, 프로세스 트리 WorkingSet 합): **404.5MB → 351.7MB**. 따라서 `tauri.conf.json` 에는 위젯 창만 선언하고, 설정 창은 `open_settings_window` 커맨드에서 `WebviewWindowBuilder` 로 만든다.

> ⚠️ WorkingSet 은 WebView2 프로세스 간 공유 페이지를 중복 계산한다. PRD 의 150MB 목표 검증은 M6 에서 release 빌드 + private bytes 기준으로 별도 수행한다.

### 3.9 위젯 기본 배치

좌표를 주지 않으면 Windows 기본 배치(좌상단 104,104)로 떨어진다. 3440×1440 울트라와이드에서 220×90 프레임리스 창은 사실상 눈에 띄지 않아서, 기본 위치를 **작업 영역 우측 하단 여백 16px** 로 잡았다.

`monitor.work_area()` 를 쓰면 작업 표시줄을 제외한 영역이 나오므로 위젯이 표시줄에 가리지 않는다. 여백은 `scale_factor` 를 곱해 고DPI에서도 시각적으로 동일하게 유지한다.

창은 `tauri.conf.json` 에서 `visible: false` 로 만들고 배치한 뒤 `show()` 한다. 그러지 않으면 좌상단에 떴다가 우측 하단으로 튀는 게 보인다.

실측 (3440×1440, DPI 100%, 작업 영역 3440×1392): `outer=220x90 margin=16 → (3204, 1286)`, 우측·하단 여백 각 16px. standalone·`tauri dev` 양쪽에서 동일 확인.

> M3 3.1 에서 저장된 위치가 있으면 이 기본값을 덮어쓴다.

### 3.10 갱신 주기 분리

- **폴링**(네트워크): 기본 60초 / 하한 30초
- **카운트다운 렌더**(로컬): 1분 tick

카운트다운을 초 단위로 돌리면 상주 CPU ~0% 목표에 불리하므로 분 해상도로 고정했다. 스모크 테스트에서 유휴 CPU 증가량은 5초간 0ms 였다.

### 3.11 창 닫기 ≠ 종료

`on_window_event` 에서 `CloseRequested` 를 가로채 숨김 처리한다. 완전 종료는 트레이 메뉴의 "종료"뿐이다 (FR-5).

## 4. 프론트↔백엔드 계약

| 커맨드 | 시그니처 | 구현 시점 |
|---|---|---|
| `get_state` | `() -> AppState` | ✅ 스캐폴딩 |
| `open_settings_window` | `() -> Result<()>` | ✅ 스캐폴딩 |
| `refresh_now` | `() -> Result<()>` | M2 |
| `get_settings` / `update_settings` | `() -> Settings` / `(patch) -> Settings` | M4 |
| `set_widget_mode` | `(mode) -> Result<()>` | M3 |
| `query_history` | `(from, to) -> Vec<HistoryEntry>` | M5 |

이벤트: `usage://state` (Rust → 프론트, `AppState` 페이로드). 이름 상수는 양쪽에 각각 `poller::EVENT_STATE` / `EVENT.state` 로 둔다.

미구현 커맨드는 `Err("M_에서 구현 예정")` 을 돌려준다. 조용히 성공한 척하지 않으므로 조기 연결 시 즉시 드러난다.

## 5. 주요 명령어

```bash
npm install              # 최초 1회
npm run tauri:dev        # 개발 (Vite HMR + Rust 자동 재빌드)
npm run build            # 프론트만 빌드 → dist/
npm run check            # svelte-check 타입 검사
npm run tauri:build      # 릴리스 + NSIS 인스톨러

cd src-tauri
cargo test --lib         # Rust 단위 테스트
cargo check --all-targets
```

## 6. 검증 상태 (2026-07-27, M2 2.2 완료 시점)

| 항목 | 결과 |
|---|---|
| `vite build` | ✅ 위젯 5.08kB + 공유 33.78kB (gzip 13.28kB) |
| `cargo check --all-targets` | ✅ 에러 0 |
| `cargo test --lib` | ✅ **20/20 통과** |
| 앱 실행 | ✅ 프레임리스 위젯 창, 우측 하단 배치 |
| **실데이터 표시** | ✅ 세션 5% / 주간 23% / "1:13 후" — PoC 스크립트 값과 완전 일치 |
| **스테일 강등** | ✅ 실제 429 발생 시 게이지 회색 + 마지막 값 유지 + "1분 전 기준" 뱃지 |
| 유휴 CPU | ✅ 10초간 0ms |
| 메모리 | ⚠️ debug 기준 351.7MB — release 기준 검증은 M6 |

### Codex CLI 교차 검증 (2026-07-27)

1차 리뷰에서 4건의 결함이 지적되어 모두 수정했고, 2차 리뷰에서 전부 CONFIRMED-FIXED + 신규 결함 없음으로 확인됐다.

| # | 지적 | 조치 |
|---|---|---|
| 1 | 401 본문 수신 실패 시 `Network` 로 오분류 → 헛된 재시도 | 상태코드를 본문보다 먼저 판정 |
| 2 | 스키마 오류인데 이전 스냅샷이 있으면 `Stale` → 고장을 숨김 | `allow_stale=false` 로 `Unavailable` 강제 |
| 3 | 동기 파일 I/O 가 tokio 워커를 블로킹 | `spawn_blocking` 으로 격리 |
| 4 | 절전 복귀 시 남은 주기를 기다림 (FR-3 위반) | 벽시계 gap ≥ 15초면 즉시 조회 |
| — | (방어) 폴링 주기 상한 없어 `i64` 변환 시 부호 뒤집힘 가능 | FR-8 범위(30~600초)로 clamp |

Codex 가 지적한 "히스토리 미저장"은 결함이 아니라 M5 5.1 로 계획된 항목이다.
