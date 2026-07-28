# Claude Usage Widget - 작업계획서

> 작성일: 2026-07-20
> 기반 문서: prd.md v1.0

## 마일스톤 개요

| # | 마일스톤 | 예상 기간 | 핵심 산출물 |
|---|---------|---------|-----------|
| M1 | 데이터 레이어 PoC (Go/No-Go 게이트) | 1~2일 | usage API 검증 스크립트, 스키마 문서 |
| M2 | 앱 골격 + 코어 서비스 | 2~3일 | Tauri 앱, credentials/usage-client/poller 모듈 |
| M3 | 위젯 & 트레이 UI | 3~4일 | 컴팩트/확장 위젯, 트레이 아이콘·메뉴 |
| M4 | 알림 & 설정 | 2~3일 | 토스트 알림, 설정 UI, 자동 시작 |
| M5 | 히스토리 & 차트 | 2~3일 | 스냅샷 저장소, 24시간/일별 차트 |
| M6 | 패키징 & 안정화 | 2~3일 | 인스톨러, 장시간 테스트, README |

## 상세 태스크

### M1: 데이터 레이어 PoC ⚠️ Go/No-Go 게이트 — **✅ Go (2026-07-27)**

> 비공식 usage 엔드포인트가 실제로 동작하는지 본 개발 전에 검증한다.
> 실패 시: 로컬 트랜스크립트 파싱 방식으로 재기획 (M2 이후 진행 중단).
> 결과: `GET https://api.anthropic.com/api/oauth/usage` → HTTP 200. 상세는 [api-schema.md](api-schema.md).

#### 1.1 자격증명 확인
- [x] `%USERPROFILE%\.claude\.credentials.json` 존재·구조 확인 (accessToken, expiresAt, subscriptionType 필드)
  - 결과: `claudeAiOauth.{accessToken, refreshToken, expiresAt(ms), scopes, subscriptionType, rateLimitTier}` + `organizationUuid`
  - ⚠️ `expiresAt`은 epoch **밀리초**
- [ ] 토큰 만료 시 Claude Code 실행으로 갱신되는지 동작 확인 (만료 시점 도래 후 확인 — M2로 이월)

#### 1.2 usage 엔드포인트 검증
- [x] Claude Code의 usage 조회 엔드포인트 URL·요청 헤더 파악
  - 방법: 설치된 `claude.exe`(v2.1.210)에 포함된 JS 번들에서 `fetchUtilization` 호출부 직접 추출
  - 결과: `GET /api/oauth/usage` + `Authorization: Bearer` + `anthropic-beta: oauth-2025-04-20`
- [x] PoC 스크립트 작성: 토큰 로드 → 엔드포인트 호출 → 응답 JSON 출력 → [scripts/m1-poc-usage.ps1](../scripts/m1-poc-usage.ps1)
- [x] 응답 스키마 문서화: 세션/주간/Opus 사용률, 리셋 시각 필드 매핑 → [docs/api-schema.md](api-schema.md)
- [ ] Claude Code `/usage` 표시값과 PoC 결과 대조 (정확도 확인) — **사용자 확인 대기**
- [x] **Go/No-Go 판정**: 엔드포인트 동작 확인 → **Go**, M2 진행

### M2: 앱 골격 + 코어 서비스

#### 2.1 프로젝트 스캐폴딩 — ✅ 완료 (2026-07-27 `/bootstrap`)
- [x] Tauri 2 프로젝트 초기화 — Tauri 2.11 + Svelte 5 + Vite 8 (SvelteKit 미사용, 멀티 엔트리)
  - 검증: `vite build` ✅ / `cargo check --all-targets` ✅ / `cargo test --lib` 4/4 ✅ / 앱 실행 ✅
- [x] 폴더 구조·타입 설정, .gitignore, git — 모듈 구조는 [architecture.md](architecture.md) §2
- [x] 트레이 + 프레임리스 always-on-top 창 + 창닫기≠종료 골격 (M3 에서 UI 살 붙임)

#### 2.2 코어 모듈 (Rust 사이드) — ✅ 완료 (2026-07-27)
- [x] credentials 모듈: load() → Token/NotFound/Expired/ParseError, 어댑터 구조로 분리
  - `AccessToken` 뉴타입 + `Debug` 마스킹. 폴링 시 `spawn_blocking` 으로 동기 I/O 격리
- [x] usage-client 모듈: fetchUsage(token), 방어적 파싱, 401/5xx/타임아웃 오류 분류, 지수 백오프 3회
  - 상태코드를 **본문보다 먼저** 판정 (401 본문 수신 실패 시 재시도 대상으로 오분류되는 문제)
  - 백오프 500ms → 1s → 2s, 총 3회 시도. 429 는 재시도 대상에서 제외
- [x] poller 모듈: 주기 폴링(기본 60초), 수동 새로고침(5초 스로틀), 절전 복귀 감지 시 즉시 조회
  - 5초 단위로 깨어 **벽시계**를 비교 → 확인 간격이 15초 이상 벌어지면 절전 복귀로 보고 즉시 조회
  - 주기는 FR-8 범위(30초~10분)로 clamp
