//! FR-8 설정. 로컬 JSON 으로 저장하고 즉시 적용한다 (앱 재시작 불필요).
//! 프론트의 `Settings` 인터페이스와 1:1 대응.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::notifier::DEFAULT_THRESHOLDS;
use crate::poller::DEFAULT_INTERVAL_SECS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WidgetMode {
    Compact,
    Expanded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

/// 사용자가 드래그로 조정한 위젯 크기 (논리 픽셀, 확장 모드 기준).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WinSize {
    pub width: f64,
    pub height: f64,
}

/// 위젯 리사이즈 허용 범위 (논리 픽셀). 최소는 게이지·차트가 깨지지 않는 하한,
/// 최대는 "위젯"이라는 정체성을 벗어나지 않는 상한.
pub const WIDGET_MIN_SIZE: (f64, f64) = (240.0, 300.0);
pub const WIDGET_MAX_SIZE: (f64, f64) = (560.0, 900.0);

/// 위젯 색상. 전부 `#rrggbb` 16진 문자열이며 프론트가 CSS 변수로 꽂는다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Colors {
    pub text: String,
    pub text_dim: String,
    /// 사용률 구간별 게이지 색 (<60 / 60~85 / >85)
    pub gauge_normal: String,
    pub gauge_warning: String,
    pub gauge_danger: String,
    /// 게이지 빈 부분
    pub gauge_track: String,
    pub background: String,
    /// 차트 선 색.
    ///
    /// 게이지의 구간 색(정상/주의/위험)과 **분리해서** 둔다. 재사용하면
    /// 주간 추이선이 황토색으로 그려져 "주의 상태"처럼 읽힌다 —
    /// 실제로는 그냥 다른 계열일 뿐인데도.
    pub chart_session: String,
    pub chart_weekly: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            text: "#f3f3f3".into(),
            text_dim: "#8b8b8b".into(),
            gauge_normal: "#3fb950".into(),
            gauge_warning: "#d29922".into(),
            gauge_danger: "#f85149".into(),
            gauge_track: "#3a3a3a".into(),
            background: "#202020".into(),
            chart_session: "#3fb950".into(),
            chart_weekly: "#58a6ff".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub polling_interval_sec: u64,
    pub thresholds: Vec<f64>,
    pub notifications_enabled: bool,
    pub notify_on_reset: bool,
    pub theme: Theme,
    /// 위젯 배경 불투명도 0.3 ~ 1.0
    pub opacity: f64,
    pub auto_start: bool,
    pub widget_mode: WidgetMode,
    pub widget_position: Option<Position>,
    /// 확장 모드에서 사용자가 조정한 크기. 없으면 기본 크기.
    /// (기존 설정 파일에는 이 키가 없으므로 default 필수 — 빠지면 전체가 기본값으로 리셋된다)
    #[serde(default)]
    pub widget_size: Option<WinSize>,
    pub colors: Colors,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            polling_interval_sec: DEFAULT_INTERVAL_SECS,
            thresholds: DEFAULT_THRESHOLDS.to_vec(),
            notifications_enabled: true,
            notify_on_reset: false, // FR-6: 기본 꺼짐
            theme: Theme::System,
            opacity: 0.85,
            auto_start: false,
            // 처음 실행에서는 정보를 다 보여준다. 접는 건 사용자가 정한다.
            widget_mode: WidgetMode::Expanded,
            widget_position: None,
            widget_size: None,
            colors: Colors::default(),
        }
    }
}

