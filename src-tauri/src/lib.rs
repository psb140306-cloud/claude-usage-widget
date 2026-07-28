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
use model::AppState;
use poller::Poller;
use settings::{Settings, SettingsStore, WidgetMode, EVENT_SETTINGS};

/// 위젯 창 크기 (논리 픽셀). 컴팩트는 게이지 2개만 남기고 접는다.
const WIDGET_SIZE_EXPANDED: (f64, f64) = (240.0, 215.0);
const WIDGET_SIZE_COMPACT: (f64, f64) = (240.0, 96.0);

/// 창 라벨. `tauri.conf.json` 및 `capabilities/default.json` 과 일치해야 한다.
const WIDGET_WINDOW: &str = "widget";
const SETTINGS_WINDOW: &str = "settings";

/// 앱 전역 상태.
struct AppCtx {
    poller: Arc<Poller>,
    settings: SettingsStore,
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

#[tauri::command]
fn get_settings(ctx: tauri::State<'_, AppCtx>) -> Settings {
    ctx.settings.get()
}

/// 부분 갱신 → 저장 → 즉시 적용. 보낸 키만 바뀐다.
#[tauri::command]
fn update_settings(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, AppCtx>,
    patch: serde_json::Value,
) -> Result<Settings, String> {
    let next = ctx.settings.update(patch).map_err(|e| e.to_string())?;

    // 폴링 주기는 저장만 해서는 반영되지 않는다 — 돌고 있는 루프에 알려야 한다
    ctx.poller.set_interval_secs(next.polling_interval_sec);

    // 열려 있는 창들이 색상을 바로 다시 칠하도록
    if let Err(e) = app.emit(EVENT_SETTINGS, &next) {
        eprintln!("설정 이벤트 발행 실패: {e}");
    }

    Ok(next)
}

#[tauri::command]
fn query_history(_from: String, _to: String) -> Result<Vec<model::HistoryEntry>, String> {
    // TODO(M5 5.1)
    Err("M5에서 구현 예정".into())
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

    ctx.settings
        .update(serde_json::json!({ "widgetMode": target }))
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
#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(SETTINGS_WINDOW) {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // 창 생성은 메인 스레드에서 해야 한다. 커맨드는 워커 스레드에서 실행될 수 있고,
    // 그 상태로 build() 를 부르면 Windows 에서 창은 떠도 웹뷰가 백지가 된다.
    let handle = app.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    app.run_on_main_thread(move || {
        let result = tauri::WebviewWindowBuilder::new(
            &handle,
            SETTINGS_WINDOW,
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("Claude Usage Widget 설정")
        .inner_size(420.0, 560.0)
        .min_inner_size(360.0, 400.0)
        .resizable(true)
        .center()
        .build();

        // 실패를 조용히 삼키지 않는다 — 버튼이 먹통인 것처럼 보이는 원인이 된다
        let _ = tx.send(result.err().map(|e| e.to_string()));
    })
    .map_err(|e| format!("설정 창을 열 수 없습니다: {e}"))?;

    // 생성 결과를 호출자에게 되돌려 준다. 그러지 않으면 창이 안 떠도
    // 프론트의 promise 는 성공으로 resolve 되어 catch 가 걸리지 않는다.
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(None) => Ok(()),
        Ok(Some(e)) => Err(format!("설정 창 생성 실패: {e}")),
        Err(_) => Err("설정 창 생성이 응답하지 않습니다".into()),
    }
}

// ─────────────────────────── 창 배치 ───────────────────────────

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
            if let Err(e) = open_settings_window(app.clone()) {
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
            refresh_now,
            get_settings,
            update_settings,
            query_history,
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

            app.manage(AppCtx {
                poller: poller.clone(),
                settings,
            });

            tray::build(app.handle(), on_tray_menu)?;

            // 상태가 바뀌면 트레이 아이콘 색과 툴팁을 함께 갱신한다 (FR-5).
            poller.set_observer(|app, state| {
                let colors = app.state::<AppCtx>().settings.get().colors;
                tray::update(app, state, &colors);
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
                if let Err(e) = place_bottom_right(&widget) {
                    // 배치 실패가 창을 못 띄우는 사유가 되면 안 된다
                    eprintln!("위젯 기본 위치 설정 실패(기본 배치로 진행): {e}");
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
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
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
