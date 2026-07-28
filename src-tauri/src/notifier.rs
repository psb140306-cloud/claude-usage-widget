//! FR-6 임계값 알림.
//!
//! 중복 억제 규칙: **동일 임계값 · 동일 리셋 주기 안에서 1회만**.
//! 리셋 시각이 바뀌면 새 주기이므로 발송 이력을 비운다.
//!
//! 판정은 순수 함수([`Notifier::evaluate`])로 두고, 실제 토스트 발송은
//! 호출부(lib.rs)가 한다. 그래야 알림 로직을 창 없이 테스트할 수 있다.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::model::UsageSnapshot;

/// FR-6 기본 임계값 (%)
pub const DEFAULT_THRESHOLDS: [f64; 2] = [80.0, 95.0];

/// 어떤 한도에 대한 알림인지
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Session,
    Weekly,
}

impl Bucket {
    pub fn label(self) -> &'static str {
        match self {
            Bucket::Session => "세션",
            Bucket::Weekly => "주간",
        }
    }
}

/// 발송해야 할 알림 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub bucket: Bucket,
    /// 넘긴 임계값 (%)
    pub threshold: f64,
    /// 현재 사용률 (%)
    pub utilization: f64,
    pub kind: AlertKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// 임계값 도달
    Threshold,
    /// 리셋 완료 (기본 꺼짐)
    Reset,
}

impl Alert {
    pub fn title(&self) -> String {
        match self.kind {
            AlertKind::Threshold => format!("{} 한도 {}% 도달", self.bucket.label(), self.threshold),
            AlertKind::Reset => format!("{} 한도 리셋됨", self.bucket.label()),
        }
    }

    pub fn body(&self) -> String {
        match self.kind {
            AlertKind::Threshold => {
                format!("현재 {:.0}% 사용 중입니다", self.utilization.floor())
            }
            AlertKind::Reset => "사용량이 초기화되었습니다".to_string(),
        }
    }
}

/// 이미 보낸 알림을 기억해 같은 주기에 두 번 보내지 않는다.
#[derive(Debug, Default)]
pub struct Notifier {
    thresholds: Vec<f64>,
    notify_on_reset: bool,
    /// 버킷별 현재 주기의 리셋 시각. 바뀌면 새 주기다.
    cycles: std::collections::HashMap<Bucket, Option<DateTime<Utc>>>,
    /// 이번 주기에 이미 발송한 (버킷, 임계값)
    sent: HashSet<(Bucket, u64)>,
    /// 첫 평가인지. 앱을 켜자마자 과거 임계값으로 알림이 쏟아지면 안 된다.
    primed: bool,
}

impl Notifier {
    pub fn new(thresholds: Vec<f64>, notify_on_reset: bool) -> Self {
        Self {
            thresholds,
            notify_on_reset,
            ..Default::default()
        }
    }

    pub fn configure(&mut self, thresholds: Vec<f64>, notify_on_reset: bool) {
        self.thresholds = thresholds;
        self.notify_on_reset = notify_on_reset;
    }

