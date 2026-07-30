import type { UnlistenFn } from '@tauri-apps/api/event'
import { getEnvironment, getSettings, getState, onEnvironment, onSettings, onState } from './ipc'
import type { AppState, Colors, Environment, Settings, Theme, UsageSnapshot } from './types'

/**
 * 테마별 색 프리셋.
 *
 * 색은 전부 설정값이 CSS 변수를 덮어쓰므로, `data-theme` 속성만 바꿔서는
 * 아무것도 달라지지 않는다. 그래서 테마는 **색 묶음을 갈아끼우는** 방식으로 둔다.
 * 개별 색을 손본 뒤 테마를 바꾸면 그 테마의 기본색으로 돌아간다 — 설정 UI 에 명시한다.
 */
export const PRESETS: Record<'light' | 'dark', Colors> = {
  dark: {
    text: '#f3f3f3',
    textDim: '#8b8b8b',
    gaugeNormal: '#3fb950',
    gaugeWarning: '#d29922',
    gaugeDanger: '#f85149',
    gaugeTrack: '#3a3a3a',
    background: '#202020',
    chartSession: '#3fb950',
    chartWeekly: '#58a6ff',
  },
  light: {
    text: '#1a1a1a',
    textDim: '#5c5c5c',
    gaugeNormal: '#1a7f37',
    gaugeWarning: '#9a6700',
    gaugeDanger: '#cf222e',
    gaugeTrack: '#d0d7de',
    background: '#f9f9f9',
    chartSession: '#1a7f37',
    chartWeekly: '#0969da',
  },
}

/** `system` 이면 OS 설정을 따라 실제 테마를 정한다. */
export function resolveTheme(theme: Theme): 'light' | 'dark' {
  if (theme !== 'system') return theme
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

/** 백엔드 응답 전 첫 프레임용. Rust 의 `Settings::default()` 와 같은 값. */
const FALLBACK_SETTINGS: Settings = {
  pollingIntervalSec: 60,
  thresholds: [80, 95],
  notificationsEnabled: true,
  notifyOnReset: false,
  theme: 'system',
  autoStart: false,
  widgetMode: 'expanded',
  widgetPosition: null,
  widgetSize: null,
  opacity: 0.85,
  colors: {
    text: '#f3f3f3',
    textDim: '#8b8b8b',
    gaugeNormal: '#3fb950',
    gaugeWarning: '#d29922',
    gaugeDanger: '#f85149',
    gaugeTrack: '#3a3a3a',
    background: '#202020',
    chartSession: '#3fb950',
    chartWeekly: '#58a6ff',
  },
}

/** `#rrggbb` + 알파 → `rgba(...)`. 잘못된 값이 오면 원본을 그대로 돌려준다. */
export function withAlpha(hex: string, alpha: number): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim())
  if (!m) return hex
  const n = parseInt(m[1], 16)
  const r = (n >> 16) & 255
  const g = (n >> 8) & 255
  const b = n & 255
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/**
 * 위젯 전역 상태 (Svelte 5 runes).
 *
 * `now` 는 카운트다운 갱신용 시계. PRD FR-4 대로 1분 단위로만 tick 한다
 * (초 단위로 돌리면 상주 앱 CPU 목표에 불리).
 */
class UsageStore {
  state = $state<AppState>({ kind: 'loading' })
  env = $state<Environment | null>(null)
  settings = $state<Settings>(FALLBACK_SETTINGS)
  now = $state<Date>(new Date())

  get compact(): boolean {
    return this.settings.widgetMode === 'compact'
  }

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

  /**
   * 모델별 주간 한도 중 대표 1개.
   * 현재 사용 중(`isActive`)인 것을 우선하고, 없으면 사용률이 가장 높은 것.
   */
  get primaryModelWindow() {
    const list = this.snapshot?.modelScoped ?? []
    if (list.length === 0) return null
    return list.find((m) => m.isActive) ?? [...list].sort((a, b) => b.utilization - a.utilization)[0]
  }

  /** 설정 색상을 CSS 변수로 문서에 반영한다. */
  applyTheme() {
    const { colors, opacity, theme } = this.settings
    const root = document.documentElement
    // 설정에서 덮지 않는 부분(스크롤바·폼 컨트롤 등)이 OS 다크모드를 따라가도록
    root.dataset.theme = resolveTheme(theme)
    root.style.setProperty('--text', colors.text)
    root.style.setProperty('--text-dim', colors.textDim)
    root.style.setProperty('--c-normal', colors.gaugeNormal)
    root.style.setProperty('--c-warning', colors.gaugeWarning)
    root.style.setProperty('--c-danger', colors.gaugeDanger)
    root.style.setProperty('--track', colors.gaugeTrack)
    root.style.setProperty('--bg', withAlpha(colors.background, opacity))
    root.style.setProperty('--bg-solid', colors.background)
  }

  /** 구독 시작. 반환값을 호출하면 정리된다. */
  async start(): Promise<() => void> {
    const unlisten: UnlistenFn[] = []

    try {
      unlisten.push(await onState((s) => (this.state = s)))
      unlisten.push(await onEnvironment((e) => (this.env = e)))
      unlisten.push(
        await onSettings((s) => {
          this.settings = s
          this.applyTheme()
        }),
      )

      // 이벤트를 놓쳤을 수 있으니 현재 값을 한 번 끌어온다
      this.settings = await getSettings()
      this.applyTheme()
      this.env = await getEnvironment()
      this.state = await getState()
    } catch (err) {
      // 백엔드가 아직 준비되지 않았거나 IPC 실패 — 크래시 대신 상태 강등 (PRD 6 안정성)
      this.state = { kind: 'unavailable', reason: String(err) }
    }

    const timer = setInterval(() => (this.now = new Date()), 60_000)

    return () => {
      unlisten.forEach((fn) => fn())
      clearInterval(timer)
    }
  }
}

export const usage = new UsageStore()
