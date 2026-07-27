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

### 3.2 비공식 API 격리

`usage_client.rs` 하나만 엔드포인트를 안다. URL·베타 헤더·타임아웃·재시도 횟수가 전부 이 파일의 상수다. Anthropic이 스펙을 바꾸면 수정 범위가 이 파일 + `model.rs` 의 `Raw*` 로 한정된다.

`model.rs` 의 `Raw*` 타입은 모든 필드가 `Option` 이고 미지의 필드는 serde 가 무시한다. 서버가 실험 버킷(`tangelo`, `nimbus_quill` …)을 늘려도 파싱이 깨지지 않는다.

### 3.3 토큰 노출 방지

`AccessToken` 은 뉴타입이며 `Debug` 구현이 값을 `<redacted>` 로 가린다. 값을 꺼내려면 `expose()` 를 명시적으로 호출해야 하므로, 구조체를 통째로 로깅하다 토큰이 새는 사고를 막는다. 테스트로 고정해 두었다 (`credentials::tests::token_debug_is_redacted`).

### 3.4 설정 창 지연 생성

설정 창을 시작 시 미리 만들면(숨김 상태여도) WebView2 렌더러가 하나 더 붙는다. 스캐폴딩 스모크 테스트 실측(debug, 프로세스 트리 WorkingSet 합): **404.5MB → 351.7MB**. 따라서 `tauri.conf.json` 에는 위젯 창만 선언하고, 설정 창은 `open_settings_window` 커맨드에서 `WebviewWindowBuilder` 로 만든다.

> ⚠️ WorkingSet 은 WebView2 프로세스 간 공유 페이지를 중복 계산한다. PRD 의 150MB 목표 검증은 M6 에서 release 빌드 + private bytes 기준으로 별도 수행한다.

### 3.5 갱신 주기 분리

- **폴링**(네트워크): 기본 60초 / 하한 30초
- **카운트다운 렌더**(로컬): 1분 tick

카운트다운을 초 단위로 돌리면 상주 CPU ~0% 목표에 불리하므로 분 해상도로 고정했다. 스모크 테스트에서 유휴 CPU 증가량은 5초간 0ms 였다.

### 3.6 창 닫기 ≠ 종료

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

## 6. 검증 상태 (2026-07-27 스캐폴딩 시점)

| 항목 | 결과 |
|---|---|
| `vite build` | ✅ 246ms, 위젯 5.08kB + 공유 33.78kB (gzip 13.28kB) |
| `cargo check --all-targets` | ✅ 에러 0 |
| `cargo test --lib` | ✅ 4/4 통과 |
| 앱 실행 | ✅ 프레임리스 위젯 창 생성 확인 (`MainWindowTitle: Claude Usage`) |
| 유휴 CPU | ✅ 5초간 0ms |
| 메모리 | ⚠️ debug 기준 351.7MB — release 기준 검증은 M6 |
