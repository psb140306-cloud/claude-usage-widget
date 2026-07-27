//! FR-3 폴링 서비스.
//!
//! 자격증명 로드 → usage 조회 → `AppState` 전이 → 프론트/트레이/히스토리로 브로드캐스트.
//! 오류는 크래시가 아니라 상태 강등으로 흡수한다 (PRD 6 안정성).

// M2 에서 폴링 루프가 붙으면 제거할 것.
#![allow(dead_code)]

use crate::model::AppState;

/// 프론트 이벤트 이름. `src/lib/types.ts` 의 `EVENT.state` 와 일치해야 한다.
pub const EVENT_STATE: &str = "usage://state";

/// FR-3: 기본 60초, 하한 30초
pub const DEFAULT_INTERVAL_SECS: u64 = 60;
pub const MIN_INTERVAL_SECS: u64 = 30;
/// FR-3: 수동 새로고침 연타 방지
pub const REFRESH_THROTTLE_SECS: u64 = 5;

/// 마지막 상태를 보관하고 구독자에게 전파한다.
///
/// TODO(M2 2.2): tokio interval 루프, 절전 복귀 감지, 수동 새로고침 스로틀,
/// history/notifier 연결.
pub struct Poller {
    state: AppState,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            state: AppState::Loading,
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl Default for Poller {
    fn default() -> Self {
        Self::new()
    }
}
