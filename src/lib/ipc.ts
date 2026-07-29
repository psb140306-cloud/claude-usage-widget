import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  EVENT,
  type AppInfo,
  type AppState,
  type DailyStat,
  type Environment,
  type HistoryEntry,
  type Settings,
  type WeekdayStat,
} from './types'

/**
 * Rust 커맨드 래퍼. 프론트는 이 모듈만 통해 백엔드와 통신한다.
 * 커맨드 이름은 `src-tauri/src/lib.rs` 의 `invoke_handler` 와 일치해야 한다.
 */

/** 현재 상태를 1회 조회 (창이 새로 열렸을 때 초기값 채우기용). */
export const getState = () => invoke<AppState>('get_state')

/** 계정·플랜·현재 세션(모델/effort/thinking). */
export const getEnvironment = () => invoke<Environment>('get_environment')

/** 설정 창 "정보" 섹션용. Cargo.toml 값을 그대로 읽어온다. */
export const getAppInfo = () => invoke<AppInfo>('get_app_info')

/** 수동 새로고침. Rust 쪽에서 5초 스로틀이 걸려 있다 (PRD FR-3). */
export const refreshNow = () => invoke<void>('refresh_now')

export const getSettings = () => invoke<Settings>('get_settings')

/** 부분 갱신 — 보낸 키만 바뀐다. 중첩된 `colors` 도 일부만 보낼 수 있다. */
export const updateSettings = (patch: Record<string, unknown>) =>
  invoke<Settings>('update_settings', { patch })

/** 히스토리 조회. from/to 는 ISO 8601. */
export const queryHistory = (from: string, to: string) =>
  invoke<HistoryEntry[]>('query_history', { from, to })

/** 요일별 사용 패턴. 최근 `days` 일치를 집계한다. */
export const queryWeekdayStats = (days = 30) =>
  invoke<WeekdayStat[]>('query_weekday_stats', { days })

/** 기간 리포트용 하루 요약. days 는 오늘 포함 (1~366). */
export const queryDailyReport = (days: number) =>
  invoke<DailyStat[]>('query_daily_report', { days })

/** 위젯 창 컴팩트↔확장 전환 (Rust 가 창 크기를 함께 조정한다). */
export const setWidgetMode = (mode: 'compact' | 'expanded') =>
  invoke<void>('set_widget_mode', { mode })

/** 위젯을 트레이로 숨긴다. 앱은 계속 상주한다. */
export const hideWidget = () => invoke<void>('hide_widget')

export const openSettingsWindow = () => invoke<void>('open_settings_window')

/** 상태 변경 구독. 반환된 함수를 호출하면 구독 해제. */
export const onState = (handler: (state: AppState) => void): Promise<UnlistenFn> =>
  listen<AppState>(EVENT.state, (e) => handler(e.payload))

export const onEnvironment = (handler: (env: Environment) => void): Promise<UnlistenFn> =>
  listen<Environment>(EVENT.env, (e) => handler(e.payload))

export const onSettings = (handler: (settings: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>(EVENT.settings, (e) => handler(e.payload))
