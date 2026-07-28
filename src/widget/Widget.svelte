<script lang="ts">
  import { onMount } from 'svelte'
  import Gauge from './Gauge.svelte'
  import { usage } from '../lib/state.svelte'
  import { relativeAge } from '../lib/format'
  import { openSettingsWindow, refreshNow } from '../lib/ipc'

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
  const account = $derived(usage.env?.account)
  const session = $derived(usage.env?.session)
  const modelWindow = $derived(usage.primaryModelWindow)
</script>

<!-- data-tauri-drag-region: 이 영역을 잡고 끌면 창이 이동한다 -->
<main data-tauri-drag-region>
  <!--
    헤더는 상태와 무관하게 항상 보인다. 조회 실패 중일 때야말로 설정을 열어
    폴링 주기를 바꾸고 싶을 수 있는데, 상태 분기 안에 두면 그때 버튼이 사라진다.
  -->
  <header data-tauri-drag-region>
    <span class="who">
      {#if account?.displayName}{account.displayName}{/if}
      {#if account?.planLabel}<span class="plan">{account.planLabel}</span>{/if}
    </span>
    <button
      class="cog"
      onclick={() => openSettingsWindow().catch((e) => console.error('설정 창 열기 실패:', e))}
      title="설정"
      aria-label="설정">⚙</button>
  </header>

  {#if state.kind === 'loading'}
    <p class="msg">불러오는 중…</p>
  {:else if state.kind === 'needsReauth'}
    <p class="msg">Claude Code를 한 번 실행해<br />인증을 갱신해 주세요</p>
  {:else if state.kind === 'unavailable'}
    <p class="msg">사용량을 가져올 수 없습니다</p>
    <button onclick={() => refreshNow()}>다시 시도</button>
  {:else if snap}
    <Gauge
      label="세션 (5시간)"
      utilization={snap.session?.utilization}
      resetsAt={snap.session?.resetsAt}
      now={usage.now}
      muted={stale}
    />
    <Gauge
      label="주간 (7일)"
      utilization={snap.weekly?.utilization}
      resetsAt={snap.weekly?.resetsAt}
      now={usage.now}
      muted={stale}
    />
    {#if modelWindow}
      <Gauge
        label={`주간 (${modelWindow.displayName})`}
        utilization={modelWindow.utilization}
        resetsAt={modelWindow.resetsAt}
        now={usage.now}
        muted={stale}
        active={modelWindow.isActive}
      />
    {/if}

    <footer>
      <span class="session-info">
        {#if session?.modelLabel}{session.modelLabel}{/if}
        {#if session?.effort}<span class="sep">·</span>{session.effort}{/if}
        {#if session?.thinking}<span class="sep">·</span><span title="thinking 활성">thinking</span>{/if}
      </span>
      {#if stale}
        <span class="badge">{relativeAge(snap.fetchedAt, usage.now)}</span>
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
    gap: 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    backdrop-filter: blur(20px);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    font-size: 0.68rem;
    color: var(--text-dim);
  }

  .who {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .plan {
    margin-left: 0.35rem;
    padding: 0.05rem 0.3rem;
    border-radius: 999px;
    background: var(--track);
    color: var(--text);
    font-size: 0.6rem;
  }

  .cog {
    flex: none;
    padding: 0 0.2rem;
    font-size: 0.72rem;
    line-height: 1;
    color: var(--text-dim);
    background: none;
    border: none;
    cursor: pointer;
  }
  .cog:hover {
    color: var(--text);
  }

  footer {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.4rem;
    padding-top: 0.15rem;
    border-top: 1px solid var(--border);
    font-size: 0.62rem;
    color: var(--text-dim);
  }

  .session-info {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sep {
    margin: 0 0.25rem;
    opacity: 0.5;
  }

  .msg {
    margin: 0;
    font-size: 0.72rem;
    line-height: 1.35;
    color: var(--text-dim);
    text-align: center;
  }

  .badge {
    flex: none;
    padding: 0.05rem 0.35rem;
    border-radius: 999px;
    background: var(--track);
  }

  button:not(.cog) {
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
</style>
