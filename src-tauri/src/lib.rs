//! 앱 조립부. 모듈 경계는 PRD 5.3 을 그대로 따른다.
//!
//! credentials → usage_client → poller → (history / notifier / UI)

mod credentials;
mod environment;
mod history;
mod model;
mod notifier;
mod poller;
mod settings;
mod tray;
mod usage_client;

use std::sync::Arc;

use tauri::{Emitter, LogicalSize, Manager, WindowEvent};

use environment::Environment;
use history::History;
use model::AppState;
use notifier::{Alert, Notifier};
use poller::Poller;
use settings::{Settings, SettingsStore, WidgetMode, EVENT_SETTINGS};

/// 위젯 창 크기 (논리 픽셀). 컴팩트는 게이지 2개만 남기고 접는다.
///
/// 2026-07-29 글자 크기를 전체적으로 1pt 올리면서 세로도 함께 늘렸다.
/// 폭도 조금 넓혔다 — "주간 (Fable)" 처럼 라벨이 긴 게이지에서 % 와 붙는다.
const WIDGET_SIZE_EXPANDED: (f64, f64) = (260.0, 372.0);
const WIDGET_SIZE_COMPACT: (f64, f64) = (260.0, 122.0);

/// 창 라벨. `tauri.conf.json` 및 `capabilities/default.json` 과 일치해야 한다.
const WIDGET_WINDOW: &str = "widget";
const SETTINGS_WINDOW: &str = "settings";

/// 앱 전역 상태.
struct AppCtx {
    poller: Arc<Poller>,
    settings: SettingsStore,
    notifier: std::sync::Mutex<Notifier>,
    /// 히스토리는 열지 못해도 앱은 떠야 하므로 Option 이다 (FR-7 은 권장 기능)
    history: std::sync::Mutex<Option<History>>,
    /// 위젯 위치를 마지막으로 저장한 시각 — 드래그 중 파일을 매번 쓰지 않기 위함
    position_saved_at: std::sync::Mutex<Option<std::time::Instant>>,
}

/// 위치를 다시 저장하기까지 두는 최소 간격.
/// 드래그 한 번에 `Moved` 가 수십 번 오므로 그때마다 쓰면 디스크를 두드린다.
const POSITION_SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

impl AppCtx {
    /// 위젯이 움직였을 때 호출. 간격이 지났을 때만 실제로 저장한다.
    fn remember_position(&self, x: i32, y: i32) {
        {
            let mut last = self.position_saved_at.lock().unwrap();
            let now = std::time::Instant::now();
            if matches!(*last, Some(t) if now.duration_since(t) < POSITION_SAVE_INTERVAL) {
                return;
            }
            *last = Some(now);
        }

        // 위치는 조용히 저장한다 — 이벤트를 쏘면 창이 자기 위치 변경에 반응하게 된다
        if let Err(e) = self
            .settings
            .update(serde_json::json!({ "widgetPosition": { "x": x, "y": y } }))
        {
            eprintln!("위젯 위치 저장 실패: {e}");
        }
    }
}

/// FR-6: 평가된 알림을 Windows 토스트로 내보낸다.
///
/// 폴링 태스크에서 직접 부르지 않고 블로킹 풀로 넘긴다. Windows 토스트는
/// WinRT 호출이라 상황에 따라 붙잡힐 수 있는데, 그 사이 창 생성·메시지 처리가
/// 밀리면 설정 창이 백지로 뜨거나 닫히지 않는다.
fn send_alerts(app: &tauri::AppHandle, alerts: &[Alert]) {
    use tauri_plugin_notification::NotificationExt;

    let app = app.clone();
    let alerts = alerts.to_vec();

    tauri::async_runtime::spawn_blocking(move || {
        for alert in alerts {
            if let Err(e) = app
                .notification()
                .builder()
                .title(alert.title())
                .body(alert.body())
                .show()
            {
                eprintln!("알림 발송 실패: {e}");
            }
        }
    });
}

