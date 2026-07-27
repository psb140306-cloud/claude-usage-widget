<script lang="ts">
  import { clampPct, displayPct, severityOf } from '../lib/format'

  interface Props {
    label: string
    utilization: number | null | undefined
    /** 데이터가 오래됐거나 조회 불가일 때 채도를 낮춘다 */
    muted?: boolean
  }

  let { label, utilization, muted = false }: Props = $props()

  const pct = $derived(clampPct(utilization))
  const severity = $derived(severityOf(utilization))
</script>

<div class="gauge" class:muted>
  <span class="label">{label}</span>
  <div class="track" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={label}>
    <div class="fill" data-severity={severity} style:width="{pct}%"></div>
  </div>
  <span class="value">{displayPct(utilization)}</span>
</div>

<style>
  .gauge {
    display: grid;
    grid-template-columns: 2.4rem 1fr 2.6rem;
    align-items: center;
    gap: 0.5rem;
  }

  .label {
    font-size: 0.7rem;
    color: var(--text-dim);
  }

  .track {
    height: 6px;
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

  .value {
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .muted {
    opacity: 0.45;
    filter: grayscale(1);
  }
</style>
