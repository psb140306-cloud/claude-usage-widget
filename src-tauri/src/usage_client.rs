//! FR-2 사용량 조회 클라이언트.
//!
//! 계약 전문은 `docs/api-schema.md`. 엔드포인트가 **비공식**이므로
//! 변경 시 이 모듈 하나만 고치면 되도록 상수와 파싱을 여기에 가둔다.

use std::time::Duration;

use chrono::Utc;
use reqwest::{Client, StatusCode};

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
/// 첫 재시도까지의 대기. 이후 2배씩 증가한다.
pub const BACKOFF_BASE_MS: u64 = 500;

#[derive(Debug, Clone, thiserror::Error)]
pub enum UsageError {
    /// HTTP 401/403 — 재인증 필요. 본 앱은 토큰을 갱신하지 않는다.
    #[error("재인증이 필요합니다. Claude Code를 한 번 실행해 주세요")]
    Auth,
    /// HTTP 429
    #[error("요청이 제한되었습니다 (429)")]
    RateLimited,
    /// 5xx / 타임아웃 / 연결 실패 — 재시도 대상
    #[error("네트워크 오류: {0}")]
    Network(String),
    /// 200 이지만 알려진 필드가 하나도 없음 (in-band error) 또는 JSON 파싱 실패
    #[error("응답 형식을 해석할 수 없습니다")]
    Schema,
}

impl UsageError {
    /// 지수 백오프로 재시도할 가치가 있는 오류인지.
    ///
    /// 429 는 제외한다 — 제한이 걸린 상태에서 즉시 다시 두드리면 상황을 악화시킨다.
    pub fn is_retryable(&self) -> bool {
        matches!(self, UsageError::Network(_))
    }
}