// ─────────────────────────── 커맨드 (프론트 `src/lib/ipc.ts` 와 1:1) ───────────────────────────

#[tauri::command]
fn get_state(ctx: tauri::State<'_, AppCtx>) -> AppState {
    ctx.poller.state()
}

/// 수동 새로고침. 스로틀(5초)에 걸리면 남은 시간을 안내한다.
#[tauri::command]
fn refresh_now(ctx: tauri::State<'_, AppCtx>) -> Result<(), String> {
    ctx.poller.request_refresh()
}

/// 계정·플랜·현재 세션(모델/effort/thinking).
#[tauri::command]
fn get_environment(ctx: tauri::State<'_, AppCtx>) -> Environment {
    ctx.poller.environment()
}

/// 설정 창의 "정보" 섹션용. 값은 Cargo.toml 에서 온다 — 표시용으로 따로
/// 적어 두면 버전을 올릴 때 어긋난다.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    authors: &'static str,
    license: &'static str,
    repository: &'static str,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Claude Usage Widget",
        version: env!("CARGO_PKG_VERSION"),
        authors: env!("CARGO_PKG_AUTHORS"),
        license: env!("CARGO_PKG_LICENSE"),
        repository: env!("CARGO_PKG_REPOSITORY"),
    }
}

#[tauri::command]
fn get_settings(ctx: tauri::State<'_, AppCtx>) -> Settings {
    ctx.settings.get()
}

/// 설정을 저장하고 **즉시 적용**한다.
///
/// 저장만 하면 아무 일도 일어나지 않는다. 돌고 있는 폴링 루프와 열려 있는 창들에
/// 각각 알려야 한다. 설정을 바꾸는 경로가 여럿이므로(`update_settings`,
/// `set_widget_mode`) 한 곳에 모아 두지 않으면 한쪽만 반영되는 사고가 난다.
/// (실제로 `set_widget_mode` 가 이벤트를 안 보내 창만 줄고 UI 는 그대로였다)
fn apply_settings(
    app: &tauri::AppHandle,
    ctx: &AppCtx,
    patch: serde_json::Value,
) -> Result<Settings, String> {
    let next = ctx.settings.update(patch).map_err(|e| e.to_string())?;

    ctx.poller.set_interval_secs(next.polling_interval_sec);

    if let Err(e) = app.emit(EVENT_SETTINGS, &next) {
        eprintln!("설정 이벤트 발행 실패: {e}");
    }

    Ok(next)
}

/// 부분 갱신 → 저장 → 즉시 적용. 보낸 키만 바뀐다.
#[tauri::command]
fn update_settings(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, AppCtx>,
    patch: serde_json::Value,
) -> Result<Settings, String> {
    apply_settings(&app, &ctx, patch)
}

/// 기간 히스토리 조회. from/to 는 ISO 8601.
#[tauri::command]
fn query_history(
    ctx: tauri::State<'_, AppCtx>,
    from: String,
    to: String,
) -> Result<Vec<model::HistoryEntry>, String> {
    let parse = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&chrono::Utc))
            .map_err(|e| format!("시각 형식이 올바르지 않습니다: {e}"))
    };
    let (from, to) = (parse(&from)?, parse(&to)?);

    let guard = ctx.history.lock().unwrap();
    let Some(history) = guard.as_ref() else {
        return Err("히스토리 저장소를 열 수 없었습니다".into());
    };
    history.query(from, to).map_err(|e| e.to_string())
}

/// 요일별 사용 패턴 (FR-7). `days` 일치를 대상으로 집계한다.
#[tauri::command]
fn query_weekday_stats(
    ctx: tauri::State<'_, AppCtx>,
    days: i64,
) -> Result<Vec<history::WeekdayStat>, String> {
    let since = chrono::Utc::now() - chrono::Duration::days(days.clamp(1, history::RETENTION_DAYS));

    let guard = ctx.history.lock().unwrap();
    let Some(history) = guard.as_ref() else {
        return Err("히스토리 저장소를 열 수 없었습니다".into());
    };
    history.weekday_stats(since).map_err(|e| e.to_string())
}

