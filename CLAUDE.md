# Claude Usage Widget

Claude Code의 OAuth 인증을 재사용하여 구독 사용량(5시간 세션/주간 한도 %)을 상시 표시하는 Windows 11 트레이 + 플로팅 위젯 앱.

## 프로젝트 상태

- 현재 단계: **M1~M6 완료 + v0.2.0 (2026-07-30)** — 보안 수정(email 미전달·HTTP 응답 상한) + 기간 리포트 추가. 상세는 [docs/next.md](docs/next.md)
- 위젯 표시: 계정·플랜 배지 / 세션·주간·모델별 게이지(각각 리셋 안내) / 24시간 추이 차트 / 모델·effort·thinking
- 설정 창: 사용량 리포트(오늘/7일/30일/1년, `daily_stats` 일별 롤업 — 원본 90일과 달리 무기한 보존) + 요일 패턴
- ⚠️ **메모리 KPI 미달성** — release private bytes 177.3MB (목표 150MB). 앱 자체는 7.6MB, 나머지는 WebView2. 사용자 판단 대기 (tasks.md M6)

## 핵심 문서

- [docs/planning.md](docs/planning.md) — 기획서 (배경, 기능, 기술 방향)
- [docs/prd.md](docs/prd.md) — PRD (요구사항 상세, 데이터 모델, KPI)
- [docs/tasks.md](docs/tasks.md) — 작업계획서 (마일스톤 M1~M6)
- [docs/api-schema.md](docs/api-schema.md) — **usage API 계약** (엔드포인트·헤더·응답 스키마·오류 처리) — M2 구현 스펙
- [docs/next.md](docs/next.md) — 다음 세션 가이드

## 핵심 결정 사항

- 데이터 소스: `%USERPROFILE%\.claude\.credentials.json`의 `claudeAiOauth.accessToken` 재사용 (읽기 전용, 별도 로그인 없음). `expiresAt`은 epoch **밀리초**
- 사용량 조회: `GET https://api.anthropic.com/api/oauth/usage` + `anthropic-beta: oauth-2025-04-20` (비공식 — M1 검증 완료). `utilization`은 **0~100 스케일**
- 위젯 형태: 트레이 상주 + always-on-top 플로팅 창 (Windows 위젯 보드 통합은 Non-Goal)
- 기술 방향(잠정): Tauri 2 + 경량 웹 프론트 — /bootstrap에서 확정
- 규모: 풀 버전 (알림, 히스토리 차트, 설정, 인스톨러 포함)

## 보안 원칙

- 자격증명은 읽기 전용 접근만. 토큰을 로그·히스토리·설정·문서 어디에도 기록하지 않는다
- 사용량 데이터의 외부 전송 없음 (완전 로컬 앱)

## 기술 스택

확정 (2026-07-27 `/bootstrap`). 근거·대안 비교는 [docs/architecture.md](docs/architecture.md).

- **앱**: Tauri 2.11 (Rust 1.95 / edition 2021)
- **UI**: Svelte 5.56 (runes) + TypeScript 5.9 + Vite 8 — SvelteKit 미사용, 멀티 엔트리(위젯/설정)
  - ⚠️ TS 는 7 이 아니라 **5.9 에 고정**. svelte-check 4.7 이 TS 7 네이티브 컴파일러에서 `typescript.sys` 를 못 찾고 죽는다 (빌드는 esbuild 라 무관하지만 타입 검사가 막힌다)
- **HTTP**: reqwest 0.13 (rustls + rustls-native-certs)
- **저장**: 히스토리 SQLite(rusqlite bundled) / 설정 tauri-plugin-store
- **차트**: uPlot 1.6
- **패키징**: Tauri NSIS 번들러 (currentUser 설치)

## 프로젝트 구조

```
src/           위젯 프론트 (Svelte). lib/types.ts 는 src-tauri/src/model.rs 와 1:1
src-tauri/src/ Rust 코어. PRD 5.3 모듈 경계 = 파일 구조
               credentials → usage_client → poller → (history / notifier / UI)
scripts/       M1 PoC 스크립트
docs/          기획·PRD·작업계획·API 스키마·아키텍처
```

## 주요 명령어

- 개발: `npm run tauri:dev`
- 프론트 빌드: `npm run build` / 타입 검사: `npm run check`
- Rust 테스트: `cd src-tauri; cargo test --lib`
- 릴리스+인스톨러: `npm run tauri:build`

## 개발 컨벤션

- **토큰은 Rust 경계를 넘지 않는다.** 프론트로 가는 건 `AppState`(사용률·리셋시각)뿐
- `AccessToken` 은 `Debug` 가 값을 가린다. 로깅 시 구조체째 찍어도 안전하도록 유지할 것
- 미구현 커맨드는 조용히 성공하지 말고 `Err("M_에서 구현 예정")` 을 반환
- 오류는 패닉이 아니라 `AppState` 강등으로 처리 (loading/ok/stale/needsReauth/unavailable)
- PowerShell 스크립트(.ps1)는 **UTF-8 BOM** 으로 저장 (없으면 한글 주석이 다음 줄을 삼킴)
- 커밋 메시지: `feat(m2): …` 처럼 마일스톤을 스코프로
