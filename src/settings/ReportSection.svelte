<script lang="ts">
  import { queryDailyReport, queryHistory } from '../lib/ipc'
  import { severityOf } from '../lib/format'
  import type { DailyStat } from '../lib/types'

  /**
   * 기간별 사용량 리포트.
   *
   * 사용률은 한도 대비 %라 합산이 무의미하므로 "구간별 세션 최고치"를 본다.
   * - 오늘: 원본 스냅샷을 시간대(0~23시)로 접는다
   * - 7일/30일: 하루 요약(daily_stats) 그대로
   * - 1년: 하루 요약을 월로 접는다 (월 안에서 "일 최고치의 평균")
   * 하루 요약은 원본 보존 기간(90일)과 달리 지워지지 않아 1년까지 볼 수 있다.
   */

  type Period = 'today' | 'week' | 'month' | 'year'

  const PERIODS: { key: Period; label: string }[] = [
    { key: 'today', label: '오늘' },
    { key: 'week', label: '7일' },
    { key: 'month', label: '30일' },
    { key: 'year', label: '1년' },
  ]

  interface Bar {
    /** 축에 보여줄 라벨. 빽빽한 구간은 showLabel 로 솎는다 */
    label: string
    showLabel: boolean
    /** 세션 사용률 최고치 (0~100). null 이면 기록 없음 */
    value: number | null
    /** 호버 툴팁 */
    hint: string
  }

  let period = $state<Period>('week')
  let bars = $state<Bar[]>([])
  let summary = $state('')
  let failed = $state(false)
  let loading = $state(true)

  /** 탭을 빠르게 오갈 때 늦게 도착한 옛 응답이 새 화면을 덮지 않도록 */
  let requestId = 0

  $effect(() => {
    void load(period)
  })

  async function load(p: Period) {
    const id = ++requestId
    loading = true
    failed = false
    try {
      const next = p === 'today' ? await loadToday() : await loadDaily(p)
      if (id !== requestId) return
      bars = next.bars
      summary = next.summary
    } catch {
      if (id !== requestId) return
      failed = true
    } finally {
      if (id === requestId) loading = false
    }
  }

  const pct = (v: number) => `${Math.round(v)}%`

  /** 로컬 'YYYY-MM-DD' — Rust 쪽 daily_stats.day 와 같은 규약. */
  function localDayKey(d: Date): string {
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
  }

  /** 오늘: 자정부터 지금까지의 스냅샷을 시간대별 최고치로 접는다. */
  async function loadToday(): Promise<{ bars: Bar[]; summary: string }> {
    const start = new Date()
    start.setHours(0, 0, 0, 0)
    const rows = await queryHistory(start.toISOString(), new Date().toISOString())

    const peaks: (number | null)[] = Array.from({ length: 24 }, () => null)
    for (const r of rows) {
      if (r.sessionPct == null) continue
      const h = new Date(r.timestamp).getHours()
      peaks[h] = Math.max(peaks[h] ?? 0, r.sessionPct)
    }

    const bars = peaks.map<Bar>((v, h) => ({
      label: `${h}시`,
      showLabel: h % 6 === 0,
      value: v,
      hint: v != null ? `${h}시 · 세션 최고 ${pct(v)}` : `${h}시 · 기록 없음`,
    }))

    const seen = peaks.filter((v): v is number => v != null)
    const summary = seen.length
      ? `기록 ${seen.length}시간 · 세션 최고 ${pct(Math.max(...seen))}`
      : ''
    return { bars, summary }
  }

  /** 7일/30일은 하루 단위, 1년은 월 단위로 보여준다. */
  async function loadDaily(p: 'week' | 'month' | 'year'): Promise<{ bars: Bar[]; summary: string }> {
    const days = p === 'week' ? 7 : p === 'month' ? 30 : 366
    const stats = await queryDailyReport(days)
    const byDay = new Map(stats.map((s) => [s.day, s]))

    // 요약 수치는 표시 단위와 무관하게 "기록이 있는 날" 기준
    const recorded = stats.filter((s) => s.samples > 0 && s.peakSession != null)
    const peaks = recorded.map((s) => s.peakSession as number)
    const summary = recorded.length
      ? [
          `기록 ${recorded.length}일`,
          `일 최고 평균 ${pct(peaks.reduce((a, b) => a + b, 0) / peaks.length)}`,
          `최고 ${pct(Math.max(...peaks))}`,
          `90% 이상 ${peaks.filter((v) => v >= 90).length}일`,
        ].join(' · ')
      : ''

    if (p === 'year') return { bars: monthlyBars(stats), summary }

    // 기록 없는 날도 자리를 채워야 공백(위젯 꺼짐)이 패턴으로 읽힌다
    const bars: Bar[] = []
    for (let i = days - 1; i >= 0; i--) {
      const d = new Date()
      d.setDate(d.getDate() - i)
      const hit = byDay.get(localDayKey(d))
      const label = `${d.getMonth() + 1}/${d.getDate()}`
      bars.push({
        label,
        showLabel: p === 'week' ? true : i % 5 === 0,
        value: hit?.peakSession ?? null,
        hint:
          hit?.peakSession != null
            ? `${label} · 세션 최고 ${pct(hit.peakSession)}`
            : `${label} · 기록 없음`,
      })
    }
    return { bars, summary }
  }

  /** 1년 뷰: 이번 달부터 12개월 거슬러, 월 안에서 "일 최고치의 평균". */
  function monthlyBars(stats: DailyStat[]): Bar[] {
    const byMonth = new Map<string, number[]>()
    for (const s of stats) {
      if (s.peakSession == null) continue
      const key = s.day.slice(0, 7) // 'YYYY-MM'
      byMonth.set(key, [...(byMonth.get(key) ?? []), s.peakSession])
    }

    const bars: Bar[] = []
    const now = new Date()
    for (let i = 11; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1)
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`
      const days = byMonth.get(key)
      const avg = days ? days.reduce((a, b) => a + b, 0) / days.length : null
      bars.push({
        label: `${d.getMonth() + 1}월`,
        showLabel: true,
        value: avg,
        hint:
          avg != null
            ? `${d.getFullYear()}년 ${d.getMonth() + 1}월 · 일 최고 평균 ${pct(avg)} (${days!.length}일 기록)`
            : `${d.getFullYear()}년 ${d.getMonth() + 1}월 · 기록 없음`,
      })
    }
    return bars
  }

  const hasData = $derived(bars.some((b) => b.value != null))
</script>

<section>
  <div class="head">
    <h2>사용량 리포트</h2>
    <div class="tabs" role="tablist">
      {#each PERIODS as p (p.key)}
        <button
          role="tab"
          aria-selected={period === p.key}
          class:active={period === p.key}
          onclick={() => (period = p.key)}
        >
          {p.label}
        </button>
      {/each}
    </div>
  </div>

  {#if loading}
    <p class="note">불러오는 중…</p>
  {:else if failed}
    <p class="note">리포트를 불러올 수 없습니다</p>
  {:else if !hasData}
    <p class="note">이 기간에는 기록이 없습니다. 위젯이 켜져 있는 동안 자동으로 쌓입니다.</p>
  {:else}
    {#if summary}
      <p class="note">{summary}</p>
    {/if}
    <div class="chart">
      <div class="bars">
        {#each bars as b, i (i)}
          <div class="slot" title={b.hint}>
            {#if b.value != null}
              <div
                class="fill"
                data-severity={severityOf(b.value)}
                style:height="{Math.max(b.value, 2)}%"
              ></div>
            {/if}
          </div>
        {/each}
      </div>
      <div class="axis">
        {#each bars as b, i (i)}
          <span>{b.showLabel ? b.label : ''}</span>
        {/each}
      </div>
    </div>
  {/if}
</section>

<style>
  h2 {
    margin: 0;
    font-size: 0.83rem;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.5rem;
  }

  .tabs {
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--track);
    border-radius: 6px;
  }

  .tabs button {
    padding: 0.15rem 0.55rem;
    font: inherit;
    font-size: 0.74rem;
    color: var(--text-dim);
    background: none;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--text);
    background: var(--bg-solid);
  }

  .note {
    margin: 0 0 0.5rem;
    font-size: 0.76rem;
    color: var(--text-dim);
    line-height: 1.4;
  }

  .bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 72px;
    padding-bottom: 1px;
    border-bottom: 1px solid var(--border);
  }

  .slot {
    display: flex;
    align-items: flex-end;
    flex: 1;
    height: 100%;
    min-width: 0;
  }

  .fill {
    width: 100%;
    border-radius: 2px 2px 0 0;
    transition: height 0.25s ease;
  }
  .fill[data-severity='normal'] {
    background: var(--c-normal);
  }
  .fill[data-severity='warning'] {
    background: var(--c-warning);
  }
  .fill[data-severity='danger'] {
    background: var(--c-danger);
  }

  .axis {
    display: flex;
    gap: 2px;
    margin-top: 0.25rem;
  }
  .axis span {
    flex: 1;
    min-width: 0;
    overflow: visible;
    font-size: 0.66rem;
    color: var(--text-dim);
    text-align: center;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