/// 재사용할 HTTP 클라이언트. 매 폴링마다 새로 만들면 커넥션 풀이 무의미해진다.
pub fn build_client() -> reqwest::Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .user_agent(concat!("claude-usage-widget/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// n번째 재시도(1-based) 전에 기다릴 시간. 500ms → 1s → 2s …
pub fn backoff_delay(attempt: u32) -> Duration {
    Duration::from_millis(BACKOFF_BASE_MS * 2u64.pow(attempt.saturating_sub(1)))
}

/// usage 를 1회 조회한다 (재시도 없음).
pub async fn fetch_usage(client: &Client, token: &AccessToken) -> Result<UsageSnapshot, UsageError> {
    let resp = client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {}", token.expose()))
        .header("anthropic-beta", OAUTH_BETA)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(classify_transport)?;

    // 상태코드를 **본문보다 먼저** 판정한다.
    // 본문을 먼저 읽으면 401 응답의 본문 수신이 끊겼을 때 Auth 가 아니라
    // 재시도 대상인 Network 로 잘못 분류되어, 재인증 안내 대신 헛된 재시도를 한다.
    classify_status(resp.status())?;

    let body = resp
        .text()
        .await
        .map_err(|e| UsageError::Network(format!("본문 읽기 실패: {e}")))?;

    parse_body(&body)
}

/// 지수 백오프 재시도를 감싼 조회 (FR-2: 최대 3회).
pub async fn fetch_with_retry(
    client: &Client,
    token: &AccessToken,
) -> Result<UsageSnapshot, UsageError> {
    let mut attempt = 1;
    loop {
        match fetch_usage(client, token).await {
            Ok(snapshot) => return Ok(snapshot),
            Err(e) if e.is_retryable() && attempt < MAX_RETRIES => {
                tokio::time::sleep(backoff_delay(attempt)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// 상태코드 → 오류 분류 (docs/api-schema.md §5 표).
fn classify_status(status: StatusCode) -> Result<(), UsageError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(UsageError::Auth),
        StatusCode::TOO_MANY_REQUESTS => Err(UsageError::RateLimited),
        s if s.is_server_error() => Err(UsageError::Network(format!("HTTP {}", s.as_u16()))),
        s if !s.is_success() => Err(UsageError::Schema),
        _ => Ok(()),
    }
}

fn classify_transport(e: reqwest::Error) -> UsageError {
    if e.is_timeout() {
        UsageError::Network(format!("타임아웃 ({TIMEOUT_SECS}초)"))
    } else if e.is_connect() {
        UsageError::Network("연결 실패".into())
    } else {
        UsageError::Network(e.to_string())
    }
}

/// 응답 본문 → 스냅샷. 네트워크와 분리해 두어 테스트 가능하게 한다.
///
/// 알려진 필드가 하나도 없으면 `Schema` 로 강등한다 — Claude Code 가
/// "in-band error" 로 처리하는 것과 같은 기준이다.
pub fn parse_body(body: &str) -> Result<UsageSnapshot, UsageError> {
    let raw: RawUsage = serde_json::from_str(body).map_err(|_| UsageError::Schema)?;
    raw.normalize(Utc::now()).ok_or(UsageError::Schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "five_hour": {"utilization": 17.0, "resets_at": "2026-07-27T03:09:59.869309+00:00"},
        "seven_day": {"utilization": 21.0, "resets_at": "2026-07-30T09:59:59.869329+00:00"},
        "seven_day_opus": null,
        "limits": []
    }"#;

    #[test]
    fn parses_real_response() {
        let snap = parse_body(SAMPLE).expect("정상 응답");
        assert_eq!(snap.session.unwrap().utilization, 17.0);
        assert_eq!(snap.weekly.unwrap().utilization, 21.0);
    }

    #[test]
    fn fieldless_body_is_schema_error() {
        // 200 이지만 알려진 필드가 없는 경우 (in-band error)
        assert!(matches!(parse_body("{}"), Err(UsageError::Schema)));
        assert!(matches!(
            parse_body(r#"{"error":{"type":"rate_limit_error"}}"#),
            Err(UsageError::Schema)
        ));
    }

    #[test]
    fn malformed_json_is_schema_error() {
        assert!(matches!(parse_body("not json"), Err(UsageError::Schema)));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // 서버가 실험 버킷을 늘려도 파싱이 깨지면 안 된다
        let body = r#"{"five_hour":{"utilization":5.0,"resets_at":null},
                       "tangelo":{"weird":1},"nimbus_quill":[1,2,3],"brand_new_field":"?"}"#;
        let snap = parse_body(body).expect("미지 필드는 무시되어야 한다");
        assert_eq!(snap.session.unwrap().utilization, 5.0);
    }

    #[test]
    fn status_mapping_follows_api_schema_table() {
        use StatusCode as S;
        assert!(matches!(classify_status(S::OK), Ok(())));
        assert!(matches!(classify_status(S::UNAUTHORIZED), Err(UsageError::Auth)));
        assert!(matches!(classify_status(S::FORBIDDEN), Err(UsageError::Auth)));
        assert!(matches!(
            classify_status(S::TOO_MANY_REQUESTS),
            Err(UsageError::RateLimited)
        ));
        assert!(matches!(
            classify_status(S::INTERNAL_SERVER_ERROR),
            Err(UsageError::Network(_))
        ));
        assert!(matches!(classify_status(S::BAD_REQUEST), Err(UsageError::Schema)));
    }

    #[test]
    fn only_network_errors_retry() {
        assert!(UsageError::Network("x".into()).is_retryable());
        assert!(!UsageError::Auth.is_retryable());
        assert!(!UsageError::Schema.is_retryable());
        // 429 에 즉시 재시도하면 상황이 악화된다
        assert!(!UsageError::RateLimited.is_retryable());
    }

    #[test]
    fn backoff_doubles() {
        assert_eq!(backoff_delay(1), Duration::from_millis(500));
        assert_eq!(backoff_delay(2), Duration::from_millis(1000));
        assert_eq!(backoff_delay(3), Duration::from_millis(2000));
    }
}
