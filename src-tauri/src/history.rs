//! FR-7 히스토리 저장소 (SQLite / rusqlite bundled).
//!
//! 90일 × 60초 폴링 = 최대 약 13만 행. prune·24시간 차트·요일별 집계를
//! 전부 단일 쿼리로 처리하려고 JSONL 대신 SQLite 를 택했다 (/bootstrap 결정).

// M5 에서 구현되면 제거할 것.
#![allow(dead_code)]

use chrono::{DateTime, Utc};

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
    _conn: rusqlite::Connection,
}

impl History {
    /// TODO(M5 5.1): app_data_dir 아래에 열고 SCHEMA 적용.
    pub fn open(path: &std::path::Path) -> Result<Self, HistoryError> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { _conn: conn })
    }

    /// TODO(M5 5.1)
    pub fn append(&self, _snapshot: &UsageSnapshot) -> Result<(), HistoryError> {
        todo!("M5 5.1 히스토리 append")
    }

    /// TODO(M5 5.1)
    pub fn query(
        &self,
        _from: DateTime<Utc>,
        _to: DateTime<Utc>,
    ) -> Result<Vec<HistoryEntry>, HistoryError> {
        todo!("M5 5.1 히스토리 query")
    }

    /// TODO(M5 5.1): RETENTION_DAYS 초과분 삭제
    pub fn prune(&self) -> Result<usize, HistoryError> {
        todo!("M5 5.1 히스토리 prune")
    }
}
