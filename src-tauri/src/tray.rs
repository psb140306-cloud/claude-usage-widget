//! FR-5 트레이 상주.
//!
//! 아이콘 색은 사용률 최고값의 구간(정상/주의/위험)을 따르고, 툴팁은 요약을 보여준다.
//! 아이콘은 파일로 두지 않고 매번 그린다 — 색이 설정에 따라 바뀌므로 미리 만들어 둘 수 없다.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::AppHandle;

use crate::model::{severity_of, AppState, Severity};
use crate::settings::Colors;

pub const TRAY_ID: &str = "main";

/// 트레이 아이콘 한 변의 픽셀 수. Windows 는 16~32 를 알아서 축소해 쓴다.
const ICON_SIZE: u32 = 32;

/// 메뉴 항목 id
pub const ITEM_TOGGLE: &str = "toggle";
pub const ITEM_REFRESH: &str = "refresh";
pub const ITEM_SETTINGS: &str = "settings";
pub const ITEM_QUIT: &str = "quit";

pub fn build(
    app: &AppHandle,
    on_menu: impl Fn(&AppHandle, &str) + Send + Sync + 'static,
) -> tauri::Result<TrayIcon> {
    let toggle = MenuItem::with_id(app, ITEM_TOGGLE, "위젯 표시/숨김", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, ITEM_REFRESH, "지금 새로고침", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ITEM_SETTINGS, "설정", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, ITEM_QUIT, "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &refresh, &settings, &sep, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Claude Usage Widget")
        .menu(&menu)
        .icon(dot_icon([0x80, 0x80, 0x80]))
        .on_menu_event(move |app, event| on_menu(app, event.id.as_ref()))
        .build(app)
}

/// 상태가 바뀔 때마다 아이콘 색과 툴팁을 갱신한다.
pub fn update(app: &AppHandle, state: &AppState, colors: &Colors) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    let (peak, tooltip) = summarize(state);
    let color = match state {
        // 값이 없거나 못 믿는 상태는 회색으로 — 색으로 안심시키면 안 된다
        AppState::Ok { .. } => severity_color(severity_of(peak), colors),
        _ => [0x80, 0x80, 0x80],
    };

    let _ = tray.set_icon(Some(dot_icon(color)));
    let _ = tray.set_tooltip(Some(&tooltip));
}

/// 툴팁 문구와 색 판정에 쓸 최고 사용률.
fn summarize(state: &AppState) -> (Option<f64>, String) {
    match state {
        AppState::Loading => (None, "불러오는 중…".into()),
        AppState::NeedsReauth => (
            None,
            "재인증 필요 — Claude Code를 한 번 실행해 주세요".into(),
        ),
        AppState::Unavailable { .. } => (None, "사용량을 가져올 수 없습니다".into()),
        AppState::Ok { snapshot } | AppState::Stale { snapshot, .. } => {
            let mut parts = Vec::new();
            if let Some(s) = &snapshot.session {
                parts.push(format!("세션 {}%", s.utilization.floor()));
            }
            if let Some(w) = &snapshot.weekly {
                parts.push(format!("주간 {}%", w.utilization.floor()));
            }
            if matches!(state, AppState::Stale { .. }) {
                parts.push("(오래된 값)".into());
            }
            let text = if parts.is_empty() {
                "Claude Usage Widget".to_string()
            } else {
                parts.join(" · ")
            };
            (snapshot.peak_utilization(), text)
        }
    }
}

fn severity_color(severity: Severity, colors: &Colors) -> [u8; 3] {
    let hex = match severity {
        Severity::Normal => &colors.gauge_normal,
        Severity::Warning => &colors.gauge_warning,
        Severity::Danger => &colors.gauge_danger,
    };
    parse_hex(hex).unwrap_or([0x3f, 0xb9, 0x50])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

/// 지정한 색의 원을 그려 아이콘으로 만든다.
///
/// 가장자리는 알파로 부드럽게 처리한다 — 계단이 보이면 트레이에서 유난히 지저분하다.
fn dot_icon(color: [u8; 3]) -> Image<'static> {
    let size = ICON_SIZE as f32;
    let center = (size - 1.0) / 2.0;
    let radius = size / 2.0 - 1.5;

    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = ((radius - dist + 0.5).clamp(0.0, 1.0) * 255.0) as u8;

            rgba.extend_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }

    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LimitWindow, UsageSnapshot};
    use chrono::Utc;

    fn snap(session: f64, weekly: f64) -> UsageSnapshot {
        UsageSnapshot {
            fetched_at: Utc::now(),
            session: Some(LimitWindow {
                utilization: session,
                resets_at: None,
            }),
            weekly: Some(LimitWindow {
                utilization: weekly,
                resets_at: None,
            }),
            weekly_opus: None,
            model_scoped: vec![],
        }
    }

    #[test]
    fn tooltip_summarizes_both_limits() {
        let (peak, text) = summarize(&AppState::Ok {
            snapshot: snap(42.0, 61.0),
        });
        assert_eq!(peak, Some(61.0));
        assert_eq!(text, "세션 42% · 주간 61%");
    }

    #[test]
    fn stale_tooltip_is_marked() {
        let (_, text) = summarize(&AppState::Stale {
            snapshot: snap(10.0, 20.0),
            reason: "타임아웃".into(),
        });
        assert!(text.contains("오래된 값"), "스테일임을 알려야 한다: {text}");
    }

    #[test]
    fn icon_is_rgba_of_expected_size() {
        let img = dot_icon([1, 2, 3]);
        assert_eq!(img.width(), ICON_SIZE);
        assert_eq!(img.height(), ICON_SIZE);
        assert_eq!(img.rgba().len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn icon_center_is_opaque_and_corner_transparent() {
        let img = dot_icon([10, 20, 30]);
        let px = |x: u32, y: u32| {
            let i = ((y * ICON_SIZE + x) * 4) as usize;
            img.rgba()[i..i + 4].to_vec()
        };
        assert_eq!(px(16, 16), vec![10, 20, 30, 255], "가운데는 불투명");
        assert_eq!(px(0, 0)[3], 0, "모서리는 투명");
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#3fb950"), Some([0x3f, 0xb9, 0x50]));
        assert_eq!(parse_hex("3fb950"), None, "# 없으면 거부");
        assert_eq!(parse_hex("#xyzxyz"), None);
    }
}
