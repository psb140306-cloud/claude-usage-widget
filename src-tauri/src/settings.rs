//! FR-8 설정. 로컬 JSON 으로 저장하고 즉시 적용한다 (앱 재시작 불필요).
//! 프론트의 `Settings` 인터페이스와 1:1 대응.

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub polling_interval_sec: u64,
    pub thresholds: Vec<f64>,
    pub notifications_enabled: bool,
    pub notify_on_reset: bool,
    pub theme: Theme,
    pub opacity: f64,
    pub auto_start: bool,
    pub widget_mode: WidgetMode,
    pub widget_position: Option<Position>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            polling_interval_sec: DEFAULT_INTERVAL_SECS,
            thresholds: DEFAULT_THRESHOLDS.to_vec(),
            notifications_enabled: true,
            notify_on_reset: false, // FR-6: 기본 꺼짐
            theme: Theme::System,
            opacity: 1.0,
            auto_start: false,
            widget_mode: WidgetMode::Compact,
            widget_position: None,
        }
    }
}
