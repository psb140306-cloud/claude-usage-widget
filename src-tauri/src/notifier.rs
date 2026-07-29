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

/// 같은 알림을 이 시간 안에는 다시 보내지 않는다.
///
/// 주기 판정에 구멍이 있어도 사용자가 토스트 폭탄을 맞지 않게 하는 최후 방어선이다.
/// (실제로 주기 오판으로 폴링마다 알림이 반복된 적이 있다)
const MIN_REALERT_MINUTES: i64 = 30;

/// 사용률이 이만큼 **떨어지면** 리셋으로 본다.
///
/// 리셋 시각(`resets_at`)이 바뀌었는지로 판정하면 안 된다. 그 값은 마이크로초까지
/// 들어 있고 폴링마다 미세하게 달라질 수 있어, 매번 새 주기로 오판한다.
/// 사용률은 리셋이 아니면 줄어들지 않으므로 이쪽이 확실한 신호다.
const RESET_DROP_POINTS: f64 = 5.0;

/// 이미 보낸 알림을 기억해 같은 주기에 두 번 보내지 않는다.
#[derive(Debug, Default)]
pub struct Notifier {
    thresholds: Vec<f64>,
    notify_on_reset: bool,
    /// 버킷별 직전 사용률. 크게 떨어지면 새 주기다.
    last_utilization: std::collections::HashMap<Bucket, f64>,
    /// 이번 주기에 이미 발송한 (버킷, 임계값)
    sent: HashSet<(Bucket, u64)>,
    /// (버킷, 임계값) 별 마지막 발송 시각 — 최소 재알림 간격 판정용
    last_sent_at: std::collections::HashMap<(Bucket, u64), DateTime<Utc>>,
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
    pub fn evaluate(&mut self, snapshot: &UsageSnapshot, now: DateTime<Utc>) -> Vec<Alert> {
        let buckets = [
            (Bucket::Session, snapshot.session.as_ref()),
            (Bucket::Weekly, snapshot.weekly.as_ref()),
        ];

        let mut alerts = Vec::new();

        for (bucket, window) in buckets {
            let Some(window) = window else { continue };
            let utilization = window.utilization;

            // 사용률이 크게 떨어졌으면 리셋된 것 — 이 버킷의 발송 이력을 비운다
            let previous = self.last_utilization.insert(bucket, utilization);
            let reset = matches!(previous, Some(prev) if utilization < prev - RESET_DROP_POINTS);
            if reset {
                self.sent.retain(|(b, _)| *b != bucket);
                if self.notify_on_reset && self.primed {
                    alerts.push(Alert {
                        bucket,
                        threshold: 0.0,
                        utilization,
                        kind: AlertKind::Reset,
                    });
                }
            }

            for &threshold in &self.thresholds {
                if utilization < threshold {
                    continue;
                }
                let key = (bucket, threshold.to_bits());
                if self.sent.contains(&key) {
                    continue; // 이번 주기에 이미 보냈다
                }
                if !self.primed {
                    // 시작 직후엔 기록만 하고 울리지 않는다
                    self.sent.insert(key);
                    continue;
                }
                // 주기 판정이 틀려도 짧은 간격으로 반복되지 않게 막는다.
                // 여기서 막힐 때는 `sent` 에 넣지 **않는다** — 넣어 버리면
                // 간격이 지나도 영영 울리지 않는다.
                if let Some(prev) = self.last_sent_at.get(&key) {
                    if now.signed_duration_since(*prev)
                        < chrono::Duration::minutes(MIN_REALERT_MINUTES)
                    {
                        continue;
                    }
                }
                self.sent.insert(key);
                self.last_sent_at.insert(key, now);

                alerts.push(Alert {
                    bucket,
                    threshold,
                    utilization,
                    kind: AlertKind::Threshold,
                });
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

    /// 테스트에서 시간을 흘려보낸다 (최소 재알림 간격을 넘기기 위해)
    fn later(minutes: i64) -> DateTime<Utc> {
        base_time() + chrono::Duration::minutes(minutes)
    }

    fn base_time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-27T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

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
        assert!(n.evaluate(&snap(90.0, at(12)), base_time()).is_empty());
    }

    #[test]
    fn fires_once_when_crossing_threshold() {
        let mut n = Notifier::new(DEFAULT_THRESHOLDS.to_vec(), false);
        n.evaluate(&snap(10.0, at(12)), base_time()); // 초기화

        let alerts = n.evaluate(&snap(82.0, at(12)), later(1));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80.0);
        assert_eq!(alerts[0].bucket, Bucket::Session);

        // 같은 주기에 계속 80% 를 넘어도 다시 울리지 않는다
        assert!(n.evaluate(&snap(85.0, at(12)), later(2)).is_empty());
        assert!(n.evaluate(&snap(90.0, at(12)), later(3)).is_empty());
    }

    /// 실제로 터졌던 버그: 리셋 시각이 폴링마다 미세하게 달라지자
    /// 매번 새 주기로 오판해 알림이 반복됐다.
    #[test]
    fn drifting_reset_time_does_not_refire() {
        let mut n = Notifier::new(vec![80.0], false);
        n.evaluate(&snap(10.0, at(12)), base_time());
        assert_eq!(n.evaluate(&snap(82.0, at(12)), later(1)).len(), 1);

        // 사용률은 그대로인데 리셋 시각만 바뀐 경우 — 리셋이 아니다
        for i in 2..8 {
            let alerts = n.evaluate(&snap(82.0, at(12 + i as u32)), later(i * 40));
            assert!(alerts.is_empty(), "{i}번째 폴링에서 재발송됨: {alerts:?}");
        }
    }

    /// 주기 판정이 틀리더라도 짧은 간격으로 반복되지 않아야 한다.
    #[test]
    fn same_alert_is_rate_limited() {
        let mut n = Notifier::new(vec![80.0], false);
        n.evaluate(&snap(10.0, at(12)), base_time());
        assert_eq!(n.evaluate(&snap(82.0, at(12)), later(1)).len(), 1);

        // 리셋(사용률 급락) 후 곧바로 다시 넘겨도 최소 간격 안에서는 침묵
        n.evaluate(&snap(0.0, at(17)), later(2));
        assert!(n.evaluate(&snap(82.0, at(17)), later(3)).is_empty(), "최소 간격 내");

        // 간격이 지나면 다시 울린다
        let alerts = n.evaluate(&snap(82.0, at(17)), later(MIN_REALERT_MINUTES + 5));
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn higher_threshold_fires_separately() {
        let mut n = Notifier::new(DEFAULT_THRESHOLDS.to_vec(), false);
        n.evaluate(&snap(10.0, at(12)), base_time());
        assert_eq!(n.evaluate(&snap(82.0, at(12)), later(1)).len(), 1); // 80

        let alerts = n.evaluate(&snap(96.0, at(12)), later(2));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 95.0);
    }

    #[test]
    fn reset_allows_firing_again_after_interval() {
        let mut n = Notifier::new(vec![80.0], false);
        n.evaluate(&snap(10.0, at(12)), base_time());
        assert_eq!(n.evaluate(&snap(82.0, at(12)), later(1)).len(), 1);

        // 사용률이 급락 = 리셋. 최소 간격이 지난 뒤 다시 넘기면 울려야 한다
        n.evaluate(&snap(2.0, at(17)), later(60));
        let alerts = n.evaluate(&snap(82.0, at(17)), later(61));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80.0);
    }

    #[test]
    fn reset_notification_is_opt_in() {
        let mut off = Notifier::new(vec![], false);
        off.evaluate(&snap(50.0, at(12)), base_time());
        assert!(
            off.evaluate(&snap(1.0, at(17)), later(1)).is_empty(),
            "기본은 꺼짐"
        );

        let mut on = Notifier::new(vec![], true);
        on.evaluate(&snap(50.0, at(12)), base_time());
        let alerts = on.evaluate(&snap(1.0, at(17)), later(1));
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