/// 프론트가 설정 변경을 구독하는 이벤트. `src/lib/types.ts` 와 일치해야 한다.
pub const EVENT_SETTINGS: &str = "settings://changed";

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("설정을 저장할 수 없습니다: {0}")]
    Io(#[from] std::io::Error),
    #[error("설정 형식이 올바르지 않습니다: {0}")]
    Serde(#[from] serde_json::Error),
}

/// 설정 파일 + 메모리 캐시.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// 파일이 없거나 깨졌으면 기본값으로 시작한다 — 설정 때문에 앱이 못 뜨면 안 된다.
    ///
    /// 읽어온 값도 `sanitized()` 를 통과시킨다. 설정 파일은 사람이 손댈 수 있어서,
    /// 구조는 맞지만 범위를 벗어난 값(`opacity: -10` 등)이 그대로 들어올 수 있다.
    pub fn load(path: PathBuf) -> Self {
        let current = match std::fs::read_to_string(&path) {
            Err(_) => Settings::default(), // 파일이 아직 없음 — 정상
            Ok(text) => match serde_json::from_str::<Settings>(strip_bom(&text)) {
                Ok(s) => s,
                Err(e) => {
                    // 조용히 기본값으로 되돌리면 사용자가 손댄 설정이
                    // 말없이 사라진다. 최소한 흔적은 남긴다.
                    eprintln!("설정 파일을 읽을 수 없어 기본값으로 시작합니다: {e}");
                    Settings::default()
                }
            },
        };

        Self {
            path,
            current: Mutex::new(current.sanitized()),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    /// 부분 갱신. 넘어온 키만 덮어쓰고 나머지는 유지한다.
    pub fn update(&self, patch: serde_json::Value) -> Result<Settings, SettingsError> {
        let mut guard = self.current.lock().unwrap();

        let mut merged = serde_json::to_value(&*guard)?;
        merge(&mut merged, patch);

        let next: Settings = serde_json::from_value(merged)?;
        let next = next.sanitized();

        write_atomic(&self.path, &serde_json::to_string_pretty(&next)?)?;
        *guard = next.clone();

        Ok(next)
    }
}

impl Settings {
    /// 범위를 벗어난 값을 스펙 안으로 되돌린다.
    ///
    /// 설정 파일은 사람이 손댈 수 있고 UI 도 실수할 수 있다. 여기서 한 번 막아두면
    /// 폴링 주기·투명도 이상값이 앱 안쪽으로 새지 않는다.
    fn sanitized(mut self) -> Self {
        self.polling_interval_sec = self.polling_interval_sec.clamp(
            crate::poller::MIN_INTERVAL_SECS,
            crate::poller::MAX_INTERVAL_SECS,
        );
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.3, 1.0)
        } else {
            Settings::default().opacity
        };
        self.thresholds
            .retain(|t| t.is_finite() && (0.0..=100.0).contains(t));
        self.widget_size = self.widget_size.and_then(|s| {
            if !s.width.is_finite() || !s.height.is_finite() {
                return None; // 손으로 고친 파일의 NaN 등 — 기본 크기로 되돌린다
            }
            Some(WinSize {
                width: s.width.clamp(WIDGET_MIN_SIZE.0, WIDGET_MAX_SIZE.0),
                height: s.height.clamp(WIDGET_MIN_SIZE.1, WIDGET_MAX_SIZE.1),
            })
        });
        self.colors = self.colors.sanitized();
        self
    }
}

/// `#rrggbb` 인지 확인한다. 프론트의 `<input type="color">` 는 항상 이 형식이지만,
/// 설정 파일을 직접 고치거나 다른 경로로 들어온 값이 위젯을 읽을 수 없게 만들면 안 된다.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7
        && s.starts_with('#')
        && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl Colors {
    /// 형식에 맞지 않는 색은 기본값으로 되돌린다.
    fn sanitized(self) -> Self {
        let d = Colors::default();
        let pick = |v: String, fallback: String| if is_hex_color(&v) { v } else { fallback };

        Self {
            text: pick(self.text, d.text),
            text_dim: pick(self.text_dim, d.text_dim),
            gauge_normal: pick(self.gauge_normal, d.gauge_normal),
            gauge_warning: pick(self.gauge_warning, d.gauge_warning),
            gauge_danger: pick(self.gauge_danger, d.gauge_danger),
            gauge_track: pick(self.gauge_track, d.gauge_track),
            background: pick(self.background, d.background),
            chart_session: pick(self.chart_session, d.chart_session),
            chart_weekly: pick(self.chart_weekly, d.chart_weekly),
        }
    }
}

