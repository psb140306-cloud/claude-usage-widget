/**
 * Rust 사이드(`src-tauri/src/model.rs`)와 1:1 대응하는 타입.
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
  /** 정상 — 색상 게이지 */
  | { kind: 'ok'; snapshot: UsageSnapshot }
  /** 스테일 — 마지막 값 + "N분 전 기준" 뱃지 */
  | { kind: 'stale'; snapshot: UsageSnapshot; reason: string }
  /** 재인증 필요 — "Claude Code를 한 번 실행해 주세요" */
  | { kind: 'needsReauth' }
  /** 조회 불가 — 회색 + 재시도 버튼 */
  | { kind: 'unavailable'; reason: string }
  /** 최초 로딩 */
  | { kind: 'loading' }

/** 사용률 구간 (PRD FR-4: 정상 <60 / 주의 60~85 / 위험 >85) */
export type Severity = 'normal' | 'warning' | 'danger'

export type Theme = 'light' | 'dark' | 'system'
export type WidgetMode = 'compact' | 'expanded'

/** 로컬 JSON 으로 저장되는 설정 (PRD FR-8). */
export interface Settings {
  /** 30 ~ 600 초. 기본 60 */
  pollingIntervalSec: number
  /** 알림 임계값 %. 기본 [80, 95] */
  thresholds: number[]
  notificationsEnabled: boolean
  /** 리셋 완료 알림. 기본 false */
  notifyOnReset: boolean
  theme: Theme
  /** 0.3 ~ 1.0 */
  opacity: number
  autoStart: boolean
  widgetMode: WidgetMode
  widgetPosition: { x: number; y: number } | null
}

/** 히스토리 1행 (PRD FR-7). */
export interface HistoryEntry {
  /** ISO 8601 */
  timestamp: string
  sessionPct: number | null
  weeklyPct: number | null
  opusPct: number | null
}

/** Rust → 프론트 이벤트 이름. `src-tauri/src/poller.rs` 와 공유. */
export const EVENT = {
  /** 새 스냅샷 또는 상태 전이 */
  state: 'usage://state',
} as const
