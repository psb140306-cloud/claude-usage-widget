//! 공용 타입. 프론트의 `src/lib/types.ts` 와 1:1 대응한다 (camelCase 직렬화).
//! 원시 응답 스키마의 근거는 `docs/api-schema.md` §4.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─────────────────────────── usage API 원시 응답 ───────────────────────────
//
// FR-2 방어적 파싱: 모든 필드가 Option 이고, 알 수 없는 필드는 serde 가 무시한다.
// 서버가 `tangelo`, `nimbus_quill` 같은 실험적 버킷을 계속 추가하므로
// 명명 필드를 전부 따라가지 않고 필요한 것만 선언한다.

#[derive(Debug, Clone, Deserialize)]
pub struct RawBucket {
    /// 0~100 스케일. ⚠️ 추론 응답 헤더의 0~1 과 다르다 — 100 을 곱하지 말 것.
    #[serde(default)]
    pub utilization: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawModel {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawScope {
    #[serde(default)]
    pub model: Option<RawModel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawLimit {
    /// `session` / `weekly_all` / `weekly_scoped` (확장 가능)
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub percent: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub scope: Option<RawScope>,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawUsage {
    #[serde(default)]
    pub five_hour: Option<RawBucket>,
    #[serde(default)]
    pub seven_day: Option<RawBucket>,
    #[serde(default)]
    pub seven_day_opus: Option<RawBucket>,
    /// 통합 목록. 새 한도 유형이 생기면 명명 필드보다 여기에 먼저 나타난다.
    #[serde(default)]
    pub limits: Option<Vec<RawLimit>>,
}

// ─────────────────────────── 정규화된 모델 ───────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelWindow {
    pub display_name: String,
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub fetched_at: DateTime<Utc>,
    pub session: Option<LimitWindow>,
    pub weekly: Option<LimitWindow>,
    pub weekly_opus: Option<LimitWindow>,
    pub model_scoped: Vec<ModelWindow>,
}

/// 위젯이 렌더링하는 상태 (PRD 5.1 "상태 표현" 4종 + 최초 로딩).
/// `{ "kind": "ok", "snapshot": {...} }` 형태로 직렬화된다.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppState {
    Loading,
    Ok { snapshot: UsageSnapshot },
    Stale { snapshot: UsageSnapshot, reason: String },
    NeedsReauth,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub session_pct: Option<f64>,
    pub weekly_pct: Option<f64>,
    pub opus_pct: Option<f64>,
}

// ─────────────────────────── 변환 ───────────────────────────

fn parse_ts(s: &Option<String>) -> Option<DateTime<Utc>> {
    let raw = s.as_ref()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

impl RawBucket {
    fn to_window(&self) -> Option<LimitWindow> {
        Some(LimitWindow {
            utilization: self.utilization?,
            resets_at: parse_ts(&self.resets_at),
        })
    }
}

impl RawUsage {
    /// 원시 응답 → 스냅샷.
    ///
    /// 세션·주간이 **둘 다** 없으면 표시할 게 없으므로 `None` 을 돌려주고,
    /// 호출부가 `AppState::Unavailable` 로 강등한다 (docs/api-schema.md §5).
    pub fn normalize(&self, fetched_at: DateTime<Utc>) -> Option<UsageSnapshot> {
        let session = self.five_hour.as_ref().and_then(RawBucket::to_window);
        let weekly = self.seven_day.as_ref().and_then(RawBucket::to_window);
        if session.is_none() && weekly.is_none() {
            return None;
        }

        let model_scoped = self
            .limits
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter(|l| l.kind.as_deref() == Some("weekly_scoped"))
            .filter_map(|l| {
                Some(ModelWindow {
                    display_name: l.scope.as_ref()?.model.as_ref()?.display_name.clone()?,
                    utilization: l.percent?,
                    resets_at: parse_ts(&l.resets_at),
                    is_active: l.is_active,
                })
            })
            .collect();

        Some(UsageSnapshot {
            fetched_at,
            session,
            weekly,
            weekly_opus: self.seven_day_opus.as_ref().and_then(RawBucket::to_window),
            model_scoped,
        })
    }
}

impl UsageSnapshot {
    /// 트레이 아이콘 색상 기준: 세션/주간 중 최고값.
    pub fn peak_utilization(&self) -> Option<f64> {
        [
            self.session.as_ref().map(|w| w.utilization),
            self.weekly.as_ref().map(|w| w.utilization),
        ]
        .into_iter()
        .flatten()
        .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.max(v))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// docs/api-schema.md §4 의 실제 응답 예시로 파싱을 고정한다.
    const SAMPLE: &str = r#"{
        "five_hour": {"utilization": 17.0, "resets_at": "2026-07-27T03:09:59.869309+00:00"},
        "seven_day": {"utilization": 21.0, "resets_at": "2026-07-30T09:59:59.869329+00:00"},
        "seven_day_opus": null,
        "tangelo": null,
        "limits": [
            {"kind": "session", "percent": 17, "resets_at": "2026-07-27T03:09:59.869309+00:00",
             "scope": null, "is_active": false},
            {"kind": "weekly_scoped", "percent": 29, "resets_at": "2026-07-30T09:59:59.869624+00:00",
             "scope": {"model": {"id": null, "display_name": "Fable"}}, "is_active": true}
        ],
        "member_dashboard_available": false
    }"#;

    #[test]
    fn parses_sample_response() {
        let raw: RawUsage = serde_json::from_str(SAMPLE).expect("알 수 없는 필드는 무시되어야 한다");
        let snap = raw.normalize(Utc::now()).expect("세션/주간이 있으면 Some");

        assert_eq!(snap.session.as_ref().unwrap().utilization, 17.0);
        assert_eq!(snap.weekly.as_ref().unwrap().utilization, 21.0);
        assert!(snap.weekly_opus.is_none());
        assert_eq!(snap.peak_utilization(), Some(21.0));

        assert_eq!(snap.model_scoped.len(), 1);
        assert_eq!(snap.model_scoped[0].display_name, "Fable");
        assert!(snap.model_scoped[0].is_active);
    }

    #[test]
    fn empty_body_degrades_to_none() {
        let raw: RawUsage = serde_json::from_str("{}").unwrap();
        assert!(raw.normalize(Utc::now()).is_none());
    }
}
