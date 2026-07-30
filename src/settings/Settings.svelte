<script lang="ts">
  import { onMount } from 'svelte'
  import { disable as disableAutostart, enable as enableAutostart } from '@tauri-apps/plugin-autostart'
  import { getAppInfo, getSettings, resetWidgetSize, updateSettings } from '../lib/ipc'
  import ReportSection from './ReportSection.svelte'
  import WeekdayChart from './WeekdayChart.svelte'
  import { PRESETS, resolveTheme, withAlpha } from '../lib/state.svelte'
  import type { AppInfo, Colors, Settings, Theme } from '../lib/types'

  let settings = $state<Settings | null>(null)
  let info = $state<AppInfo | null>(null)
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
    { key: 'chartSession', label: '차트 · 세션', hint: '24시간 추이의 세션 선' },
    { key: 'chartWeekly', label: '차트 · 주간', hint: '24시간 추이의 주간 선' },
  ]

  onMount(async () => {
    try {
      settings = await getSettings()
      applyPreview()
      // 정보 섹션은 실패해도 설정 자체를 막지 않는다
      info = await getAppInfo().catch(() => null)
    } catch (e) {
      error = String(e)
    }
  })

  /** 설정 창 자신도 같은 색을 쓰므로 즉시 반영해 미리보기가 된다 */
  function applyPreview() {
    if (!settings) return
    const { colors, opacity, theme } = settings
    const root = document.documentElement
    root.dataset.theme = resolveTheme(theme)
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

  /**
   * 테마 변경 = 색 묶음 교체.
   *
   * 색은 전부 설정값이 CSS 변수를 덮어쓰므로 `data-theme` 만 바꿔서는 아무 일도
   * 일어나지 않는다. 그래서 테마와 색을 한 번에 저장한다.
   */
  function applyThemePreset(theme: Theme) {
    save({ theme, colors: PRESETS[resolveTheme(theme)] })
  }

  /** "80, 95" → [80, 95]. 범위 밖·숫자 아닌 값은 Rust 쪽에서도 한 번 더 걸러진다. */
  function parseThresholds(text: string): number[] {
    return text
      .split(',')
      .map((s) => Number(s.trim()))
      .filter((n) => Number.isFinite(n) && n >= 0 && n <= 100)
      .sort((a, b) => a - b)
  }

  /** 위젯 크기를 기본값으로. 드래그로는 정확히 못 돌아가서 버튼을 둔다. */
  async function onResetSize() {
    saving = true
    error = null
    try {
      await resetWidgetSize()
      settings = await getSettings()
    } catch (e) {
      error = String(e)
    } finally {
      saving = false
    }
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
      <h2>테마</h2>
      <div class="row">
        <span class="text">
          <span class="label">색 테마</span>
          <span class="hint">바꾸면 아래 색상이 해당 테마 기본값으로 초기화됩니다</span>
        </span>
        <select value={settings.theme} onchange={(e) => applyThemePreset(e.currentTarget.value as Theme)}>
          <option value="system">시스템</option>
          <option value="light">라이트</option>
          <option value="dark">다크</option>
        </select>
      </div>
    </section>

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

      <div class="row">
        <span class="text">
          <span class="label">위젯 크기</span>
          <span class="hint">가장자리 드래그로 조절 · 너비를 늘리면 전체가 확대됩니다</span>
        </span>
        <button class="action" onclick={onResetSize} disabled={saving}>기본 크기로</button>
      </div>

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

    <ReportSection />

    <WeekdayChart days={30} />

    <p class="dim">{saving ? '저장 중…' : '변경은 즉시 저장·적용됩니다'}</p>

    {#if info}
      <section class="about">
        <h2>정보</h2>
        <p class="line"><strong>{info.name}</strong> v{info.version}</p>
        <p class="line dim">제작 {info.authors} · {info.license} 라이선스</p>
        <p class="line dim">문의는 저장소 이슈로 남겨 주세요</p>
        <p class="line dim">{info.repository}</p>
        <p class="line warn">
          사용량 조회는 Anthropic이 공개하지 않은 엔드포인트를 사용합니다.
          예고 없이 동작하지 않을 수 있습니다.
        </p>
        <p class="line dim">
          사용량 데이터는 외부로 전송되지 않습니다. 자격증명은 읽기만 합니다.
        </p>
      </section>
    {/if}
  {/if}
</main>

<style>
  main {
    padding: 1.1rem 1.25rem 1.5rem;
    color: var(--text);
    /* 위젯과 함께 2026-07-29 에 전체적으로 1pt 올렸다 */
    font-size: 0.93rem;
  }

  h1 {
    margin: 0 0 1rem;
    font-size: 1.15rem;
    font-weight: 600;
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 0.83rem;
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
    font-size: 0.88rem;
  }

  .hint {
    font-size: 0.76rem;
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

  select {
    flex: none;
    padding: 0.2rem 0.4rem;
    font: inherit;
    font-size: 0.8rem;
    color: var(--text);
    background: var(--track);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
  }

  .action {
    flex: none;
    padding: 0.25rem 0.6rem;
    font: inherit;
    font-size: 0.78rem;
    color: var(--text);
    background: var(--track);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
  }
  .action:hover {
    background: var(--bg-solid);
  }
  .action:disabled {
    opacity: 0.55;
    cursor: default;
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
    min-width: 4.6rem;
    font-size: 0.76rem;
    color: var(--text-dim);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .dim {
    margin: 0.35rem 0 0;
    font-size: 0.8rem;
    color: var(--text-dim);
  }

  .about {
    margin-top: 1.6rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
    /* 앱 전체는 드래그 방지를 위해 user-select:none 이지만,
       여기 이메일·저장소 주소는 복사할 수 있어야 한다 */
    -webkit-user-select: text;
    user-select: text;
  }

  .line {
    margin: 0 0 0.25rem;
    font-size: 0.78rem;
    line-height: 1.45;
    word-break: break-all;
  }

  .warn {
    margin-top: 0.6rem;
    padding: 0.45rem 0.6rem;
    border-radius: 6px;
    background: var(--track);
    color: var(--c-warning);
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
