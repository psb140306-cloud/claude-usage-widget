# usage API 스키마 (M1 PoC 검증 결과)

> 작성일: 2026-07-27
> 검증 대상: Claude Code v2.1.210 (win32-x64), plan=`max` / tier=`default_claude_max_20x`
> PoC 스크립트: [scripts/m1-poc-usage.ps1](../scripts/m1-poc-usage.ps1)
> ⚠️ **비공식 엔드포인트** — Anthropic의 공개 문서에 없으며 사전 통보 없이 변경될 수 있다.

## 1. 판정: **Go** ✅

| 검증 항목 | 결과 |
|---|---|
| 자격증명 파일 존재·파싱 | ✅ `%USERPROFILE%\.claude\.credentials.json` |
| 엔드포인트 응답 | ✅ `HTTP 200` |
| 세션(5시간) 사용률 | ✅ `five_hour.utilization` |
| 주간 사용률 | ✅ `seven_day.utilization` |
| 리셋 시각 | ✅ `resets_at` (ISO 8601, UTC 오프셋 포함) |
| Opus 등 모델별 한도 | ✅ `seven_day_opus` / `limits[].scope.model` |

같은 세션 안에서 두 번 호출했을 때 `five_hour`가 14.0 → 17.0으로 증가 — 값이 실제 사용량을 실시간 추종함을 확인.

## 2. 출처

값은 추측이 아니라 설치된 Claude Code 실행 파일에 포함된 JS 번들에서 직접 추출했다.

```js
// claude.exe 내 번들, function pTe() = fetchUtilization
Ei.get("/api/oauth/usage", {
  timeout: 5000,
  headers: { "Content-Type": "application/json" },
  refreshOAuth: true          // 401 → 토큰 리프레시 → 1회 재시도
})
```

```js
// 응답에서 기대하는 최상위 필드 화이트리스트 (변수 mOy)
["five_hour","seven_day","seven_day_oauth_apps","seven_day_opus",
 "seven_day_sonnet","cinder_cove","extra_usage","limits"]
```

이 중 **하나도 없으면** Claude Code는 "in-band error"로 간주하고 `status:"unavailable"`로 강등한다. 우리도 같은 판정 기준을 쓴다 (FR-2 "필수 필드 누락 시 조회 불가").

## 3. 요청

```http
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <claudeAiOauth.accessToken>
anthropic-beta: oauth-2025-04-20
Content-Type: application/json
Accept: application/json
```

- `anthropic-beta` 값은 번들 상수 `K2e = "oauth-2025-04-20"`.
- Claude Code 자체 타임아웃은 **5초**. 우리도 이에 준해 잡는다.
- 요청 바디 없음.

### 3.1 자격증명 파일 스키마

`%USERPROFILE%\.claude\.credentials.json` (읽기 전용)

```jsonc
{
  "claudeAiOauth": {
    "accessToken":           "<108자 문자열>",   // 절대 로그·저장 금지
    "refreshToken":          "<108자 문자열>",   // 사용하지 않음 (갱신은 Claude Code에 위임)
    "expiresAt":             1785133196889,      // epoch milliseconds
    "refreshTokenExpiresAt": 1786550038889,      // epoch milliseconds
    "scopes":                ["user:inference", "user:profile", "..."],  // 5개
    "subscriptionType":      "max",              // pro | max | team | enterprise
    "rateLimitTier":         "default_claude_max_20x"
  },
  "organizationUuid": "<uuid>",
  "mcpOAuth": { /* MCP 서버별 토큰 — 본 앱과 무관, 읽지 않는다 */ }
}
```

> ⚠️ `expiresAt`은 **초가 아니라 밀리초**다. 초로 오해하면 항상 "만료됨"으로 판정된다.
>
> 토큰 갱신은 본 앱이 하지 않는다. 만료 시 "Claude Code를 한 번 실행해 주세요" 안내로 강등한다 (PRD 시나리오 8).

## 4. 응답 (HTTP 200)

실제 응답 예시 (값은 검증 시점 기준):

```jsonc
{
  "five_hour": {
    "utilization": 17.0,                                  // 0~100 (백분율)
    "resets_at": "2026-07-27T03:09:59.869309+00:00",      // ISO 8601 + 오프셋
    "limit_dollars": null, "used_dollars": null, "remaining_dollars": null
  },
  "seven_day": { "utilization": 21.0, "resets_at": "2026-07-30T09:59:59.869329+00:00", /* …dollars */ },

  "seven_day_opus":       null,   // 해당 플랜에서 미적용 시 null
  "seven_day_sonnet":     null,
  "seven_day_oauth_apps": null,

  // ↓ 서버가 임의로 늘리는 실험적 버킷들. 이름을 하드코딩하지 말 것.
  "seven_day_cowork": null, "seven_day_omelette": null, "tangelo": null,
  "iguana_necktie": null, "omelette_promotional": null, "nimbus_quill": null,
  "cinder_cove": null, "amber_ladder": null,

  "extra_usage": {
    "is_enabled": false, "monthly_limit": null, "used_credits": null,
    "utilization": null, "currency": null, "decimal_places": null,
    "disabled_reason": null, "user_disabled": true,
    "spend_limit_reached": false, "credits_ever_enabled": true,
    "daily": null, "weekly": null
  },

  "limits": [
    { "kind": "session",       "group": "session", "percent": 17, "severity": "normal",
      "resets_at": "2026-07-27T03:09:59.869309+00:00", "scope": null, "is_active": false },
    { "kind": "weekly_all",    "group": "weekly",  "percent": 21, "severity": "normal",
      "resets_at": "2026-07-30T09:59:59.869329+00:00", "scope": null, "is_active": false },
    { "kind": "weekly_scoped", "group": "weekly",  "percent": 29, "severity": "normal",
      "resets_at": "2026-07-30T09:59:59.869624+00:00",
      "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null },
      "is_active": true }
  ],

  "spend": {
    "used": { "amount_minor": 0, "currency": "USD", "exponent": 2 },
    "limit": null, "percent": 0, "severity": "normal", "enabled": false,
    "disabled_reason": null, "cap": null, "balance": null, "auto_reload": null,
    "disclaimer": "...", "can_purchase_credits": false, "can_toggle": false
  },

  "member_dashboard_available": false
}
```

