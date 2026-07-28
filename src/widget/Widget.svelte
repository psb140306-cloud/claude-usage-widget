<script lang="ts">
  import { onMount } from 'svelte'
  import Gauge from './Gauge.svelte'
  import { usage } from '../lib/state.svelte'
  import { relativeAge } from '../lib/format'
  import { hideWidget, openSettingsWindow, refreshNow, setWidgetMode } from '../lib/ipc'

  function report(action: string) {
    return (e: unknown) => console.error(`${action} 실패:`, e)
  }

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
  const compact = $derived(usage.compact)
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

    <span class="controls">
      <button
        class="ctl"
        onclick={() => openSettingsWindow().catch(report('설정 창 열기'))}
        title="설정"
        aria-label="설정">⚙</button>
      <!--
        진짜 최소화(window.minimize)는 쓰지 않는다. 이 창은 skipTaskbar 라
        최소화하면 작업 표시줄에 나타나지 않아 되돌릴 방법이 없다.
        대신 컴팩트 모드로 접는다 (PRD FR-4).
      -->
      <button
        class="ctl"
        onclick={() =>
          setWidgetMode(compact ? 'expanded' : 'compact').catch(report('모드 전환'))}
        title={compact ? '펼치기' : '접기'}
        aria-label={compact ? '펼치기' : '접기'}>{compact ? '▢' : '–'}</button>
      <button
        class="ctl close"
        onclick={() => hideWidget().catch(report('숨기기'))}
        title="숨기기 (트레이에서 다시 열 수 있습니다)"
        aria-label="숨기기">✕</button>
    </span>
  </header>

  {#if state.kind === 'loading'}
    <p class="msg">불러오는 중…</p>
  {:else if state.kind === 'needsReauth'}
    <p class="msg">Claude Code를 한 번 실행해<br />인증을 갱신해 주세요</p>
  {:else if state.kind === 'unavailable'}
    <p class="msg">사용량을 가져올 수 없습니다</p>
    <button onclick={() => refreshNow()}>다시 시도</button>
  {:else if snap}
    <!-- 컴팩트 모드에서는 게이지 2개만 남기고 리셋 안내·모델별 한도·푸터를 접는다 -->
    <Gauge
      label="세션 (5시간)"
      utilization={snap.session?.utilization}
      resetsAt={compact ? null : snap.session?.resetsAt}
      now={usage.now}
      muted={stale}
    />
    <Gauge
      label="주간 (7일)"
      utilization={snap.weekly?.utilization}
      resetsAt={compact ? null : snap.weekly?.resetsAt}
      now={usage.now}
      muted={stale}
    />
    {#if modelWindow && !compact}
      <Gauge
        label={`주간 (${modelWindow.displayName})`}
        utilization={modelWindow.utilization}
        resetsAt={modelWindow.resetsAt}
        now={usage.now}
        muted={stale}
        active={modelWindow.isActive}
      />
    {/if}

    {#if !compact}
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

  .controls {
    display: flex;
    flex: none;
    align-items: center;
    gap: 0.1rem;
  }

  .ctl {
    /* 프레임리스 창의 창 버튼. 트레이 위젯이라 실제 최소화 대신 접기를 쓴다 */
    display: grid;
    place-items: center;
    width: 1.05rem;
    height: 1.05rem;
    padding: 0;
    font: inherit;
    font-size: 0.7rem;
    line-height: 1;
    color: var(--text-dim);
    background: none;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }
  .ctl:hover {
    color: var(--text);
    background: var(--track);
  }
  .close:hover {
    color: #fff;
    background: #c42b1c; /* Windows 11 창 닫기 버튼과 같은 계열 */
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

  button:not(.ctl) {
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
