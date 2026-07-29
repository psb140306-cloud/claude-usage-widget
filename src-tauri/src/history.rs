//! FR-7 히스토리 저장소 (SQLite / rusqlite bundled).
//!
//! 90일 × 60초 폴링 = 최대 약 13만 행. prune·24시간 차트·요일별 집계를
//! 전부 단일 쿼리로 처리하려고 JSONL 대신 SQLite 를 택했다 (/bootstrap 결정).
//!
//! 시각은 epoch **초**로 저장한다. 밀리초까지 필요한 정밀도가 아니고,
//! 초 단위면 `ts` 를 그대로 PRIMARY KEY 로 써서 같은 초의 중복 저장이 막힌다.

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection};

use crate::model::{HistoryEntry, UsageSnapshot};

/// FR-7 보존 기간 (원본 스냅샷). 하루 요약(`daily_stats`)은 지우지 않는다 —
/// 1년이 쌓여도 365행이라 부담이 없고, 덕분에 리포트가 원본 보존 기간을 넘어 산다.
pub const RETENTION_DAYS: i64 = 90;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS snapshots (
    ts          INTEGER PRIMARY KEY,  -- epoch seconds (UTC)
    session_pct REAL,
    weekly_pct  REAL,
    opus_pct    REAL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_snapshots_ts ON snapshots(ts);

CREATE TABLE IF NOT EXISTS daily_stats (
    day          TEXT PRIMARY KEY,    -- 로컬 날짜 'YYYY-MM-DD'
    peak_session REAL,
    peak_weekly  REAL,
    peak_opus    REAL,
    samples      INTEGER NOT NULL
) STRICT;
"#;

/// 하루 요약 한 줄 (기간 리포트용). `day` 는 로컬 날짜 `YYYY-MM-DD`.
///
/// 사용률은 한도 대비 %라 합산이 무의미하므로 "그 날 최고치"를 요약으로 쓴다
/// (요일 패턴과 같은 논리 — [`History::weekday_stats`] 참고).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStat {
    pub day: String,
    pub peak_session: Option<f64>,
    pub peak_weekly: Option<f64>,
    pub peak_opus: Option<f64>,
    /// 그 날 저장된 스냅샷 수 — 위젯이 꺼져 있던 날(0)과 구분한다
    pub samples: u32,
}

/// 요일별 집계 한 줄. `weekday` 는 SQLite `strftime('%w')` 규약대로 0=일요일.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekdayStat {
    pub weekday: u8,
    /// 그 요일 "하루 최고 세션 사용률"의 평균
    pub avg_peak_session: Option<f64>,
    pub avg_peak_weekly: Option<f64>,
    /// 집계에 들어간 날 수 — 표본이 적으면 UI 에서 알려줘야 한다
    pub days: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("히스토리 DB 오류: {0}")]
    Db(#[from] rusqlite::Error),
}

pub struct History {
    conn: Connection,
}

