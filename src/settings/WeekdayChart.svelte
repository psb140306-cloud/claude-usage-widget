<script lang="ts">
  import { onMount } from 'svelte'
  import { queryWeekdayStats } from '../lib/ipc'
  import { severityOf } from '../lib/format'
  import type { WeekdayStat } from '../lib/types'

  interface Props {
    /** 집계 기간 (일) */
    days?: number
  }

  let { days = 30 }: Props = $props()

  /** SQLite strftime('%w') 규약: 0 = 일요일 */
  const LABELS = ['일', '월', '화', '수', '목', '금', '토']

  let stats = $state<WeekdayStat[]>([])
  let failed = $state(false)
  let loaded = $state(false)

  /** 요일 7칸을 항상 채운다 — 기록 없는 요일도 자리를 비워 보여줘야 패턴이 읽힌다 */
  const rows = $derived(
    LABELS.map((label, weekday) => {
      const hit = stats.find((s) => s.weekday === weekday)
      return {
        label,
        value: hit?.avgPeakSession ?? null,
        days: hit?.days ?? 0,
      }
    }),
  )

  const hasData = $derived(rows.some((r) => r.value != null))

  onMount(async () => {
    try {
      stats = await queryWeekdayStats(days)
    } catch {
      failed = true
    } finally {
      loaded = true
    }
  })
</script>

<section>
  <h2>사용 패턴</h2>

  {#if !loaded}
    <p class="note">불러오는 중…</p>
  {:else if failed}
    <p class="note">패턴을 불러올 수 없습니다</p>
  {:else if !hasData}
    <p class="note">
      아직 기록이 부족합니다. 며칠 사용하면 요일별 패턴이 나타납니다.
    </p>
  {:else}
    <p class="note">최근 {days}일 · 요일별 세션 최고 사용률 평균</p>
    <div class="bars">
      {#each rows as r (r.label)}
        <div class="col" title={r.days > 0 ? `${r.days}일치 기록` : '기록 없음'}>
          <span class="value">{r.value != null ? `${Math.round(r.value)}%` : '–'}</span>
          <div class="track">
            <div
              class="fill"
              data-severity={severityOf(r.value)}
              style:height="{r.value ?? 0}%"
            ></div>
          </div>
          <span class="label" class:empty={r.days === 0}>{r.label}</span>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  h2 {
    margin: 0 0 0.5rem;
    font-size: 0.83rem;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .note {
    margin: 0 0 0.5rem;
    font-size: 0.76rem;
    color: var(--text-dim);
    line-height: 1.4;
  }

  .bars {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 0.35rem;
    align-items: end;
  }

  .col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.2rem;
  }

  .value {
    font-size: 0.68rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .track {
    display: flex;
    align-items: flex-end;
    width: 100%;
    height: 64px;
    background: var(--track);
    border-radius: 3px;
    overflow: hidden;
  }

  .fill {
    width: 100%;
    border-radius: 3px;
    transition: height 0.3s ease;
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

  .label {
    font-size: 0.76rem;
    color: var(--text);
  }
  .label.empty {
    color: var(--text-dim);
    opacity: 0.5;
  }
</style>