### 4.1 단위 규약 ⚠️

| 소스 | `utilization` 범위 |
|---|---|
| **본 엔드포인트 응답** | **0 ~ 100** (`17.0` = 17%) |
| 추론 응답 헤더 `anthropic-ratelimit-unified-*` | 0 ~ 1 (Claude Code가 `*100` 함) |

번들의 `seedUtilization()`이 헤더 값에 `*100`을 하는 걸 보고 응답 값에도 곱하면 **100배 부풀려진다**. 응답의 `five_hour.utilization`(17.0)과 `limits[kind=session].percent`(17)가 일치하는 것으로 0~100임을 확정했다.

Claude Code 렌더링도 `Math.floor(utilization)`로 그대로 %를 찍는다:

```js
i.push(`${title}: ${Math.floor(a.utilization)}% used${resets}`)
```

### 4.2 필드 상세

**버킷 객체** (`five_hour`, `seven_day`, `seven_day_*`) — 미적용 시 `null`

| 필드 | 타입 | 비고 |
|---|---|---|
| `utilization` | number \| null | 0~100. `null`이면 해당 버킷 표시 생략 |
| `resets_at` | string \| null | ISO 8601 (`+00:00` 오프셋 포함) |
| `limit_dollars` / `used_dollars` / `remaining_dollars` | number \| null | 구독 플랜에서는 전부 `null` |

**`limits[]`** — 서버가 주는 통합 목록. 새 한도 유형이 생기면 여기에 먼저 나타난다.

| 필드 | 타입 | 비고 |
|---|---|---|
| `kind` | string | `session` / `weekly_all` / `weekly_scoped` (확장 가능) |
| `group` | string | `session` / `weekly` |
| `percent` | number | 0~100 |
| `severity` | string | `normal` / … (색상 강등 힌트로 활용 가능) |
| `resets_at` | string | ISO 8601 |
| `scope` | object \| null | `scope.model.display_name` = 모델 버킷 라벨 (예: `Fable`) |
| `is_active` | bool | 현재 사용 중인 모델의 한도인지 |

> Claude Code는 `kind === "weekly_scoped"`인 항목을 `scope.model.display_name` 기준으로 필터링해 "Current week (Fable)" 형태로 렌더링한다.

### 4.3 우리 데이터 모델 매핑 (PRD 5.2)

```
UsageSnapshot.fetchedAt            := 로컬 수신 시각
UsageSnapshot.session.utilization  := five_hour.utilization
UsageSnapshot.session.resetsAt     := five_hour.resets_at
UsageSnapshot.weekly.utilization   := seven_day.utilization
UsageSnapshot.weekly.resetsAt      := seven_day.resets_at
UsageSnapshot.weeklyOpus?          := seven_day_opus (null 이면 미표시)
UsageSnapshot.modelScoped[]?       := limits[] where kind == "weekly_scoped"
                                      → { displayName, utilization: percent, resetsAt }
```

`five_hour`/`seven_day`가 **둘 다** 없으면 "조회 불가" 상태로 강등한다.

## 5. 오류 처리

| 상황 | 우리 동작 |
|---|---|
| `401` | "재인증 필요" — Claude Code 실행 안내. (Claude Code는 refresh 후 1회 재시도하지만, 본 앱은 토큰을 갱신하지 않는다) |
| `429` | Claude Code는 `rateLimitedVia:"http_429"`로 표시. 마지막 스냅샷 유지 + 스테일 표시 |
| `5xx` / 타임아웃 | 지수 백오프 최대 3회 (FR-2) → 실패 시 마지막 값 + 스테일 |
| 200이지만 알려진 필드 0개 | "조회 불가" (Claude Code의 in-band error 판정과 동일) |
| 알 수 없는 신규 필드 | **무시**. 이름 하드코딩 금지 — 실험적 버킷이 수시로 추가됨 |

## 6. 알려진 리스크

1. **비공식 API** — 경로·헤더·스키마가 예고 없이 바뀔 수 있다. `usage-client`를 어댑터로 분리하고, 파싱 실패가 크래시가 아닌 상태 강등이 되도록 한다.
2. **베타 헤더 만료** — `oauth-2025-04-20`이 갱신되면 값 교체가 필요하다. 상수 1곳에 모아둔다.
3. **실험 버킷 증식** — `tangelo`, `nimbus_quill` 등 코드네임 필드가 계속 늘어난다. `limits[]` 배열을 1차 소스로 쓰고 명명 필드는 보조로 쓰는 편이 변화에 강하다.
4. **토큰 만료** — `expiresAt` 경과 시 본 앱은 갱신하지 않는다. Claude Code 실행이 유일한 갱신 경로.

## 7. 구현 메모

- **PowerShell 5.1 스크립트는 UTF-8 BOM으로 저장할 것.** BOM이 없으면 CP949로 읽혀 한글 주석 끝 글자가 개행을 삼키고 다음 줄이 주석에 흡수된다 (PoC 작성 중 실제로 발생).
- `Invoke-WebRequest`에는 `-UseBasicParsing` 필수. 없으면 IE 엔진을 호출하려다 비대화형 환경에서 실패한다.
