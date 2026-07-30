<script lang="ts">
  import { onMount } from 'svelte'
  import Chart from './Chart.svelte'
  import Gauge from './Gauge.svelte'
  import { usage } from '../lib/state.svelte'
  import { ellipsize, isStaleEnoughToMute, relativeAge } from '../lib/format'
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

  // `state` 라는 이름은 피한다 — 같은 스코프의 $state 룬이 스토어 구독($변수)으로 해석된다
  const appState = $derived(usage.state)
  const snap = $derived(usage.snapshot)
  const stale = $derived(appState.kind === 'stale')
  // 뱃지는 첫 실패부터, 흑백 처리는 값이 충분히 오래됐을 때만
  const muted = $derived(stale && !!snap && isStaleEnoughToMute(snap.fetchedAt, usage.now))
  const account = $derived(usage.env?.account)
  const session = $derived(usage.env?.session)
  const modelWindow = $derived(usage.primaryModelWindow)
  const compact = $derived(usage.compact)

  const sessionTitle = $derived(
    session?.sourceProject
      ? `${session.sourceProject} 프로젝트의 Claude Code 세션 기준입니다.\n` +
        '세션이 여러 개면 가장 최근에 응답한 쪽이 표시됩니다.\n' +
        '위쪽 사용률 게이지는 웹·앱을 포함한 계정 전체 기준입니다.'
      : 'Claude Code 세션 정보',
  )

  /**
   * "다시 시도"의 반응성. 재시도 결과가 이전과 같은 오류면(429 지속 등)
   * 화면이 그대로라 버튼이 죽은 것처럼 보인다 — 시도 중임을 표시하고,
   * 다음 상태 이벤트가 오면 (내용이 같아도) 표시를 끝낸다.
   */
  let retrying = $state(false)
  let retryNote = $state<string | null>(null)

  $effect(() => {
    void appState // 다음 상태 이벤트 = 재시도 한 사이클이 끝났다는 뜻
    retrying = false
  })

  function retry() {
    if (retrying) return
    retrying = true
    retryNote = null
    refreshNow().catch((e) => {
      retryNote = String(e) // "N초 후에 다시 시도해 주세요" (스로틀) 등
      retrying = false
    })
    // 폴러가 응답을 못 주는 극단적 경우에도 버튼이 영영 잠기지 않게
    setTimeout(() => (retrying = false), 8000)
  }
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

  {#if appState.kind === 'loading'}
    <p class="msg">불러오는 중…</p>
  {:else if appState.kind === 'needsReauth'}
    <p class="msg">Claude Code를 한 번 실행해<br />인증을 갱신해 주세요</p>
  {:else if appState.kind === 'unavailable'}
    <!--
      이유를 함께 보여준다. 처음 설치한 사람에게 "가져올 수 없습니다" 만
      띄우면 Claude Code 로그인이 필요한 건지 네트워크 문제인지 알 수 없다.
    -->
    <p class="msg">{appState.reason || '사용량을 가져올 수 없습니다'}</p>
    {#if appState.reason.includes('429')}
      <p class="hint">요청이 잦아 서버가 잠시 제한을 걸었습니다.<br />제한이 풀리면 자동으로 복구됩니다</p>
    {/if}
    <button onclick={retry} disabled={retrying}>{retrying ? '확인 중…' : '다시 시도'}</button>
    {#if retryNote}
      <p class="hint">{retryNote}</p>
    {/if}
  {:else if snap}
    <!-- 컴팩트 모드에서는 게이지 2개만 남기고 리셋 안내·모델별 한도·푸터를 접는다 -->
    <Gauge
      label="세션 (5시간)"
      utilization={snap.session?.utilization}
      resetsAt={compact ? null : snap.session?.resetsAt}
      now={usage.now}
      muted={muted}
    />
    <Gauge
      label="주간 (7일)"
      utilization={snap.weekly?.utilization}
      resetsAt={compact ? null : snap.weekly?.resetsAt}
      now={usage.now}
      muted={muted}
    />
    {#if modelWindow && !compact}
      <Gauge
        label={`주간 (${modelWindow.displayName})`}
        utilization={modelWindow.utilization}
        resetsAt={modelWindow.resetsAt}
        now={usage.now}
        muted={muted}
        active={modelWindow.isActive}
      />
    {/if}

    {#if !compact}
      <Chart colors={usage.settings.colors} revision={snap.fetchedAt} />

      <!--
        모델·effort·thinking 은 계정 전체가 아니라 "가장 최근 활동한 Claude Code
        세션" 하나의 값이다. 사용률 게이지(계정 전체)와 범위가 다르므로
        출처 프로젝트를 툴팁으로 알려준다.
      -->
      <footer>
        <span class="session-info" title={sessionTitle}>
          {#if session?.modelLabel}{session.modelLabel}{/if}
          {#if session?.effort}<span class="sep">·</span>{session.effort}{/if}
          {#if session?.thinking}<span class="sep">·</span>thinking{/if}
          {#if session?.sourceProject}<span class="src">@{ellipsize(session.sourceProject)}</span>{/if}
        </span>
        <!-- 뱃지는 별도 줄 — 같은 줄에 두면 출처 라벨을 밀어내 @1… 처럼 뭉개진다 -->
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
    font-size: 0.78rem;
    color: var(--text-dim);
  }

  .who {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .plan {
    margin-left: 0.35rem;
    padding: 0.08rem 0.35rem;
    border-radius: 999px;
    background: var(--track);
    color: var(--text);
    /* 가장 작아서 안 보인다는 지적이 있던 곳. 다른 항목보다 더 올렸다 */
    font-size: 0.72rem;
    font-weight: 600;
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
    width: 1.2rem;
    height: 1.2rem;
    padding: 0;
    font: inherit;
    font-size: 0.8rem;
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
    /* 세로 배치: 1줄 = 모델·effort·thinking·출처, 2줄(스테일 시) = "N분 전 기준" 뱃지.
       뱃지를 같은 줄에 두면 출처 라벨이 밀려 @1… 처럼 잘린다 */
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.2rem;
    padding-top: 0.2rem;
    border-top: 1px solid var(--border);
    /* 모델·effort·thinking 줄. 여기도 안 보인다는 지적이 있어 더 올렸다 */
    font-size: 0.72rem;
    color: var(--text-dim);
  }

  .session-info {
    max-width: 100%; /* column flex 에서 줄 폭을 넘으면 ellipsis 가 받는다 */
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sep {
    margin: 0 0.25rem;
    opacity: 0.5;
  }

  .src {
    margin-left: 0.3rem;
    opacity: 0.6;
  }

  .msg {
    margin: 0;
    font-size: 0.8rem;
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
    padding: 0.25rem 0.7rem;
    font: inherit;
    font-size: 0.78rem;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
  }
  button:not(.ctl):disabled {
    opacity: 0.55;
    cursor: default;
  }

  .hint {
    margin: 0;
    font-size: 0.72rem;
    line-height: 1.4;
    color: var(--text-dim);
    text-align: center;
    opacity: 0.85;
  }
</style>
