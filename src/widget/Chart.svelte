<script lang="ts">
  import { onMount } from 'svelte'
  import uPlot from 'uplot'
  import 'uplot/dist/uPlot.min.css'
  import { queryHistory } from '../lib/ipc'
  import type { Colors, HistoryEntry } from '../lib/types'

  interface Props {
    /** 표시할 시간 범위 (시간) */
    hours?: number
    colors: Colors
    /** 폴링이 끝날 때마다 올라가는 값. 바뀌면 다시 읽는다 */
    revision?: unknown
  }

  let { hours = 24, colors, revision }: Props = $props()

  let host = $state<HTMLDivElement | null>(null)
  let chart: uPlot | null = null
  let empty = $state(true)
  let failed = $state<string | null>(null)

  /** HistoryEntry[] → uPlot 이 원하는 [x[], y1[], y2[]] 형태 */
  function toSeries(rows: HistoryEntry[]): uPlot.AlignedData {
    const x: number[] = []
    const session: (number | null)[] = []
    const weekly: (number | null)[] = []
    for (const r of rows) {
      x.push(new Date(r.timestamp).getTime() / 1000)
      session.push(r.sessionPct)
      weekly.push(r.weeklyPct)
    }
    return [x, session, weekly]
  }

  function options(width: number): uPlot.Options {
    return {
      width,
      height: 72,
      // 위젯 안에 들어가는 스파크라인에 가깝다 — 범례·커서는 군더더기다
      legend: { show: false },
      cursor: { show: false },
      scales: { y: { range: [0, 100] } },
      axes: [
        {
          stroke: colors.textDim,
          grid: { show: false },
          ticks: { show: false },
          size: 18,
          font: '9px system-ui',
        },
        {
          stroke: colors.textDim,
          grid: { stroke: colors.gaugeTrack, width: 1 },
          ticks: { show: false },
          size: 26,
          font: '9px system-ui',
          values: (_u, splits) => splits.map((v) => `${v}%`),
        },
      ],
      series: [
        {},
        { label: '세션', stroke: colors.gaugeNormal, width: 1.5, points: { show: false } },
        { label: '주간', stroke: colors.gaugeWarning, width: 1.5, points: { show: false } },
      ],
    }
  }

  async function load() {
    if (!host) return
    const to = new Date()
    const from = new Date(to.getTime() - hours * 3600_000)

    try {
      const rows = await queryHistory(from.toISOString(), to.toISOString())
      failed = null
      empty = rows.length < 2 // 점 하나로는 선이 안 그려진다

      const data = toSeries(rows)
      if (chart) {
        chart.setData(data)
      } else if (!empty) {
        chart = new uPlot(options(host.clientWidth || 220), data, host)
      }
    } catch (e) {
      failed = String(e)
    }
  }

  onMount(() => {
    load()
    return () => {
      chart?.destroy()
      chart = null
    }
  })

  // 새 스냅샷이 저장될 때마다 다시 그린다
  $effect(() => {
    void revision
    load()
  })
</script>

<div class="chart" bind:this={host}>
  {#if failed}
    <p class="note">차트를 불러올 수 없습니다</p>
  {:else if empty}
    <p class="note">기록을 모으는 중… ({hours}시간 추이)</p>
  {/if}
</div>

<style>
  .chart {
    width: 100%;
    min-height: 72px;
  }

  .note {
    margin: 0;
    padding: 1.5rem 0;
    font-size: 0.62rem;
    color: var(--text-dim);
    text-align: center;
  }

  /* uPlot 이 만든 요소는 이 컴포넌트 밖에서 생성되므로 :global 이 필요하다 */
  .chart :global(.u-wrap) {
    font-variant-numeric: tabular-nums;
  }
</style>
