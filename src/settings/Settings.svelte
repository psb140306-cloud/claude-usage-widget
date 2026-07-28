<script lang="ts">
  import { onMount } from 'svelte'
  import { disable as disableAutostart, enable as enableAutostart } from '@tauri-apps/plugin-autostart'
  import { getSettings, updateSettings } from '../lib/ipc'
  import { withAlpha } from '../lib/state.svelte'
  import type { Colors, Settings } from '../lib/types'

  let settings = $state<Settings | null>(null)
  let error = $state<string | null>(null)
  let saving = $state(false)

  /** 색상 항목 정의 — 라벨과 설명을 한곳에 모아둔다 */
  const COLOR_FIELDS: { key: keyof Colors; label: string; hint: string }[] = [
    { key: 'text', label: '글자', hint: '수치·라벨 기본 색' },
    { key: 'textDim', label: '보조 글자', hint: '리셋 안내, 계정 줄' },
    { key: 'gaugeNormal', label: '게이지 · 정상', hint: '60% 미만' },
    { key: 'gaugeWarning', label: '게이지 · 주의', hint: '60 ~ 85%' },
    { key: 'gaugeDanger', label: '게이지 · 위험', hint: '85% 초과' },
    { key: 'gaugeTrack', label: '게이지 배경', hint: '막대의 빈 부분' },
    { key: 'background', label: '위젯 배경', hint: '아래 불투명도와 함께 적용' },
  ]

  onMount(async () => {
    try {
      settings = await getSettings()
      applyPreview()
    } catch (e) {
      error = String(e)
    }
  })

  /** 설정 창 자신도 같은 색을 쓰므로 즉시 반영해 미리보기가 된다 */
  function applyPreview() {
    if (!settings) return
    const { colors, opacity } = settings
    const root = document.documentElement
    root.style.setProperty('--text', colors.text)
    root.style.setProperty('--text-dim', colors.textDim)
    root.style.setProperty('--c-normal', colors.gaugeNormal)
    root.style.setProperty('--c-warning', colors.gaugeWarning)
    root.style.setProperty('--c-danger', colors.gaugeDanger)
    root.style.setProperty('--track', colors.gaugeTrack)
    root.style.setProperty('--bg', withAlpha(colors.background, opacity))
    root.style.setProperty('--bg-solid', colors.background)
  }

  /**
   * 위젯에도 반영하려면 저장까지 가야 한다 (Rust 가 이벤트를 뿌린다).
   *
   * 색을 빠르게 여러 번 바꾸면 저장 요청이 겹친다. 이전 요청을 기다렸다 보내고,
   * 응답도 마지막 요청의 것만 반영한다. 그러지 않으면 늦게 도착한 옛 응답이
   * 방금 고른 색을 덮어써 되돌아간 것처럼 보인다.
   */
  let pending: Promise<unknown> = Promise.resolve()
  let latestRequest = 0

  function save(patch: Record<string, unknown>) {
    const id = ++latestRequest
    saving = true
    error = null

    pending = pending
      .catch(() => {}) // 앞 요청의 실패가 뒤 요청을 막지 않게
      .then(() => updateSettings(patch))
      .then((next) => {
        if (id !== latestRequest) return // 뒤에 더 새로운 요청이 있다
        settings = next
        applyPreview()
      })
      .catch((e) => {
        if (id === latestRequest) error = String(e)
      })
      .finally(() => {
        if (id === latestRequest) saving = false
      })
  }

  function onColorInput(key: keyof Colors, value: string) {
    if (!settings) return
    settings.colors[key] = value // 즉시 미리보기
    applyPreview()
    save({ colors: { [key]: value } })
  }

  /** "80, 95" → [80, 95]. 범위 밖·숫자 아닌 값은 Rust 쪽에서도 한 번 더 걸러진다. */
  function parseThresholds(text: string): number[] {
    return text
      .split(',')
      .map((s) => Number(s.trim()))
      .filter((n) => Number.isFinite(n) && n >= 0 && n <= 100)
      .sort((a, b) => a - b)
  }

  /**
   * 자동 시작은 설정 파일만 바꿔선 안 되고 OS 등록도 해야 한다.
   * 플러그인 호출이 실패하면 설정도 바꾸지 않는다 — 실제 상태와 표시가 어긋나면 안 된다.
   */
  async function setAutoStart(enabled: boolean) {
    saving = true
    error = null
    try {
      if (enabled) await enableAutostart()
      else await disableAutostart()
      settings = await updateSettings({ autoStart: enabled })
    } catch (e) {
      error = `자동 시작 설정 실패: ${e}`
      settings = await getSettings().catch(() => settings)
    } finally {
      saving = false
    }
  }
</script>