    /// 이번 스냅샷에서 새로 보내야 할 알림 목록.
    ///
    /// 첫 호출은 현재 상태를 "이미 본 것"으로 기록만 하고 아무것도 보내지 않는다.
    /// 그러지 않으면 앱을 켤 때마다 이미 넘긴 임계값이 다시 울린다.
    pub fn evaluate(&mut self, snapshot: &UsageSnapshot) -> Vec<Alert> {
        let buckets = [
            (Bucket::Session, snapshot.session.as_ref()),
            (Bucket::Weekly, snapshot.weekly.as_ref()),
        ];

        let mut alerts = Vec::new();

        for (bucket, window) in buckets {
            let Some(window) = window else { continue };
            let resets_at = window.resets_at;

            // 리셋 주기가 바뀌었으면 이 버킷의 발송 이력을 비운다
            let previous = self.cycles.insert(bucket, resets_at);
            let cycle_changed = matches!(previous, Some(prev) if prev != resets_at);
            if cycle_changed {
                self.sent.retain(|(b, _)| *b != bucket);
                if self.notify_on_reset && self.primed {
                    alerts.push(Alert {
                        bucket,
                        threshold: 0.0,
                        utilization: window.utilization,
                        kind: AlertKind::Reset,
                    });
                }
            }

            for &threshold in &self.thresholds {
                if window.utilization < threshold {
                    continue;
                }
                let key = (bucket, threshold.to_bits());
                if !self.sent.insert(key) {
                    continue; // 이번 주기에 이미 보냈다
                }
                if self.primed {
                    alerts.push(Alert {
                        bucket,
                        threshold,
                        utilization: window.utilization,
                        kind: AlertKind::Threshold,
                    });
                }
            }
        }

        self.primed = true;
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LimitWindow;

    fn snap(session: f64, resets: Option<DateTime<Utc>>) -> UsageSnapshot {
        UsageSnapshot {
            fetched_at: Utc::now(),
            session: Some(LimitWindow {
                utilization: session,
                resets_at: resets,
            }),
            weekly: None,
            weekly_opus: None,
            model_scoped: vec![],
        }
    }

    fn at(hour: u32) -> Option<DateTime<Utc>> {
        Some(
            DateTime::parse_from_rfc3339(&format!("2026-07-27T{hour:02}:00:00Z"))
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn first_evaluation_is_silent() {
        let mut n = Notifier::new(DEFAULT_THRESHOLDS.to_vec(), false);
        // 앱을 켰더니 이미 90% — 여기서 울리면 실행할 때마다 알림이 온다
        assert!(n.evaluate(&snap(90.0, at(12))).is_empty());
    }

    #[test]
    fn fires_once_when_crossing_threshold() {
        let mut n = Notifier::new(DEFAULT_THRESHOLDS.to_vec(), false);
        n.evaluate(&snap(10.0, at(12))); // 초기화

        let alerts = n.evaluate(&snap(82.0, at(12)));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80.0);
        assert_eq!(alerts[0].bucket, Bucket::Session);

        // 같은 주기에 계속 80% 를 넘어도 다시 울리지 않는다
        assert!(n.evaluate(&snap(85.0, at(12))).is_empty());
        assert!(n.evaluate(&snap(90.0, at(12))).is_empty());
    }

    #[test]
    fn higher_threshold_fires_separately() {
        let mut n = Notifier::new(DEFAULT_THRESHOLDS.to_vec(), false);
        n.evaluate(&snap(10.0, at(12)));
        assert_eq!(n.evaluate(&snap(82.0, at(12))).len(), 1); // 80

        let alerts = n.evaluate(&snap(96.0, at(12)));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 95.0);
    }

    #[test]
    fn new_cycle_allows_firing_again() {
        let mut n = Notifier::new(DEFAULT_THRESHOLDS.to_vec(), false);
        n.evaluate(&snap(10.0, at(12)));
        assert_eq!(n.evaluate(&snap(82.0, at(12))).len(), 1);

        // 리셋 시각이 바뀌면 새 주기 — 다시 울려야 한다
        let alerts = n.evaluate(&snap(82.0, at(17)));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80.0);
    }

    #[test]
    fn reset_notification_is_opt_in() {
        let mut off = Notifier::new(vec![], false);
        off.evaluate(&snap(50.0, at(12)));
        assert!(off.evaluate(&snap(1.0, at(17))).is_empty(), "기본은 꺼짐");

        let mut on = Notifier::new(vec![], true);
        on.evaluate(&snap(50.0, at(12)));
        let alerts = on.evaluate(&snap(1.0, at(17)));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Reset);
    }

    #[test]
    fn message_text() {
        let a = Alert {
            bucket: Bucket::Session,
            threshold: 80.0,
            utilization: 82.7,
            kind: AlertKind::Threshold,
        };
        assert_eq!(a.title(), "세션 한도 80% 도달");
        assert_eq!(a.body(), "현재 82% 사용 중입니다");
    }
}
