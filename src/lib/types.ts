/**
 * Rust 사이드(`src-tauri/src/`)와 1:1 대응하는 타입.
 * Rust 는 `#[serde(rename_all = "camelCase")]` 로 직렬화한다.
 * 한쪽을 바꾸면 반드시 다른 쪽도 바꿀 것.
 */

/** 하나의 한도 창(세션 / 주간 / Opus 주간). utilization 은 0~100 스케일. */
export interface LimitWindow {
  /** 0 ~ 100 (백분율). usage API 가 이미 백분율로 준다 — 100 을 곱하지 말 것. */
  utilization: number
  /** ISO 8601 (UTC) */
  resetsAt: string | null
}

/** `limits[]` 중 kind === "weekly_scoped" 인 모델별 주간 한도. */
export interface ModelWindow {
  displayName: string
  utilization: number
  resetsAt: string | null
  /** 현재 사용 중인 모델의 한도인지 */
  isActive: boolean
}

/** 폴링 1회 결과. */
export interface UsageSnapshot {
  /** 로컬 수신 시각 (ISO 8601) */
  fetchedAt: string
  session: LimitWindow | null
  weekly: LimitWindow | null
  weeklyOpus: LimitWindow | null
  modelScoped: ModelWindow[]
}

/**
 * 위젯이 렌더링해야 하는 상태 (PRD 5.1 "상태 표현" 4종).
 * Rust 의 `AppState` enum 이 `{ kind: "...", ... }` 형태로 직렬화된다.
 */
export type AppState =
  | { kind: 'ok'; snapshot: UsageSnapshot }
  | { kind: 'stale'; snapshot: UsageSnapshot; reason: string }
  | { kind: 'needsReauth' }
  | { kind: 'unavailable'; reason: string }
  | { kind: 'loading' }

/** 계정·플랜. `~/.claude.json` 의 oauthAccount 에서 온다. 이메일은 읽지 않는다. */
export interface Account {
  displayName: string | null
  /** `max`, `pro` … */
  plan: string | null
  /** 사람이 읽는 라벨 (`Max 20x`) */
  planLabel: string | null
}

/** 현재 Claude Code 세션. 트랜스크립트에서 메타데이터만 읽는다. */
export interface Session {
  model: string | null
  /** `Opus 5` */
  modelLabel: string | null
  /** `low` / `medium` / `high` / `xhigh` / `max` */
  effort: string | null
  thinking: boolean
  /** 이 값들의 출처 프로젝트. 세션이 여러 개일 때 어느 쪽인지 알려준다 */
  sourceProject: string | null
}

export interface Environment {
  account: Account
  session: Session
}

/** 하루 요약 (기간 리포트). day 는 로컬 날짜 'YYYY-MM-DD'. */
export interface DailyStat {
  day: string
  /** 그 날 세션 사용률 최고치 (0~100) */
  peakSession: number | null
  peakWeekly: number | null
  peakOpus: number | null
  /** 그 날 저장된 스냅샷 수 — 0이면 위젯이 꺼져 있던 날 */
  samples: number
}

/** 요일별 사용 패턴 (FR-7). weekday 는 0=일요일. */
export interface WeekdayStat {
  weekday: number
  /** 그 요일 "하루 최고 세션 사용률"의 평균 */
  avgPeakSession: number | null
  avgPeakWeekly: number | null
  /** 집계에 들어간 날 수 */
  days: number
}

/** 설정 창 "정보" 섹션. Cargo.toml 에서 온다. */
export interface AppInfo {
  name: string
  version: string
  authors: string
  license: string
  repository: string
}

/** 사용률 구간 (PRD FR-4: 정상 <60 / 주의 60~85 / 위험 >85) */
export type Severity = 'normal' | 'warning' | 'danger'

export type Theme = 'light' | 'dark' | 'system'
export type WidgetMode = 'compact' | 'expanded'

/** 위젯 색상. 전부 `#rrggbb`. */
export interface Colors {
  text: string
  textDim: string
  gaugeNormal: string
  gaugeWarning: string
  gaugeDanger: string
  gaugeTrack: string
  background: string
  /** 차트 선 색. 게이지 구간 색과 분리해 둔다 (주간선이 "주의"로 읽히지 않도록) */
  chartSession: string
  chartWeekly: string
}

/** 로컬 JSON 으로 저장되는 설정 (PRD FR-8). */
export interface Settings {
  /** 30 ~ 600 초 */
  pollingIntervalSec: number
  thresholds: number[]
  notificationsEnabled: boolean
  notifyOnReset: boolean
  theme: Theme
  /** 0.3 ~ 1.0 — 위젯 배경 불투명도 */
  opacity: number
  autoStart: boolean
  widgetMode: WidgetMode
  widgetPosition: { x: number; y: number } | null
  /** 확장 모드에서 사용자가 조정한 크기 (논리 px). 없으면 기본 260×390 */
  widgetSize: { width: number; height: number } | null
  colors: Colors
}

/** 히스토리 1행 (PRD FR-7). */
export interface HistoryEntry {
  timestamp: string
  sessionPct: number | null
  weeklyPct: number | null
  opusPct: number | null
}

/** Rust → 프론트 이벤트 이름. `src-tauri/src/` 와 공유. */
export const EVENT = {
  /** 새 스냅샷 또는 상태 전이 (poller::EVENT_STATE) */
  state: 'usage://state',
  /** 계정·세션 정보 (poller::EVENT_ENV) */
  env: 'usage://env',
  /** 설정 변경 (settings::EVENT_SETTINGS) */
  settings: 'settings://changed',
} as const
