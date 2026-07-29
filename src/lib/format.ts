import type { Severity } from './types'

/** PRD FR-4: 정상(<60) / 주의(60~85) / 위험(>85) */
export function severityOf(utilization: number | null | undefined): Severity {
  if (utilization == null) return 'normal'
  if (utilization > 85) return 'danger'
  if (utilization >= 60) return 'warning'
  return 'normal'
}

/** 게이지 폭 등에 쓰도록 0~100 으로 clamp. */
export function clampPct(utilization: number | null | undefined): number {
  if (utilization == null || Number.isNaN(utilization)) return 0
  return Math.min(100, Math.max(0, utilization))
}

/** 표시용 정수 %. Claude Code 와 동일하게 내림한다. */
export function displayPct(utilization: number | null | undefined): string {
  if (utilization == null) return '–'
  return `${Math.floor(utilization)}%`
}

/**
 * 리셋까지 남은 시간. PRD FR-4 상 1분 단위로 갱신되므로 분 해상도로 충분하다.
 * 이미 지났으면 "곧 리셋".
 */
export function countdown(resetsAt: string | null | undefined, now: Date = new Date()): string {
  if (!resetsAt) return '–'
  const target = new Date(resetsAt).getTime()
  if (Number.isNaN(target)) return '–'

  const diffMin = Math.floor((target - now.getTime()) / 60_000)
  if (diffMin <= 0) return '곧 리셋'

  const h = Math.floor(diffMin / 60)
  const m = diffMin % 60
  if (h >= 24) {
    const d = Math.floor(h / 24)
    return `${d}일 ${h % 24}시간 후`
  }
  if (h > 0) return `${h}:${String(m).padStart(2, '0')} 후`
  return `${m}분 후`
}

/**
 * 게이지 아래에 붙는 리셋 안내. 남은 시간의 크기에 따라 단위를 바꾼다.
 * 주간 한도는 며칠 단위라 "3일 후 리셋", 세션은 "9분 후 리셋" 처럼 나온다.
 */
export function resetsIn(resetsAt: string | null | undefined, now: Date = new Date()): string {
  if (!resetsAt) return ''
  const target = new Date(resetsAt).getTime()
  if (Number.isNaN(target)) return ''

  const min = Math.floor((target - now.getTime()) / 60_000)
  if (min <= 0) return '곧 리셋'
  if (min < 60) return `${min}분 후 리셋`

  const h = Math.floor(min / 60)
  if (h < 24) {
    const m = min % 60
    return m > 0 ? `${h}시간 ${m}분 후 리셋` : `${h}시간 후 리셋`
  }

  const d = Math.floor(h / 24)
  const restH = h % 24
  return restH > 0 ? `${d}일 ${restH}시간 후 리셋` : `${d}일 후 리셋`
}

/** 확장 모드용 절대 시각 (로컬 타임존). */
export function absoluteTime(resetsAt: string | null | undefined): string {
  if (!resetsAt) return '–'
  const d = new Date(resetsAt)
  if (Number.isNaN(d.getTime())) return '–'
  return d.toLocaleString('ko-KR', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/**
 * 게이지를 흑백으로 죽일 만큼 오래된 값인지.
 *
 * 폴링이 한 번 실패했다고 바로 흑백이 되면, 1분밖에 안 지난 값에도
 * 화면이 죽어 보인다. 뱃지는 첫 실패부터 띄우되 색은 이만큼 지나야 뺀다.
 */
export const STALE_MUTE_MINUTES = 5

export function isStaleEnoughToMute(fetchedAt: string, now: Date = new Date()): boolean {
  const t = new Date(fetchedAt).getTime()
  if (Number.isNaN(t)) return false
  return now.getTime() - t >= STALE_MUTE_MINUTES * 60_000
}

/** 스테일 뱃지용 "N분 전 기준". */
export function relativeAge(fetchedAt: string, now: Date = new Date()): string {
  const t = new Date(fetchedAt).getTime()
  if (Number.isNaN(t)) return ''
  const min = Math.floor((now.getTime() - t) / 60_000)
  if (min < 1) return '방금 기준'
  if (min < 60) return `${min}분 전 기준`
  const h = Math.floor(min / 60)
  return `${h}시간 전 기준`
}
