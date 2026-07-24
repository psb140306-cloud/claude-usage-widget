# 다음 세션 가이드

> 작성: 2026-07-20
> 마지막 커밋: (git 미사용 — /bootstrap 시 초기화 예정)

## 즉시 검증 (다음 세션 첫 1분)
- [docs/prd.md](prd.md), [docs/tasks.md](tasks.md) 존재 확인 — /bootstrap의 입력 문서
- `%USERPROFILE%\.claude\.credentials.json` 존재 확인 — M1 PoC의 전제 조건

## 이번 세션 완료 (2026-07-20)

| 작업 | 산출물 | 상태 |
|------|--------|------|
| 기획 방향 확정 | 데이터 소스=Claude Code OAuth 재사용, 형태=트레이+플로팅 위젯, 규모=풀 버전 | ✅ |
| 기획서 작성 | docs/planning.md | ✅ |
| PRD 작성 | docs/prd.md (FR-1~8, Non-Goals, KPI) | ✅ |
| 작업계획서 작성 | docs/tasks.md (M1~M6) | ✅ |
| 프로젝트 개요 | CLAUDE.md | ✅ |

## 다음 작업 우선순위

1. **사용자 Go/No-Go 판단** — 기획 문서 3종 검토 (사용자 결정 필요)
2. Go 시 → `/bootstrap` 실행: 기술스택 확정(Tauri 2 잠정안) + 스캐폴딩 + git 초기화
3. 이후 → `/dev`로 **M1 PoC** 시작: 비공식 usage 엔드포인트 검증 (⚠️ Go/No-Go 게이트 — 실패 시 로컬 트랜스크립트 파싱으로 방향 전환)

## 사용자 행동 필요
- 기획 문서 검토 후 Go / 수정 / 보류 결정

## 알려진 이슈 / 모니터 대상
- 사용량 조회가 **비공식 엔드포인트** 의존 — M1 PoC 전까지 실현 가능성 미확정
- Claude Code 자격증명 파일 경로·포맷이 버전업으로 바뀔 수 있음 → 어댑터 분리 설계 반영됨 (prd.md FR-1)
- 00.아이디어_워크플로우 폴더 부재 → 기획 문서는 이 프로젝트 docs/에 직접 저장하는 방식으로 확정
