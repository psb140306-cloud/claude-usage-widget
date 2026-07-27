import type { UnlistenFn } from '@tauri-apps/api/event'
import { getState, onState } from './ipc'
import type { AppState, UsageSnapshot } from './types'

/**
 * 위젯 전역 상태 (Svelte 5 runes).
 *
 * - `state`  : Rust 가 밀어주는 최신 상태
 * - `now`    : 카운트다운 갱신용 시계. PRD FR-4 대로 1분 단위로만 tick 한다
 *              (초 단위로 돌리면 상주 앱 CPU 목표에 불리).
 */
class UsageStore {
  state = $state<AppState>({ kind: 'loading' })
  now = $state<Date>(new Date())

  /** 상태 종류와 무관하게 마지막으로 받은 스냅샷 (스테일 표시용). */
  get snapshot(): UsageSnapshot | null {
    const s = this.state
    return s.kind === 'ok' || s.kind === 'stale' ? s.snapshot : null
  }

  /** 트레이 아이콘 색상과 동일한 기준: 세션/주간 중 최고값. */
  get peakUtilization(): number | null {
    const snap = this.snapshot
    if (!snap) return null
    const vals = [snap.session?.utilization, snap.weekly?.utilization].filter(
      (v): v is number => v != null,
    )
    return vals.length > 0 ? Math.max(...vals) : null
  }

  /** 구독 시작. 반환값을 호출하면 정리된다. */
  async start(): Promise<() => void> {
    let unlisten: UnlistenFn | undefined
    try {
      unlisten = await onState((s) => {
        this.state = s
      })
      this.state = await getState()
    } catch (err) {
      // 백엔드가 아직 준비되지 않았거나 IPC 실패 — 크래시 대신 상태 강등 (PRD 6 안정성)
      this.state = { kind: 'unavailable', reason: String(err) }
    }

    const timer = setInterval(() => {
      this.now = new Date()
    }, 60_000)

    return () => {
      unlisten?.()
      clearInterval(timer)
    }
  }
}

export const usage = new UsageStore()
