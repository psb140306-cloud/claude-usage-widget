//! FR-2 사용량 조회 클라이언트.
//!
//! 계약 전문은 `docs/api-schema.md`. 엔드포인트가 **비공식**이므로
//! 변경 시 이 모듈 하나만 고치면 되도록 상수와 파싱을 여기에 가둔다.

// M2 에서 poller 에 연결되면 제거할 것.
#![allow(dead_code)]

use chrono::Utc;

use crate::credentials::AccessToken;
use crate::model::{RawUsage, UsageSnapshot};

/// docs/api-schema.md §3
pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// 번들 상수 `K2e`. 값이 바뀌면 여기만 고친다.
pub const OAUTH_BETA: &str = "oauth-2025-04-20";
/// Claude Code 자체 타임아웃과 동일하게 맞춘다.
pub const TIMEOUT_SECS: u64 = 5;
/// FR-2: 5xx/타임아웃 시 지수 백오프 최대 3회
pub const MAX_RETRIES: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    /// HTTP 401 — 재인증 필요. 본 앱은 토큰을 갱신하지 않는다.
    #[error("재인증이 필요합니다. Claude Code를 한 번 실행해 주세요")]
    Auth,
    /// HTTP 429
    #[error("요청이 제한되었습니다 (429)")]
    RateLimited,
    /// 5xx / 타임아웃 / 연결 실패 — 재시도 대상
    #[error("네트워크 오류: {0}")]
    Network(String),
    /// 200 이지만 알려진 필드가 하나도 없음 (in-band error)
    #[error("응답 형식을 해석할 수 없습니다")]
    Schema,
}

impl UsageError {
    /// 지수 백오프로 재시도할 가치가 있는 오류인지.
    pub fn is_retryable(&self) -> bool {
        matches!(self, UsageError::Network(_))
    }
}

/// 토큰으로 usage 를 1회 조회한다 (재시도 없음 — 재시도는 poller 책임).
///
/// TODO(M2 2.2): reqwest 클라이언트를 재사용하도록 구조화하고,
/// 상태코드 → `UsageError` 매핑 + `RawUsage::normalize` 연결.
pub async fn fetch_usage(_token: &AccessToken) -> Result<UsageSnapshot, UsageError> {
    let _ = (USAGE_URL, OAUTH_BETA, TIMEOUT_SECS, Utc::now());
    let _ = RawUsage::default();
    todo!("M2 2.2 usage-client 구현")
}
