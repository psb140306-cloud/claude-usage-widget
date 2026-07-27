//! FR-6 임계값 알림.
//!
//! 중복 억제 규칙: **동일 임계값 · 동일 리셋 주기 안에서 1회만**.
//! 리셋 시각이 바뀌면(= 새 주기) 발송 이력을 비운다.

// M4 에서 구현되면 제거할 것.
#![allow(dead_code)]

use crate::model::UsageSnapshot;

/// FR-6 기본 임계값 (%)
pub const DEFAULT_THRESHOLDS: [f64; 2] = [80.0, 95.0];

/// 어떤 한도에 대한 알림인지
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Session,
    Weekly,
}

/// 발송 이력을 들고 중복을 억제한다.
///
/// TODO(M4 4.1): evaluate() 구현 + tauri-plugin-notification 연결.
pub struct Notifier {
    _thresholds: Vec<f64>,
}

impl Notifier {
    pub fn new(thresholds: Vec<f64>) -> Self {
        Self {
            _thresholds: thresholds,
        }
    }

    /// 이번 스냅샷에서 새로 넘긴 임계값 목록을 돌려준다 (비어 있으면 발송 없음).
    pub fn evaluate(&mut self, _snapshot: &UsageSnapshot) -> Vec<(Bucket, f64)> {
        todo!("M4 4.1 임계값 평가 + 중복 억제")
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new(DEFAULT_THRESHOLDS.to_vec())
    }
}
