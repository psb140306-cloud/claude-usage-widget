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

  function options(width: number, height: number): uPlot.Options {
    return {
      width,
      height,
      // 위젯 안에 들어가는 스파크라인에 가깝다 — 범례·커서는 군더더기다
      legend: { show: false },
      cursor: { show: false },
      scales: { y: { range: [0, 100] } },
      axes: [
        {
          stroke: colors.textDim,
          grid: { show: false },
          ticks: { show: false },
          size: 22,
          font: '11px system-ui',
        },
        {
          stroke: colors.textDim,
          grid: { stroke: colors.gaugeTrack, width: 1 },
          ticks: { show: false },
          size: 32,
          font: '11px system-ui',
          values: (_u, splits) => splits.map((v) => `${v}%`),
        },
      ],
      series: [
        {},
        { label: '세션', stroke: colors.chartSession, width: 1.5, points: { show: false } },
        { label: '주간', stroke: colors.chartWeekly, width: 1.5, points: { show: false } },
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
        chart = new uPlot(
          options(host.clientWidth || 220, host.clientHeight || 72),
          data,
          host,
        )
      }
    } catch (e) {
      failed = String(e)
    }
  }

  onMount(() => {
    load()

    // 창을 세로로 늘리면 차트가 남는 공간을 채운다 (.chart 의 flex:1).
    // uPlot 은 명시적 픽셀 크기가 필요하므로 컨테이너 크기를 따라가게 한다.
    // 리사이즈 드래그 중 이벤트가 몰리므로 프레임당 한 번만 반영한다.
    let raf = 0
    const observer = new ResizeObserver(() => {
      if (raf) return
      raf = requestAnimationFrame(() => {
        raf = 0
        if (chart && host) {
          chart.setSize({ width: host.clientWidth, height: host.clientHeight })
        }
      })
    })
    if (host) observer.observe(host)

    return () => {
      observer.disconnect()
      if (raf) cancelAnimationFrame(raf)
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

<!--
  uPlot 기본 범례는 커서를 따라다니는 대화형이라 이 크기엔 맞지 않는다.
  어느 선이 무엇인지만 알면 되므로 정적인 범례를 직접 그린다.
-->
{#if !failed && !empty}
  <div class="legend">
    <span class="key">최근 {hours}시간</span>
    <span class="item"><i style:background={colors.chartSession}></i>세션</span>
    <span class="item"><i style:background={colors.chartWeekly}></i>주간</span>
  </div>
{/if}

<div class="chart" bind:this={host}>
  {#if failed}
    <p class="note">차트를 불러올 수 없습니다</p>
  {:else if empty}
    <p class="note">기록을 모으는 중… ({hours}시간 추이)</p>
  {/if}
</div>

<style>
  .legend {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.68rem;
    color: var(--text-dim);
  }

  .key {
    margin-right: auto;
  }

  .item {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }

  .item i {
    width: 0.7rem;
    height: 2px;
    border-radius: 1px;
  }

  .chart {
    width: 100%;
    min-height: 72px;
    /* main(flex column)의 남는 세로 공간을 차트가 가져간다 — 창을 늘리면 차트가 커진다 */
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .note {
    margin: 0;
    padding: 1.5rem 0;
    font-size: 0.72rem;
    color: var(--text-dim);
    text-align: center;
  }

  /* uPlot 이 만든 요소는 이 컴포넌트 밖에서 생성되므로 :global 이 필요하다 */
  .chart :global(.u-wrap) {
    font-variant-numeric: tabular-nums;
  }
</style>