- [x] 프론트로 이벤트 발행 (`usage://state`) 및 단위 테스트 (20개)
- [ ] 트레이 툴팁·아이콘에 반영 → M3 3.2
- [ ] 히스토리 저장소로 브로드캐스트 → M5 5.1

### M3: 위젯 & 트레이 UI

#### 3.1 플로팅 위젯 창
- [x] 프레임리스 + always-on-top + 스킵 태스크바 창 설정
- [x] 게이지 3개(세션 5시간 / 주간 7일 / 모델별 주간) + **게이지마다 리셋 안내**, 구간별 색상(<60/60~85/>85)
- [x] 헤더: 계정 이름 + 플랜 배지(Max 20x) + 설정 버튼 — 상태와 무관하게 항상 표시
- [x] 푸터: 현재 세션 모델 · effort · thinking
- [ ] 확장 모드 UI: 수치 카드 + 히스토리 차트, 컴팩트↔확장 전환 (기본 크기 240×215로 상시 표시하는 방식 채택)
- [ ] 드래그 이동 + 위치·모드 저장·복원
  - 기본 위치(저장값 없을 때)는 스캐폴딩에서 구현됨: 작업 영역 우측 하단 여백 16px (`place_bottom_right`)
- [ ] 상태 UI: 스테일("N분 전 기준"), 재인증 필요 안내, 조회 불가 + 재시도 버튼

#### 3.2 트레이
- [ ] 트레이 아이콘 + 사용률 구간별 아이콘 색상 변화
  - 의존성: 2.2 poller
- [ ] 툴팁 요약 (세션 % · 주간 % · 리셋 시각)
- [ ] 우클릭 메뉴: 위젯 표시/숨김, 지금 새로고침, 설정, 종료 (창 닫기 ≠ 앱 종료)

### M4: 알림 & 설정

#### 4.1 임계값 알림
- [ ] notifier 모듈: 임계값(80/95%) 평가 + Windows 토스트 발송
  - 의존성: M2 poller
- [ ] 중복 억제: 동일 임계값·동일 리셋 주기 내 1회만
- [ ] 리셋 완료 알림 (기본 off)

#### 4.2 설정
- [x] Settings 저장소 (로컬 JSON, 원자적 쓰기, 부분 갱신, 범위 검증, 즉시 적용)
- [x] 설정 UI(1차): **글자·프로그레스바 색상 7종**, 배경 불투명도, 폴링 주기
- [ ] 설정 UI(2차): 알림 임계값, 알림 on/off, 테마, 자동 시작
- [ ] Windows 시작 시 자동 실행 등록/해제 (tauri autostart 플러그인)
- [ ] 다크/라이트/시스템 테마 적용

### M5: 히스토리 & 차트

#### 5.1 저장소
- [ ] history 모듈: 스냅샷 append, 기간 query, 90일 초과 prune
  - 의존성: M2 poller
- [x] 저장 포맷 결정 → **SQLite** (rusqlite bundled). 근거: architecture.md §1. 구현은 M5

#### 5.2 시각화
- [ ] 확장 모드에 최근 24시간 세션 사용률 추이 차트
  - 의존성: 5.1, 3.1 확장 모드
- [ ] 일별/주별 집계 뷰 (요일 패턴)

### M6: 패키징 & 안정화

- [ ] 앱 아이콘·이름·버전 정리
- [ ] Tauri 번들러로 NSIS 인스톨러 생성, 설치/제거 테스트
  - 의존성: M3~M5 완료
- [ ] 24시간 상주 테스트: 크래시·메모리(≤150MB)·CPU 확인
- [ ] KPI 검증: `/usage` 대조 10회, 알림 적시성, 재부팅 자동 시작
- [ ] README 작성 (설치법, 비공식 API 주의사항)
- [ ] (선택) GitHub Releases 자동 업데이트 연동

## 의존관계 다이어그램

```
M1 (PoC: credentials → endpoint 검증) ⚠️ Go/No-Go
 └→ M2 (스캐폴딩 → credentials/usage-client/poller)
     ├→ M3 (위젯 UI, 트레이)  ─┐
     ├→ M4 (알림, 설정)        ├→ M6 (패키징·안정화)
     └→ M5 (히스토리, 차트)   ─┘
M3~M5는 M2 완료 후 병렬 진행 가능 (단, 5.2 차트는 3.1 확장 모드 필요)
```

## Go/No-Go 체크리스트

- [x] 아이디어가 충분히 구체화되었는가? → planning.md, prd.md 완료
- [x] 기술적으로 실현 가능한가? → **M1 PoC 통과 (2026-07-27)** — HTTP 200, 세션/주간/모델별 사용률·리셋 시각 모두 확보
- [x] 투입 시간 대비 가치가 있는가? → 매일 쓰는 도구, 총 2~3주 규모
- [x] 필요한 리소스가 확보 가능한가? → Claude Code 로그인 상태의 PC (확보됨), 엔드포인트 접근 확인됨