/// 기간 리포트용 하루 요약. `days` 는 오늘 포함 조회 일수.
///
/// 하루 요약은 원본(90일)과 달리 지우지 않으므로 1년 리포트가 가능하다.
/// 상한 366은 그 리포트 최장 기간이다.
#[tauri::command]
fn query_daily_report(
    ctx: tauri::State<'_, AppCtx>,
    days: u32,
) -> Result<Vec<history::DailyStat>, String> {
    let days = days.clamp(1, 366);
    let since = (chrono::Local::now().date_naive() - chrono::Duration::days(days as i64 - 1))
        .to_string();

    let guard = ctx.history.lock().unwrap();
    let Some(history) = guard.as_ref() else {
        return Err("히스토리 저장소를 열 수 없었습니다".into());
    };
    history.daily_report(&since).map_err(|e| e.to_string())
}

/// 컴팩트 ↔ 확장 전환. 창 크기를 바꾸고 설정에 남긴다.
#[tauri::command]
fn set_widget_mode(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, AppCtx>,
    mode: String,
) -> Result<(), String> {
    let (target, size) = match mode.as_str() {
        "compact" => (WidgetMode::Compact, WIDGET_SIZE_COMPACT),
        "expanded" => (WidgetMode::Expanded, WIDGET_SIZE_EXPANDED),
        other => return Err(format!("알 수 없는 모드: {other}")),
    };

    let win = app
        .get_webview_window(WIDGET_WINDOW)
        .ok_or("위젯 창을 찾을 수 없습니다")?;

    // 창 크기보다 **UI 전환을 먼저** 알린다. 순서가 반대면 확장 레이아웃이
    // 잠깐 좁은 창에 잘려 보인다.
    apply_settings(&app, &ctx, serde_json::json!({ "widgetMode": target }))?;

    // 우측 하단에 붙여 쓰는 위젯이므로, 크기가 바뀌어도 그 모서리는 그대로 둔다.
    // 좌상단을 고정하면 접을 때 화면 가운데로 떠오른 것처럼 보인다.
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let before = win.outer_size().map_err(|e| e.to_string())?;
    let pos = win.outer_position().map_err(|e| e.to_string())?;

    win.set_size(LogicalSize::new(size.0, size.1))
        .map_err(|e| e.to_string())?;

    let after_h = (size.1 * scale).round() as i32;
    let y = anchor_bottom(pos.y, before.height, after_h);
    win.set_position(tauri::PhysicalPosition::new(pos.x, y))
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 위젯을 트레이로 숨긴다. 앱은 계속 상주한다 (FR-5).
#[tauri::command]
fn hide_widget(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview_window(WIDGET_WINDOW)
        .ok_or("위젯 창을 찾을 수 없습니다")?
        .hide()
        .map_err(|e| e.to_string())
}

/// 설정 창은 **지연 생성**한다.
///
/// 시작 시 미리 만들어 두면(숨겨 두더라도) WebView2 렌더러가 하나 더 붙는다.
/// 스캐폴딩 스모크 테스트 실측(debug 빌드, 프로세스 트리 WorkingSet 합):
/// 미리 생성 404.5MB → 지연 생성 351.7MB.
///
/// ⚠️ WorkingSet 은 WebView2 프로세스 간 공유 페이지를 중복 계산하므로
/// PRD 의 150MB 목표와 직접 비교할 수 없다. 실제 검증은 M6 에서
/// release 빌드 + private bytes 기준으로 한다.
/// 설정 창을 연다. 이미 있으면 앞으로 가져온다.
///
/// **메인 스레드를 막지 않는 것이 핵심이다.** 이 자리에서 세 번 헤맸다:
///
/// 1. 워커 스레드에서 `build()` 를 직접 호출 → 창은 떠도 웹뷰가 백지
/// 2. 존재 확인과 생성이 다른 스레드에 있어 창이 두 개 겹쳐 뜸
/// 3. 결과를 채널로 받으려고 `recv_timeout` 으로 대기 → **동기 커맨드는 메인
///    스레드에서 실행되므로 메인을 막아버린다.** 그러면 이벤트 루프가 큐에 넣은
///    창 생성을 처리하지 못하고 새 웹뷰가 초기화를 못 끝내 다시 백지가 된다
///
/// 그래서 지금은 확인+생성을 메인 스레드 클로저 안에서 **한 번에** 하고,
/// 결과는 기다리지 않고 로그로 남긴다.
fn spawn_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();

    app.run_on_main_thread(move || {
        if let Some(win) = handle.get_webview_window(SETTINGS_WINDOW) {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
            return;
        }

        let result = tauri::WebviewWindowBuilder::new(
            &handle,
            SETTINGS_WINDOW,
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("Claude Usage Widget 설정")
        .inner_size(420.0, 620.0)
        .min_inner_size(360.0, 400.0)
        .resizable(true)
        .center()
        .build();

        // 실패를 조용히 삼키지 않는다 — 버튼이 먹통인 것처럼 보이는 원인이 된다
        if let Err(e) = result {
            eprintln!("설정 창 생성 실패: {e}");
        }
    })
    .map_err(|e| format!("설정 창을 열 수 없습니다: {e}"))
}

/// `async` 로 두어 커맨드 본문이 **메인 스레드가 아닌 곳**에서 실행되게 한다.
/// 동기 커맨드였다면 아래 `run_on_main_thread` 가 메인에서 메인으로 넘기는 꼴이 된다.
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    spawn_settings_window(&app)
}