/// UTF-8 BOM 을 걷어낸다.
///
/// `serde_json` 은 BOM 으로 시작하는 문자열을 거부한다. 그런데 Windows 메모장과
/// PowerShell 의 `Out-File -Encoding utf8` 은 기본으로 BOM 을 붙인다.
/// 설정 파일은 사람이 직접 고칠 수 있어야 하므로 여기서 흡수한다.
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// `patch` 의 객체 키를 `base` 에 재귀적으로 덮어쓴다.
///
/// `colors` 처럼 중첩된 값도 일부만 보낼 수 있게 하기 위한 것이다.
/// (`{"colors":{"text":"#fff"}}` 를 보내도 나머지 색이 지워지지 않는다)
fn merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base), serde_json::Value::Object(patch)) => {
            for (k, v) in patch {
                merge(base.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (base, patch) => *base = patch,
    }
}

/// 임시 파일에 쓰고 rename — 저장 도중 종료돼도 설정이 반쯤 쓰이지 않는다.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_keeps_untouched_nested_keys() {
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        merge(&mut base, json!({ "colors": { "text": "#ff0000" } }));

        let s: Settings = serde_json::from_value(base).unwrap();
        assert_eq!(s.colors.text, "#ff0000");
        // 같이 보내지 않은 색은 유지되어야 한다
        assert_eq!(s.colors.gauge_normal, Colors::default().gauge_normal);
        assert_eq!(s.polling_interval_sec, DEFAULT_INTERVAL_SECS);
    }

    #[test]
    fn merge_replaces_scalars_and_arrays() {
        let mut base = json!({ "a": 1, "list": [1, 2] });
        merge(&mut base, json!({ "a": 2, "list": [9] }));
        assert_eq!(base, json!({ "a": 2, "list": [9] }));
    }

    #[test]
    fn sanitize_rejects_non_hex_colors() {
        let s = Settings {
            colors: Colors {
                text: "transparent".into(), // 위젯을 못 읽게 만드는 값
                gauge_normal: "#00ff00".into(),
                ..Colors::default()
            },
            ..Default::default()
        }
        .sanitized();

        assert_eq!(s.colors.text, Colors::default().text, "잘못된 색은 기본값으로");
        assert_eq!(s.colors.gauge_normal, "#00ff00", "정상 값은 유지");
    }

    #[test]
    fn load_sanitizes_hand_edited_file() {
        let dir = std::env::temp_dir().join("cuw-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hand-edited.json");

        // 구조는 맞지만 범위를 벗어난 값 — 사람이 파일을 직접 고친 경우
        let mut raw = serde_json::to_value(Settings::default()).unwrap();
        raw["opacity"] = json!(-10.0);
        raw["pollingIntervalSec"] = json!(1);
        std::fs::write(&path, raw.to_string()).unwrap();

        let loaded = SettingsStore::load(path.clone()).get();
        assert_eq!(loaded.opacity, 0.3);
        assert_eq!(loaded.polling_interval_sec, crate::poller::MIN_INTERVAL_SECS);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let s = Settings {
            polling_interval_sec: 1,
            opacity: 5.0,
            thresholds: vec![80.0, 150.0, -3.0],
            ..Default::default()
        }
        .sanitized();

        assert_eq!(s.polling_interval_sec, crate::poller::MIN_INTERVAL_SECS);
        assert_eq!(s.opacity, 1.0);
        assert_eq!(s.thresholds, vec![80.0]);
    }

    /// v0.2.x 설정 파일에는 widgetSize 키가 없다 — 그것 때문에 나머지 설정이
    /// 기본값으로 리셋되면 안 되고, 범위 밖 값은 스펙 안으로 들어와야 한다.
    #[test]
    fn widget_size_missing_key_and_clamp() {
        let mut raw = serde_json::to_value(Settings::default()).unwrap();
        raw.as_object_mut().unwrap().remove("widgetSize");
        raw["opacity"] = json!(0.5);

        let s: Settings = serde_json::from_str(&raw.to_string()).unwrap();
        assert!(s.widget_size.is_none(), "키가 없으면 None (구버전 파일 호환)");
        assert_eq!(s.opacity, 0.5, "다른 설정은 유지");

        let s = Settings {
            widget_size: Some(WinSize { width: 9999.0, height: 10.0 }),
            ..Default::default()
        }
        .sanitized();
        let size = s.widget_size.unwrap();
        assert_eq!(size.width, WIDGET_MAX_SIZE.0);
        assert_eq!(size.height, WIDGET_MIN_SIZE.1);

        let s = Settings {
            widget_size: Some(WinSize { width: f64::NAN, height: 400.0 }),
            ..Default::default()
        }
        .sanitized();
        assert!(s.widget_size.is_none(), "NaN 은 기본 크기로 되돌린다");
    }

    /// 메모장·PowerShell 로 고친 설정 파일에는 BOM 이 붙는다.
    /// 이걸 못 읽으면 사용자가 손댄 설정이 말없이 기본값으로 되돌아간다.
    #[test]
    fn load_accepts_utf8_bom() {
        let dir = std::env::temp_dir().join("cuw-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bom.json");

        let mut raw = serde_json::to_value(Settings::default()).unwrap();
        raw["thresholds"] = json!([17.0]);
        std::fs::write(&path, format!("\u{feff}{raw}")).unwrap();

        assert_eq!(SettingsStore::load(path.clone()).get().thresholds, vec![17.0]);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn strip_bom_leaves_plain_text_alone() {
        assert_eq!(strip_bom("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_bom("\u{feff}{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn broken_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join("cuw-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.json");
        std::fs::write(&path, "{ not json").unwrap();

        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get().polling_interval_sec, DEFAULT_INTERVAL_SECS);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn update_persists_and_survives_reload() {
        let dir = std::env::temp_dir().join("cuw-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.json");
        std::fs::remove_file(&path).ok();

        let store = SettingsStore::load(path.clone());
        let updated = store
            .update(json!({ "colors": { "gaugeDanger": "#123456" }, "opacity": 0.5 }))
            .unwrap();
        assert_eq!(updated.colors.gauge_danger, "#123456");

        let reloaded = SettingsStore::load(path.clone());
        assert_eq!(reloaded.get().colors.gauge_danger, "#123456");
        assert_eq!(reloaded.get().opacity, 0.5);

        std::fs::remove_file(path).ok();
    }
}