impl History {
    pub fn open(path: &std::path::Path) -> Result<Self, HistoryError> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, HistoryError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, HistoryError> {
        // WAL: 폴링 쓰기와 차트 읽기가 겹쳐도 서로 막지 않는다
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// 스냅샷 1건 저장. 같은 초에 두 번 들어오면 나중 값으로 덮는다.
    pub fn append(&self, snapshot: &UsageSnapshot) -> Result<(), HistoryError> {
        self.conn.execute(
            "INSERT INTO snapshots (ts, session_pct, weekly_pct, opus_pct)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(ts) DO UPDATE SET
                session_pct = excluded.session_pct,
                weekly_pct  = excluded.weekly_pct,
                opus_pct    = excluded.opus_pct",
            params![
                snapshot.fetched_at.timestamp(),
                snapshot.session.as_ref().map(|w| w.utilization),
                snapshot.weekly.as_ref().map(|w| w.utilization),
                snapshot.weekly_opus.as_ref().map(|w| w.utilization),
            ],
        )?;
        Ok(())
    }

    /// 기간 조회 (양끝 포함), 오래된 것부터.
    pub fn query(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<HistoryEntry>, HistoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, session_pct, weekly_pct, opus_pct
             FROM snapshots
             WHERE ts BETWEEN ?1 AND ?2
             ORDER BY ts",
        )?;

        let rows = stmt.query_map(params![from.timestamp(), to.timestamp()], |row| {
            let ts: i64 = row.get(0)?;
            Ok(HistoryEntry {
                timestamp: Utc.timestamp_opt(ts, 0).single().unwrap_or_default(),
                session_pct: row.get(1)?,
                weekly_pct: row.get(2)?,
                opus_pct: row.get(3)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 요일별 사용 패턴 (FR-7).
    ///
    /// 각 요일의 **최고 세션 사용률 평균**을 낸다. 단순 평균을 쓰면 새벽처럼
    /// 안 쓰는 시간대가 값을 눌러 요일 간 차이가 뭉개진다. "그 날 얼마나
    /// 많이 썼나"를 보려면 하루 최고치를 쓰는 편이 맞다.
    ///
    /// 시각은 **로컬 시간** 기준으로 요일을 가른다 — 사용자가 체감하는 요일이어야 한다.
    pub fn weekday_stats(&self, since: DateTime<Utc>) -> Result<Vec<WeekdayStat>, HistoryError> {
        let mut stmt = self.conn.prepare(
            "WITH daily AS (
                 SELECT date(ts, 'unixepoch', 'localtime')            AS day,
                        CAST(strftime('%w', ts, 'unixepoch', 'localtime') AS INTEGER) AS weekday,
                        MAX(session_pct) AS peak_session,
                        MAX(weekly_pct)  AS peak_weekly
                 FROM snapshots
                 WHERE ts >= ?1
                 GROUP BY day
             )
             SELECT weekday, AVG(peak_session), AVG(peak_weekly), COUNT(*)
             FROM daily
             GROUP BY weekday
             ORDER BY weekday",
        )?;

        let rows = stmt.query_map([since.timestamp()], |row| {
            Ok(WeekdayStat {
                weekday: row.get(0)?,
                avg_peak_session: row.get(1)?,
                avg_peak_weekly: row.get(2)?,
                days: row.get(3)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 원본 스냅샷을 하루 요약(`daily_stats`)으로 접는다. 몇 번을 불러도 같다.
    ///
    /// prune 이 하루의 앞부분만 지운 날은 원본만 다시 집계하면 요약이 **줄어든다**.
    /// 그래서 표본 수가 기존 요약 이상일 때만 덮어쓴다 — "더 많은 표본으로 계산된
    /// 쪽이 이긴다"는 불변식으로 부분 삭제에도 요약이 퇴화하지 않는다.
    pub fn rollup(&self) -> Result<(), HistoryError> {
        self.conn.execute(
            "INSERT INTO daily_stats (day, peak_session, peak_weekly, peak_opus, samples)
             SELECT date(ts, 'unixepoch', 'localtime'),
                    MAX(session_pct), MAX(weekly_pct), MAX(opus_pct), COUNT(*)
             FROM snapshots
             GROUP BY 1
             ON CONFLICT(day) DO UPDATE SET
                peak_session = excluded.peak_session,
                peak_weekly  = excluded.peak_weekly,
                peak_opus    = excluded.peak_opus,
                samples      = excluded.samples
             WHERE excluded.samples >= daily_stats.samples",
            [],
        )?;
        Ok(())
    }

    /// `since_day`("YYYY-MM-DD", 포함) 이후의 하루 요약, 오래된 날부터.
    ///
    /// 조회 전에 [`Self::rollup`] 을 돌려 오늘 몫까지 요약에 반영한다.
    pub fn daily_report(&self, since_day: &str) -> Result<Vec<DailyStat>, HistoryError> {
        self.rollup()?;

        let mut stmt = self.conn.prepare(
            "SELECT day, peak_session, peak_weekly, peak_opus, samples
             FROM daily_stats
             WHERE day >= ?1
             ORDER BY day",
        )?;

        let rows = stmt.query_map([since_day], |row| {
            Ok(DailyStat {
                day: row.get(0)?,
                peak_session: row.get(1)?,
                peak_weekly: row.get(2)?,
                peak_opus: row.get(3)?,
                samples: row.get(4)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 보존 기간을 넘긴 행을 지운다. 지운 행 수를 돌려준다.
    ///
    /// 지우기 전에 하루 요약을 먼저 남긴다 — 원본은 90일이지만 리포트는 요약으로 산다.
    pub fn prune(&self, now: DateTime<Utc>) -> Result<usize, HistoryError> {
        self.rollup()?;
        let cutoff = now - chrono::Duration::days(RETENTION_DAYS);
        let n = self
            .conn
            .execute("DELETE FROM snapshots WHERE ts < ?1", params![cutoff.timestamp()])?;
        Ok(n)
    }

    #[cfg(test)]
    fn count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LimitWindow;

    fn snap(at: DateTime<Utc>, session: f64, weekly: f64) -> UsageSnapshot {
        UsageSnapshot {
            fetched_at: at,
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

    fn t(offset_min: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_785_000_000 + offset_min * 60, 0).unwrap()
    }

    #[test]
    fn append_and_query_roundtrip() {
        let h = History::in_memory().unwrap();
        h.append(&snap(t(0), 10.0, 20.0)).unwrap();
        h.append(&snap(t(1), 12.0, 21.0)).unwrap();

        let rows = h.query(t(0), t(10)).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].session_pct, Some(10.0));
        assert_eq!(rows[1].weekly_pct, Some(21.0));
        assert!(rows[0].opus_pct.is_none());
        assert!(rows[0].timestamp < rows[1].timestamp, "오래된 것부터");
    }

    #[test]
    fn same_second_updates_instead_of_duplicating() {
        let h = History::in_memory().unwrap();
        h.append(&snap(t(0), 10.0, 20.0)).unwrap();
        h.append(&snap(t(0), 99.0, 88.0)).unwrap();

        assert_eq!(h.count(), 1, "같은 초는 한 행");
        assert_eq!(h.query(t(0), t(0)).unwrap()[0].session_pct, Some(99.0));
    }

    #[test]
    fn query_range_is_inclusive_and_bounded() {
        let h = History::in_memory().unwrap();
        for i in 0..5 {
            h.append(&snap(t(i), i as f64, 0.0)).unwrap();
        }
        let rows = h.query(t(1), t(3)).unwrap();
        assert_eq!(rows.len(), 3, "양끝 포함");
        assert_eq!(rows[0].session_pct, Some(1.0));
        assert_eq!(rows[2].session_pct, Some(3.0));
    }

    #[test]
    fn prune_drops_only_expired_rows() {
        let h = History::in_memory().unwrap();
        let now = t(0);
        let old = now - chrono::Duration::days(RETENTION_DAYS + 1);
        let fresh = now - chrono::Duration::days(1);

        h.append(&snap(old, 1.0, 1.0)).unwrap();
        h.append(&snap(fresh, 2.0, 2.0)).unwrap();
        assert_eq!(h.count(), 2);

        assert_eq!(h.prune(now).unwrap(), 1);
        assert_eq!(h.count(), 1);
        assert_eq!(h.query(fresh, now).unwrap()[0].session_pct, Some(2.0));
    }

    #[test]
    fn weekday_stats_uses_daily_peak_not_raw_average() {
        let h = History::in_memory().unwrap();
        // 같은 날에 낮은 값 여러 개 + 높은 값 하나.
        // 단순 평균이면 낮은 값들에 눌리고, 하루 최고치를 쓰면 90 이 나온다.
        let day = Utc.timestamp_opt(1_785_000_000, 0).unwrap();
        for i in 0..5 {
            h.append(&snap(day + chrono::Duration::minutes(i), 5.0, 1.0)).unwrap();
        }
        h.append(&snap(day + chrono::Duration::minutes(10), 90.0, 2.0)).unwrap();

        let stats = h.weekday_stats(day - chrono::Duration::days(1)).unwrap();
        assert_eq!(stats.len(), 1, "하루치이므로 요일 하나");
        assert_eq!(stats[0].avg_peak_session, Some(90.0));
        assert_eq!(stats[0].days, 1);
    }

    #[test]
    fn weekday_stats_averages_across_days() {
        let h = History::in_memory().unwrap();
        let day = Utc.timestamp_opt(1_785_000_000, 0).unwrap();
        // 정확히 7일 간격 = 같은 요일. 최고치 20 과 40 → 평균 30
        h.append(&snap(day, 20.0, 0.0)).unwrap();
        h.append(&snap(day + chrono::Duration::days(7), 40.0, 0.0)).unwrap();

        let stats = h.weekday_stats(day - chrono::Duration::days(1)).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].avg_peak_session, Some(30.0));
        assert_eq!(stats[0].days, 2);
    }

    #[test]
    fn weekday_stats_empty_when_no_data() {
        let h = History::in_memory().unwrap();
        assert!(h.weekday_stats(Utc::now()).unwrap().is_empty());
    }

    #[test]
    fn empty_range_returns_empty() {
        let h = History::in_memory().unwrap();
        h.append(&snap(t(0), 1.0, 1.0)).unwrap();
        assert!(h.query(t(100), t(200)).unwrap().is_empty());
    }

    /// ts 의 로컬 날짜 문자열 — SQLite `date(ts,'unixepoch','localtime')` 과 같은 값.
    /// 테스트를 타임존에 못 박지 않기 위해 기대값도 같은 변환으로 만든다.
    fn local_day(at: DateTime<Utc>) -> String {
        chrono::DateTime::<chrono::Local>::from(at).date_naive().to_string()
    }

    #[test]
    fn rollup_folds_snapshots_into_daily_peak() {
        let h = History::in_memory().unwrap();
        h.append(&snap(t(0), 5.0, 1.0)).unwrap();
        h.append(&snap(t(1), 90.0, 2.0)).unwrap();
        h.append(&snap(t(2), 10.0, 3.0)).unwrap();

        let report = h.daily_report("1970-01-01").unwrap();
        assert_eq!(report.len(), 1, "같은 날은 한 줄로 접힌다");
        assert_eq!(report[0].day, local_day(t(0)));
        assert_eq!(report[0].peak_session, Some(90.0));
        assert_eq!(report[0].peak_weekly, Some(3.0));
        assert_eq!(report[0].samples, 3);
    }

    /// 원본이 보존 기간에서 잘려도 리포트(요약)는 살아남아야 한다.
    #[test]
    fn daily_report_survives_prune() {
        let h = History::in_memory().unwrap();
        let now = t(0);
        let old = now - chrono::Duration::days(RETENTION_DAYS + 30);
        h.append(&snap(old, 77.0, 5.0)).unwrap();
        h.append(&snap(now, 20.0, 1.0)).unwrap();

        h.prune(now).unwrap();
        assert!(h.query(old, old).unwrap().is_empty(), "원본은 지워졌다");

        let report = h.daily_report("1970-01-01").unwrap();
        assert_eq!(report.len(), 2, "요약은 남는다");
        assert_eq!(report[0].day, local_day(old));
        assert_eq!(report[0].peak_session, Some(77.0));
    }

    /// 하루의 앞부분만 지워진 뒤의 재롤업이 요약을 퇴화시키면 안 된다.
    #[test]
    fn partial_delete_keeps_fuller_rollup() {
        let h = History::in_memory().unwrap();
        h.append(&snap(t(0), 95.0, 1.0)).unwrap();
        h.append(&snap(t(1), 10.0, 1.0)).unwrap();
        h.rollup().unwrap();

        // 최고치가 있던 행만 지워진 상황 (prune 이 하루 중간을 자른 경우)
        h.conn
            .execute("DELETE FROM snapshots WHERE ts = ?1", params![t(0).timestamp()])
            .unwrap();
        h.rollup().unwrap();

        let report = h.daily_report("1970-01-01").unwrap();
        assert_eq!(report[0].peak_session, Some(95.0), "표본이 준 재집계는 덮어쓰지 못한다");
        assert_eq!(report[0].samples, 2);
    }

    #[test]
    fn daily_report_filters_by_since_day() {
        let h = History::in_memory().unwrap();
        let d1 = t(0);
        let d2 = t(0) + chrono::Duration::days(3);
        h.append(&snap(d1, 10.0, 1.0)).unwrap();
        h.append(&snap(d2, 20.0, 2.0)).unwrap();

        let report = h.daily_report(&local_day(d2)).unwrap();
        assert_eq!(report.len(), 1, "since_day 이전은 제외");
        assert_eq!(report[0].day, local_day(d2));
        assert_eq!(report[0].peak_session, Some(20.0));
    }
}
