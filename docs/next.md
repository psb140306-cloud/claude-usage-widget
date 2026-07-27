# 다음 세션 가이드

> 작성: 2026-07-27 (이전: 2026-07-20)
> 마지막 커밋: 63e01ce chore: 초기 커밋 - Git 안전망 도입 (DMS)

## 즉시 검증 (다음 세션 첫 1분)
- `powershell -ExecutionPolicy Bypass -File scripts\m1-poc-usage.ps1` — HTTP 200 + 세션/주간 % 출력되면 데이터 레이어 정상
- 실패 시 → [docs/api-schema.md](api-schema.md) §5 오류 처리 표 참조 (401이면 Claude Code 한 번 실행)

## 이번 세션 완료 (2026-07-27) — M1 PoC ✅ Go

| 작업 | 산출물 | 상태 |
|------|--------|------|
| 자격증명 스키마 확인 | api-schema.md §3.1 (토큰 값 미기록) | ✅ |
| usage 엔드포인트 역추적 | claude.exe v2.1.210 번들에서 `fetchUtilization` 추출 | ✅ |
| 엔드포인트 실호출 검증 | `GET /api/oauth/usage` → **HTTP 200** | ✅ |
| PoC 스크립트 | scripts/m1-poc-usage.ps1 | ✅ |
| 응답 스키마 문서화 | docs/api-schema.md | ✅ |
| Go/No-Go 판정 | **Go** — M2 진행 | ✅ |

### 검증된 계약 (요약)
```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <claudeAiOauth.accessToken>
anthropic-beta: oauth-2025-04-20

→ { five_hour:{utilization:0~100, resets_at:ISO8601}, seven_day:{...},
    seven_day_opus:null, limits:[{kind,percent,resets_at,scope,is_active}], ... }
```

## 다음 작업 우선순위

1. **`/bootstrap` 실행** — Tauri 2 기술스택 확정 + 스캐폴딩 (M1 Go 판정으로 선행조건 해소됨)
2. M2 코어 모듈: `credentials` → `usage-client` → `poller` (Rust). api-schema.md의 매핑(§4.3)·오류 표(§5)를 그대로 구현 스펙으로 사용
3. M3 위젯/트레이 UI

## 사용자 행동 필요
- **`/usage` 대조 (M1 잔여 1건)**: Claude Code 대화형 세션에서 `/usage`를 실행해 PoC 출력값과 일치하는지 눈으로 확인. `/usage`는 CLI 서브커맨드가 없어 자동화 불가.
  - PoC 최근 측정: session 17% (리셋 07-27 12:10) / weekly_all 21% (리셋 07-30 19:00) / weekly_scoped[Fable] 29%
  - ※ 대조 시점의 실제 값은 달라지므로, 두 값을 **같은 시각에** 비교할 것

## 알려진 이슈 / 모니터 대상
- 비공식 엔드포인트 — 경로·헤더·스키마가 예고 없이 변경 가능. `usage-client` 어댑터 분리 + 파싱 실패 시 크래시 대신 상태 강등 (api-schema.md §6)
- 베타 헤더 `oauth-2025-04-20` 만료 가능성 → 상수 1곳 집중 관리
- 실험적 버킷 필드(`tangelo`, `nimbus_quill` 등) 증식 중 → 명명 필드 하드코딩 대신 `limits[]` 배열 우선 사용
- 토큰 갱신은 본 앱 책임 아님(Claude Code 실행이 유일 경로). 만료 시 동작은 실제 만료 시점에 M2에서 확인 (tasks.md 1.1 잔여)
- PowerShell 5.1 스크립트는 **UTF-8 BOM 필수** (없으면 한글 주석이 다음 줄을 삼킴 — 실제 발생)
