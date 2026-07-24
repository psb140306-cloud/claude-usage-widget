# Claude Usage Widget

Claude Code의 OAuth 인증을 재사용하여 구독 사용량(5시간 세션/주간 한도 %)을 상시 표시하는 Windows 11 트레이 + 플로팅 위젯 앱.

## 프로젝트 상태

- 현재 단계: 기획 완료 (2026-07-20) → 다음: `/bootstrap` (기술스택 확정·스캐폴딩)
- 첫 작업: M1 PoC — 비공식 usage 엔드포인트 검증 (Go/No-Go 게이트, docs/tasks.md 참조)

## 핵심 문서

- [docs/planning.md](docs/planning.md) — 기획서 (배경, 기능, 기술 방향)
- [docs/prd.md](docs/prd.md) — PRD (요구사항 상세, 데이터 모델, KPI)
- [docs/tasks.md](docs/tasks.md) — 작업계획서 (마일스톤 M1~M6)

## 핵심 결정 사항

- 데이터 소스: `%USERPROFILE%\.claude\.credentials.json`의 OAuth 토큰 재사용 (읽기 전용, 별도 로그인 없음)
- 사용량 조회: Claude Code 내부 OAuth usage 엔드포인트 (비공식 — M1에서 검증)
- 위젯 형태: 트레이 상주 + always-on-top 플로팅 창 (Windows 위젯 보드 통합은 Non-Goal)
- 기술 방향(잠정): Tauri 2 + 경량 웹 프론트 — /bootstrap에서 확정
- 규모: 풀 버전 (알림, 히스토리 차트, 설정, 인스톨러 포함)

## 보안 원칙

- 자격증명은 읽기 전용 접근만. 토큰을 로그·히스토리·설정·문서 어디에도 기록하지 않는다
- 사용량 데이터의 외부 전송 없음 (완전 로컬 앱)

## 기술 스택

> /bootstrap 후 확정 내용으로 업데이트 예정