// ─────────────────────────── 창 배치 ───────────────────────────

/// 저장된 위치가 지금 화면 배치에서 여전히 쓸 만한지.
///
/// 모니터를 뽑거나 해상도를 바꾸면 예전 좌표가 화면 밖이 된다. 그대로 복원하면
/// 위젯이 보이지 않는 곳에 뜨고 사용자는 사라졌다고 생각한다.
/// 창의 좌상단이 어느 모니터 안에 있으면 쓸 만하다고 본다.
fn position_is_visible(monitors: &[tauri::window::Monitor], x: i32, y: i32) -> bool {
    monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x && y >= p.y && x < p.x + s.width as i32 && y < p.y + s.height as i32
    })
}

/// 크기가 바뀌어도 **아래 모서리**가 제자리에 있도록 새 y 좌표를 구한다.
///
/// 좌상단을 고정하면 접을 때 위젯이 화면 가운데로 떠오른 것처럼 보인다.
/// 우측 하단에 붙여 쓰는 위젯이라 아래를 기준으로 잡는 게 자연스럽다.
fn anchor_bottom(top: i32, old_height: u32, new_height: i32) -> i32 {
    top + old_height as i32 - new_height
}

/// 화면 가장자리로부터의 여백 (논리 픽셀)
const WIDGET_MARGIN: f64 = 16.0;

/// 위젯 기본 위치: **작업 영역 우측 하단**.
///
/// `work_area()` 는 작업 표시줄을 제외한 영역이라 위젯이 표시줄에 가리지 않는다.
/// 좌표를 주지 않으면 OS 기본 배치(좌상단 104,104)로 떨어져 울트라와이드에서는
/// 사실상 눈에 띄지 않는다.
///
/// TODO(M3 3.1): 저장된 위치가 있으면 그 값이 이 기본값을 덮어쓴다.
fn place_bottom_right(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(()); // 모니터 정보를 못 얻으면 OS 기본 배치를 그대로 둔다
    };

    let area = monitor.work_area();
    let size = window.outer_size()?;
    let margin = (WIDGET_MARGIN * monitor.scale_factor()).round() as i32;

    let x = area.position.x + area.size.width as i32 - size.width as i32 - margin;
    let y = area.position.y + area.size.height as i32 - size.height as i32 - margin;

    window.set_position(tauri::PhysicalPosition::new(x, y))
}

