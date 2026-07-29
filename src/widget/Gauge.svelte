<script lang="ts">
  import { clampPct, displayPct, resetsIn, severityOf } from '../lib/format'

  interface Props {
    label: string
    utilization: number | null | undefined
    /** ISO 8601. 있으면 게이지 아래에 "N분 후 리셋" 을 보여준다 */
    resetsAt?: string | null
    /** 카운트다운 재계산용 시계 (1분 tick) */
    now?: Date
    /** 데이터가 오래됐거나 조회 불가일 때 채도를 낮춘다 */
    muted?: boolean
    /** 현재 사용 중인 모델 한도 표시 */
    active?: boolean
  }

  let {
    label,
    utilization,
    resetsAt = null,
    now = new Date(),
    muted = false,
    active = false,
  }: Props = $props()

  const pct = $derived(clampPct(utilization))
  const severity = $derived(severityOf(utilization))
  const reset = $derived(resetsIn(resetsAt, now))
</script>

<div class="gauge" class:muted>
  <div class="top">
    <span class="label">
      {label}{#if active}<span class="dot" title="현재 사용 중">●</span>{/if}
    </span>
    <span class="value">{displayPct(utilization)}</span>
  </div>

  <div
    class="track"
    role="progressbar"
    aria-valuenow={pct}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-label={label}
  >
    <div class="fill" data-severity={severity} style:width="{pct}%"></div>
  </div>

  {#if reset}
    <span class="reset">{reset}</span>
  {/if}
</div>

<style>
  .gauge {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  /* 글자 크기는 2026-07-29 에 전체적으로 1pt(≈0.08rem) 올렸다.
     기존 크기는 실사용에서 읽기 어렵다는 피드백이 있었다. */
  .label {
    font-size: 0.8rem;
    color: var(--text);
  }

  .dot {
    margin-left: 0.25rem;
    font-size: 0.55rem;
    color: var(--c-normal);
    vertical-align: middle;
  }

  .value {
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  .track {
    height: 5px;
    border-radius: 3px;
    background: var(--track);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.35s ease;
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

  .reset {
    font-size: 0.7rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .muted {
    opacity: 0.5;
    filter: grayscale(1);
  }
</style>
