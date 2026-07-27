<script lang="ts">
  import { onMount } from 'svelte'
  import Gauge from './Gauge.svelte'
  import { usage } from '../lib/state.svelte'
  import { countdown, relativeAge } from '../lib/format'
  import { refreshNow } from '../lib/ipc'

  onMount(() => {
    // start() 는 async 이므로 정리 함수를 별도로 보관한다
    let dispose: (() => void) | undefined
    let cancelled = false

    usage.start().then((d) => {
      if (cancelled) d()
      else dispose = d
    })

    return () => {
      cancelled = true
      dispose?.()
    }
  })

  const state = $derived(usage.state)
  const snap = $derived(usage.snapshot)
  const stale = $derived(state.kind === 'stale')
</script>

<!--
  M3 에서 컴팩트/확장 모드 분리, 드래그 이동, 상태 UI 4종을 마저 구현한다.
  지금은 컴팩트 모드의 골격만 있다 (스캐폴딩 검증용).
  data-tauri-drag-region: 이 영역을 잡고 끌면 창이 이동한다.
-->
<main data-tauri-drag-region>
  {#if state.kind === 'loading'}
    <p class="msg">불러오는 중…</p>
  {:else if state.kind === 'needsReauth'}
    <p class="msg">Claude Code를 한 번 실행해<br />인증을 갱신해 주세요</p>
  {:else if state.kind === 'unavailable'}
    <p class="msg">사용량을 가져올 수 없습니다</p>
    <button onclick={() => refreshNow()}>다시 시도</button>
  {:else if snap}
    <Gauge label="세션" utilization={snap.session?.utilization} muted={stale} />
    <Gauge label="주간" utilization={snap.weekly?.utilization} muted={stale} />
    <footer>
      {#if stale}
        <span class="badge">{relativeAge(snap.fetchedAt, usage.now)}</span>
      {:else}
        <span class="reset">{countdown(snap.session?.resetsAt, usage.now)}</span>
      {/if}
    </footer>
  {/if}
</main>

<style>
  main {
    height: 100%;
    padding: 0.6rem 0.7rem;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.45rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    backdrop-filter: blur(20px);
  }

  .msg {
    margin: 0;
    font-size: 0.72rem;
    line-height: 1.35;
    color: var(--text-dim);
    text-align: center;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    font-size: 0.66rem;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .badge {
    padding: 0.05rem 0.35rem;
    border-radius: 999px;
    background: var(--surface);
  }

  button {
    align-self: center;
    padding: 0.2rem 0.6rem;
    font: inherit;
    font-size: 0.68rem;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
  }
  button:hover {
    background: var(--track);
  }
</style>
