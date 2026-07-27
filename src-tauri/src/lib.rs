//! 앱 조립부. 모듈 경계는 PRD 5.3 을 그대로 따른다.
//!
//! credentials → usage_client → poller → (history / notifier / UI)

mod credentials;
mod history;
mod model;
mod notifier;
mod poller;
mod settings;
mod usage_client;

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};

use model::AppState;
use poller::Poller;
use settings::Settings;

/// 앱 전역 상태.
struct AppCtx {
    poller: Mutex<Poller>,
    settings: Mutex<Settings>,
}

// ─────────────────────────── 커맨드 (프론트 `src/lib/ipc.ts` 와 1:1) ───────────────────────────

#[tauri::command]
fn get_state(ctx: tauri::State<'_, AppCtx>) -> AppState {
    ctx.poller.lock().unwrap().state().clone()
}

#[tauri::command]
fn refresh_now() -> Result<(), String> {
    // TODO(M2 2.2): poller 수동 트리거 + 5초 스로틀
    Err("M2에서 구현 예정".into())
}

#[tauri::command]
fn get_settings(ctx: tauri::State<'_, AppCtx>) -> Settings {
    ctx.settings.lock().unwrap().clone()
}

#[tauri::command]
fn update_settings(
    _ctx: tauri::State<'_, AppCtx>,
    _patch: serde_json::Value,
) -> Result<Settings, String> {
    // TODO(M4 4.2): 병합 → 저장 → 즉시 적용(폴링 주기·테마·투명도·자동 시작)
    Err("M4에서 구현 예정".into())
}

#[tauri::command]
fn query_history(_from: String, _to: String) -> Result<Vec<model::HistoryEntry>, String> {
    // TODO(M5 5.1)
    Err("M5에서 구현 예정".into())
}

#[tauri::command]
fn set_widget_mode(_mode: String) -> Result<(), String> {
    // TODO(M3 3.1): 컴팩트(220×90) ↔ 확장(320×360) 창 크기 전환 + 설정 저장
    Err("M3에서 구현 예정".into())
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
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Claude Usage Widget 설정")
    .inner_size(420.0, 520.0)
    .resizable(true)
    .center()
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ─────────────────────────── 창 배치 ───────────────────────────

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

// ─────────────────────────── 트레이 (FR-5) ───────────────────────────

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "위젯 표시/숨김", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "지금 새로고침", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "설정", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle, &refresh, &settings_item, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Claude Usage Widget")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                if let Some(w) = app.get_webview_window("widget") {
                    let visible = w.is_visible().unwrap_or(false);
                    let _ = if visible { w.hide() } else { w.show() };
                }
            }
            "refresh" => {
                // TODO(M2 2.2): poller.refresh_now()
            }
            "settings" => {
                let _ = open_settings_window(app.clone());
            }
            // 창을 닫아도 앱은 살아 있고, 완전 종료는 여기서만 (FR-5)
            "quit" => app.exit(0),
            _ => {}
        });

    // TODO(M3 3.2): 사용률 구간(정상/주의/위험)에 따라 아이콘 색상 교체
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

// ─────────────────────────── 엔트리 ───────────────────────────

pub fn run() {
    let mut builder = tauri::Builder::default();

    // 단일 인스턴스가 먼저 등록되어야 한다. 두 번째 실행은 기존 위젯을 띄우고 종료.
    #[cfg(windows)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppCtx {
            poller: Mutex::new(Poller::new()),
            settings: Mutex::new(Settings::default()),
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_now,
            get_settings,
            update_settings,
            query_history,
            set_widget_mode,
            open_settings_window,
        ])
        .setup(|app| {
            setup_tray(app.handle())?;

            // 위젯은 config 에서 visible:false 로 만들어 두고, 배치한 뒤에 보여준다.
            // 그러지 않으면 좌상단에 떴다가 우측 하단으로 튀는 게 보인다.
            if let Some(widget) = app.get_webview_window("widget") {
                if let Err(e) = place_bottom_right(&widget) {
                    // 배치 실패가 창을 못 띄우는 사유가 되면 안 된다
                    eprintln!("위젯 기본 위치 설정 실패(기본 배치로 진행): {e}");
                }
                widget.show()?;
            }

            // TODO(M2 2.2): poller 시작 → EVENT_STATE 브로드캐스트
            Ok(())
        })
        .on_window_event(|window, event| {
            // FR-5: 창을 닫아도 앱은 트레이에 상주한다
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("Tauri 앱 실행 실패");
}