// ─────────────────────────── 트레이 메뉴 처리 (FR-5) ───────────────────────────

fn on_tray_menu(app: &tauri::AppHandle, id: &str) {
    match id {
        tray::ITEM_TOGGLE => {
            if let Some(w) = app.get_webview_window(WIDGET_WINDOW) {
                let visible = w.is_visible().unwrap_or(false);
                let _ = if visible { w.hide() } else { w.show() };
            }
        }
        tray::ITEM_REFRESH => {
            if let Some(ctx) = app.try_state::<AppCtx>() {
                // 스로틀에 걸리면 조용히 무시한다 (트레이 메뉴엔 알릴 곳이 없다)
                let _ = ctx.poller.request_refresh();
            }
        }
        tray::ITEM_SETTINGS => {
            if let Err(e) = spawn_settings_window(app) {
                eprintln!("{e}");
            }
        }
        // 창을 닫아도 앱은 살아 있고, 완전 종료는 여기서만 (FR-5)
        tray::ITEM_QUIT => app.exit(0),
        _ => {}
    }
}

// ─────────────────────────── 엔트리 ───────────────────────────

pub fn run() {
    let mut builder = tauri::Builder::default();

    // 단일 인스턴스가 먼저 등록되어야 한다. 두 번째 실행은 기존 위젯을 띄우고 종료.
    #[cfg(windows)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window(WIDGET_WINDOW) {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_environment,
            get_app_info,
            refresh_now,
            get_settings,
            update_settings,
            query_history,
            query_weekday_stats,
            query_daily_report,
            set_widget_mode,
            hide_widget,
            open_settings_window,
        ])
        .setup(|app| {
            // 설정 파일 경로는 AppHandle 이 있어야 정해지므로 여기서 상태를 만든다
            let settings_path = app
                .path()
                .app_config_dir()
                .map_err(|e| format!("설정 경로를 찾을 수 없습니다: {e}"))?
                .join("settings.json");

            let poller = Arc::new(Poller::new());
            let settings = SettingsStore::load(settings_path);
            let saved = settings.get();
            poller.set_interval_secs(saved.polling_interval_sec);

            // 히스토리는 열지 못해도 사용량 표시는 계속되어야 한다 (FR-7 은 권장 기능)
            let history = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())
                .and_then(|dir| {
                    History::open(&dir.join("history.db")).map_err(|e| e.to_string())
                })
                .inspect_err(|e| eprintln!("히스토리 저장소를 열 수 없습니다: {e}"))
                .ok();

            if let Some(h) = &history {
                match h.prune(chrono::Utc::now()) {
                    Ok(n) if n > 0 => eprintln!("히스토리 {n}행 정리(보존 {}일 초과)", history::RETENTION_DAYS),
                    Err(e) => eprintln!("히스토리 정리 실패: {e}"),
                    _ => {}
                }
            }

            app.manage(AppCtx {
                poller: poller.clone(),
                notifier: std::sync::Mutex::new(Notifier::new(
                    saved.thresholds.clone(),
                    saved.notify_on_reset,
                )),
                history: std::sync::Mutex::new(history),
                position_saved_at: std::sync::Mutex::new(None),
                settings,
            });

            tray::build(app.handle(), on_tray_menu)?;

            // 상태가 바뀌면 트레이(FR-5)와 알림(FR-6)을 함께 처리한다.
            poller.set_observer(|app, state| {
                let settings = app.state::<AppCtx>().settings.get();
                tray::update(app, state, &settings.colors);

                // 스테일한 값으로 알림을 울리면 안 된다 — 이미 지난 상태일 수 있다
                let AppState::Ok { snapshot } = state else {
                    return;
                };

                let ctx = app.state::<AppCtx>();

                // FR-7: 성공한 스냅샷만 기록한다
                if let Some(h) = ctx.history.lock().unwrap().as_ref() {
                    if let Err(e) = h.append(snapshot) {
                        eprintln!("히스토리 저장 실패: {e}");
                    }
                }

                if !settings.notifications_enabled {
                    return;
                }
                let alerts = {
                    let mut notifier = ctx.notifier.lock().unwrap();
                    notifier.configure(settings.thresholds.clone(), settings.notify_on_reset);
                    notifier.evaluate(snapshot, chrono::Utc::now())
                };
                if !alerts.is_empty() {
                    send_alerts(app, &alerts);
                }
            });

            // 위젯은 config 에서 visible:false 로 만들어 두고, 배치한 뒤에 보여준다.
            // 그러지 않으면 좌상단에 떴다가 우측 하단으로 튀는 게 보인다.
            if let Some(widget) = app.get_webview_window(WIDGET_WINDOW) {
                // 저장된 모드가 컴팩트면 그 크기로 시작한다
                if saved.widget_mode == WidgetMode::Compact {
                    let _ = widget.set_size(LogicalSize::new(
                        WIDGET_SIZE_COMPACT.0,
                        WIDGET_SIZE_COMPACT.1,
                    ));
                }
                // 저장된 위치가 있고 지금 화면에서 보이는 자리면 복원하고,
                // 아니면 기본 배치(우측 하단)로 간다.
                let restored = saved.widget_position.and_then(|p| {
                    let monitors = widget.available_monitors().ok()?;
                    if !position_is_visible(&monitors, p.x, p.y) {
                        eprintln!("저장된 위젯 위치가 화면 밖이라 기본 배치로 진행합니다");
                        return None;
                    }
                    widget
                        .set_position(tauri::PhysicalPosition::new(p.x, p.y))
                        .ok()
                });

                if restored.is_none() {
                    if let Err(e) = place_bottom_right(&widget) {
                        // 배치 실패가 창을 못 띄우는 사유가 되면 안 된다
                        eprintln!("위젯 기본 위치 설정 실패(기본 배치로 진행): {e}");
                    }
                }
                widget.show()?;
            }

            // 폴링 시작. 첫 조회는 즉시 일어나고, 이후 설정된 주기로 돈다.
            poller.start(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            // FR-5: **위젯** 창을 닫아도 앱은 트레이에 상주한다. 완전 종료는 트레이 메뉴에서만.
            //
            // 설정 창까지 가로채면 X 버튼이 먹통이 된다. 설정 창은 그대로 닫히게 두어야
            // WebView2 렌더러도 함께 반환된다 (지연 생성과 같은 이유).
            if window.label() != WIDGET_WINDOW {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // 드래그로 옮긴 자리를 기억한다. Moved 는 드래그 중 계속 오므로
                // 매번 파일에 쓰지 않고, 마지막 좌표만 들고 있다가 간격을 두고 저장한다.
                WindowEvent::Moved(pos) => {
                    if let Some(ctx) = window.app_handle().try_state::<AppCtx>() {
                        ctx.remember_position(pos.x, pos.y);
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 접었다 펴도 아래 모서리는 그대로여야 한다.
    #[test]
    fn resize_keeps_bottom_edge() {
        // 확장(215) → 컴팩트(96): 아래가 1376 으로 유지되도록 위가 내려간다
        let top = 1161;
        let bottom = top + 215;
        let new_top = anchor_bottom(top, 215, 96);
        assert_eq!(new_top + 96, bottom);
        assert_eq!(new_top, 1280);

        // 다시 펼치면 원래 자리로 돌아온다
        assert_eq!(anchor_bottom(new_top, 96, 215), top);
    }

    #[test]
    fn compact_is_shorter_but_same_width() {
        assert_eq!(WIDGET_SIZE_COMPACT.0, WIDGET_SIZE_EXPANDED.0);
        assert!(WIDGET_SIZE_COMPACT.1 < WIDGET_SIZE_EXPANDED.1);
    }
}