<main>
  <h1>설정</h1>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if !settings}
    <p class="dim">불러오는 중…</p>
  {:else}
    <section>
      <h2>색상</h2>
      <div class="fields">
        {#each COLOR_FIELDS as f (f.key)}
          <label class="row">
            <input
              type="color"
              value={settings.colors[f.key]}
              oninput={(e) => onColorInput(f.key, e.currentTarget.value)}
            />
            <span class="text">
              <span class="label">{f.label}</span>
              <span class="hint">{f.hint}</span>
            </span>
            <code>{settings.colors[f.key]}</code>
          </label>
        {/each}
      </div>
    </section>

    <section>
      <h2>위젯</h2>
      <label class="row slider">
        <span class="text">
          <span class="label">배경 불투명도</span>
          <span class="hint">0.3 ~ 1.0</span>
        </span>
        <input
          type="range"
          min="0.3"
          max="1"
          step="0.05"
          value={settings.opacity}
          oninput={(e) => {
            if (settings) settings.opacity = Number(e.currentTarget.value)
            applyPreview()
          }}
          onchange={(e) => save({ opacity: Number(e.currentTarget.value) })}
        />
        <code>{settings.opacity.toFixed(2)}</code>
      </label>

      <label class="row slider">
        <span class="text">
          <span class="label">폴링 주기</span>
          <span class="hint">30초 ~ 10분</span>
        </span>
        <input
          type="range"
          min="30"
          max="600"
          step="10"
          value={settings.pollingIntervalSec}
          oninput={(e) => {
            if (settings) settings.pollingIntervalSec = Number(e.currentTarget.value)
          }}
          onchange={(e) => save({ pollingIntervalSec: Number(e.currentTarget.value) })}
        />
        <code>{settings.pollingIntervalSec}초</code>
      </label>
    </section>

    <section>
      <h2>알림</h2>
      <label class="row">
        <input
          type="checkbox"
          checked={settings.notificationsEnabled}
          onchange={(e) => save({ notificationsEnabled: e.currentTarget.checked })}
        />
        <span class="text">
          <span class="label">임계값 알림</span>
          <span class="hint">한도에 도달하면 Windows 토스트</span>
        </span>
      </label>

      <label class="row">
        <input
          type="checkbox"
          checked={settings.notifyOnReset}
          onchange={(e) => save({ notifyOnReset: e.currentTarget.checked })}
        />
        <span class="text">
          <span class="label">리셋 완료 알림</span>
          <span class="hint">사용량이 초기화되면 알림</span>
        </span>
      </label>

      <div class="row" class:disabled={!settings.notificationsEnabled}>
        <span class="text">
          <span class="label">임계값</span>
          <span class="hint">쉼표로 구분 (0~100)</span>
        </span>
        <input
          class="thresholds"
          type="text"
          value={settings.thresholds.join(', ')}
          disabled={!settings.notificationsEnabled}
          onchange={(e) => save({ thresholds: parseThresholds(e.currentTarget.value) })}
        />
      </div>
    </section>

    <section>
      <h2>시작</h2>
      <label class="row">
        <input
          type="checkbox"
          checked={settings.autoStart}
          onchange={(e) => setAutoStart(e.currentTarget.checked)}
        />
        <span class="text">
          <span class="label">Windows 시작 시 자동 실행</span>
          <span class="hint">로그인하면 트레이에 상주</span>
        </span>
      </label>
    </section>

    <p class="dim">{saving ? '저장 중…' : '변경은 즉시 저장·적용됩니다'}</p>
  {/if}
</main>

<style>
  main {
    padding: 1.1rem 1.25rem 1.5rem;
    color: var(--text);
    font-size: 0.85rem;
  }

  h1 {
    margin: 0 0 1rem;
    font-size: 1.05rem;
    font-weight: 600;
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  section {
    margin-bottom: 1.4rem;
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.3rem 0.2rem;
    border-radius: 6px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--track);
  }

  .row .text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .label {
    font-size: 0.8rem;
  }

  .hint {
    font-size: 0.68rem;
    color: var(--text-dim);
  }

  input[type='color'] {
    flex: none;
    width: 2rem;
    height: 1.5rem;
    padding: 0;
    background: none;
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
  }

  .slider input[type='range'] {
    flex: none;
    width: 7rem;
  }

  input[type='checkbox'] {
    flex: none;
    width: 1rem;
    height: 1rem;
    accent-color: var(--c-normal);
    cursor: pointer;
  }

  .thresholds {
    flex: none;
    width: 6rem;
    padding: 0.2rem 0.4rem;
    font: inherit;
    font-size: 0.75rem;
    color: var(--text);
    background: var(--track);
    border: 1px solid var(--border);
    border-radius: 4px;
    text-align: right;
  }

  .disabled {
    opacity: 0.45;
  }

  code {
    flex: none;
    min-width: 4.2rem;
    font-size: 0.68rem;
    color: var(--text-dim);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .dim {
    margin: 0.35rem 0 0;
    font-size: 0.72rem;
    color: var(--text-dim);
  }

  .error {
    margin: 0 0 0.75rem;
    padding: 0.45rem 0.6rem;
    font-size: 0.75rem;
    color: var(--c-danger);
    background: var(--track);
    border-radius: 6px;
  }
</style>
