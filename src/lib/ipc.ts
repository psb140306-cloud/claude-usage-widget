import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { EVENT, type AppState, type HistoryEntry, type Settings } from './types'

/**
 * Rust 커맨드 래퍼. 프론트는 이 모듈만 통해 백엔드와 통신한다.
 * 커맨드 이름은 `src-tauri/src/lib.rs` 의 `invoke_handler` 와 일치해야 한다.
 */

/** 현재 상태를 1회 조회 (창이 새로 열렸을 때 초기값 채우기용). */
export const getState = () => invoke<AppState>('get_state')

/** 수동 새로고침. Rust 쪽에서 5초 스로틀이 걸려 있다 (PRD FR-3). */
export const refreshNow = () => invoke<void>('refresh_now')

export const getSettings = () => invoke<Settings>('get_settings')
export const updateSettings = (patch: Partial<Settings>) =>
  invoke<Settings>('update_settings', { patch })

/** 히스토리 조회. from/to 는 ISO 8601. */
export const queryHistory = (from: string, to: string) =>
  invoke<HistoryEntry[]>('query_history', { from, to })

/** 위젯 창 컴팩트↔확장 전환 (Rust 가 창 크기를 함께 조정한다). */
export const setWidgetMode = (mode: 'compact' | 'expanded') =>
  invoke<void>('set_widget_mode', { mode })

export const openSettingsWindow = () => invoke<void>('open_settings_window')

/** 상태 변경 구독. 반환된 함수를 호출하면 구독 해제. */
export function onState(handler: (state: AppState) => void): Promise<UnlistenFn> {
  return listen<AppState>(EVENT.state, (e) => handler(e.payload))
}
