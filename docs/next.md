# 다음 세션 가이드

> 작성: 2026-07-27 (M1 PoC + /bootstrap 완료)

## 즉시 검증 (다음 세션 첫 1분)
```powershell
powershell -ExecutionPolicy Bypass -File scripts\m1-poc-usage.ps1   # 데이터 레이어 살아있는지
cd src-tauri; cargo test --lib                                       # 4/4 통과해야 함
npm run tauri:dev                                                    # 위젯 창 + 트레이 아이콘 확인
```
PoC 가 401 이면 Claude Code 를 한 번 실행해 토큰을 갱신할 것.

## 이번 세션 완료 (2026-07-27)

### M1 데이터 레이어 PoC — ✅ Go
| 작업 | 산출물 |
|------|--------|
| usage 엔드포인트 역추적 | claude.exe v2.1.210 번들에서 `fetchUtilization` 추출 |
| 실호출 검증 | `GET /api/oauth/usage` → **HTTP 200** |
| PoC 스크립트 | scripts/m1-poc-usage.ps1 |
| 응답 스키마 문서화 | docs/api-schema.md |

### M2 2.1 스캐폴딩 — ✅ 완료 (`/bootstrap`)
| 항목 | 결과 |
|------|------|
| 기술스택 확정 | Tauri 2.11 + Svelte 5(runes) + TS 7 + Vite 8 + SQLite + uPlot |
| 모듈 구조 | PRD 5.3 경계를 파일로 (credentials → usage_client → poller → history/notifier/UI) |
| 설계 문서 | docs/architecture.md |
| `vite build` | ✅ 246ms |
| `cargo check --all-targets` | ✅ 에러 0 |
| `cargo test --lib` | ✅ 4/4 (응답 파싱 · 토큰 마스킹 · ms 처리 고정) |
| 앱 실행 | ✅ 프레임리스 위젯 창 + 유휴 CPU 0ms/5s |

## 다음 작업 우선순위 — M2 2.2 코어 모듈 (Rust)

구현 스펙은 전부 [api-schema.md](api-schema.md) 에 있다. 추측하지 말고 그대로 옮길 것.

1. **`usage_client::fetch_usage`** — 현재 `todo!()`
   - 상수는 이미 있음: `USAGE_URL` / `OAUTH_BETA` / `TIMEOUT_SECS`
   - 상태코드 → `UsageError` 매핑은 api-schema.md §5 표 그대로
   - 200 이지만 `RawUsage::normalize()` 가 `None` → `UsageError::Schema`
2. **`poller`** — tokio interval 루프, 지수 백오프 3회, 수동 새로고침 5초 스로틀, 절전 복귀 시 즉시 1회
   - 상태 전이 후 `emit(EVENT_STATE, AppState)` → 프론트가 이미 구독 중
3. **`credentials`** 는 구현 완료 — 만료 시 `CredentialsError::Expired` → `AppState::NeedsReauth` 로 연결만 하면 됨
4. 이후 M3(위젯/트레이 UI) → M4(알림/설정) → M5(히스토리/차트) → M6(패키징)

## 사용자 행동 필요
- **`/usage` 대조 (M1 잔여 1건)**: Claude Code 대화형 세션에서 `/usage` 실행 → PoC 출력과 같은 시각 기준으로 비교. CLI 서브커맨드가 없어 자동화 불가.

## 알려진 이슈 / 모니터 대상
- **비공식 엔드포인트** — 변경 시 수정 범위는 `usage_client.rs` + `model.rs` 의 `Raw*` 로 한정되도록 격리해 둠
- **베타 헤더** `oauth-2025-04-20` 만료 가능성 → `usage_client.rs` 상수 1곳
- **실험 버킷 증식** (`tangelo`, `nimbus_quill` …) → 명명 필드 대신 `limits[]` 우선 사용
- **토큰 만료 동작 미검증** (tasks.md 1.1 잔여) — 실제 만료 시점에 M2 에서 확인
- **메모리 목표** — debug 기준 351.7MB. WorkingSet 은 WebView2 공유 페이지를 중복 계산하므로 PRD 150MB 검증은 M6 에서 release + private bytes 로 별도 수행
- `AppState` variants dead-code 경고 1건 — M2 에서 실제 전이가 생기면 사라짐
- PowerShell 스크립트는 **UTF-8 BOM 필수**
