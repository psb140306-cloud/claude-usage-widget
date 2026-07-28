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

/// FR-7 보존 기간
pub const RETENTION_DAYS: i64 = 90;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS snapshots (
    ts          INTEGER PRIMARY KEY,  -- epoch seconds (UTC)
    session_pct REAL,
    weekly_pct  REAL,
    opus_pct    REAL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_snapshots_ts ON snapshots(ts);
"#;

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

    /// 보존 기간을 넘긴 행을 지운다. 지운 행 수를 돌려준다.
    pub fn prune(&self, now: DateTime<Utc>) -> Result<usize, HistoryError> {
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
    fn empty_range_returns_empty() {
        let h = History::in_memory().unwrap();
        h.append(&snap(t(0), 1.0, 1.0)).unwrap();
        assert!(h.query(t(100), t(200)).unwrap().is_empty());
    }
}
