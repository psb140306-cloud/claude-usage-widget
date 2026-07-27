//! FR-1 자격증명 로드.
//!
//! Claude Code 가 저장한 OAuth 토큰을 **읽기 전용**으로 재사용한다.
//! 경로·포맷이 버전업으로 바뀔 수 있으므로 이 모듈만 교체하면 되도록 격리한다
//! (파일 스키마 근거: `docs/api-schema.md` §3.1).
//!
//! 보안 규칙: `AccessToken` 의 `Debug` 는 값을 가린다.
//! 토큰을 로그·설정·히스토리 어디에도 남기지 않기 위한 방어다.

// M2 에서 usage_client 에 연결되면 제거할 것.
#![allow(dead_code)]

use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

/// 토큰 래퍼. 값을 꺼내려면 명시적으로 [`AccessToken::expose`] 를 호출해야 한다.
#[derive(Clone)]
pub struct AccessToken(String);

impl AccessToken {
    /// usage 요청의 Authorization 헤더를 만들 때만 사용한다.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

// Debug 를 직접 구현해 실수로 토큰이 로그에 찍히는 것을 막는다.
impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub token: AccessToken,
    pub expires_at: DateTime<Utc>,
    pub subscription_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialsError {
    #[error("자격증명 파일을 찾을 수 없습니다: {0}")]
    NotFound(PathBuf),
    #[error("자격증명 파일을 읽을 수 없습니다: {0}")]
    Io(#[from] std::io::Error),
    #[error("자격증명 형식을 해석할 수 없습니다 (Claude Code 버전이 바뀌었을 수 있습니다)")]
    Parse,
    #[error("토큰이 만료되었습니다. Claude Code를 한 번 실행해 주세요")]
    Expired,
}

// ── 온디스크 스키마 ──────────────────────────────────────────────
// 우리가 쓰는 필드만 선언한다. mcpOAuth 등 나머지는 무시된다.

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OauthSection>,
}

#[derive(Deserialize)]
struct OauthSection {
    #[serde(rename = "accessToken")]
    access_token: String,
    /// ⚠️ epoch **밀리초** (초 아님)
    #[serde(rename = "expiresAt")]
    expires_at: i64,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

/// 기본 경로: `%USERPROFILE%\.claude\.credentials.json`
pub fn default_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude").join(".credentials.json"))
}

/// 자격증명을 읽는다. 파일은 열기만 하고 절대 쓰지 않는다.
pub fn load() -> Result<Credentials, CredentialsError> {
    let path = default_path().ok_or_else(|| CredentialsError::NotFound(PathBuf::from("~")))?;
    load_from(&path)
}

pub fn load_from(path: &PathBuf) -> Result<Credentials, CredentialsError> {
    if !path.exists() {
        return Err(CredentialsError::NotFound(path.clone()));
    }

    let text = std::fs::read_to_string(path)?;
    let file: CredentialsFile = serde_json::from_str(&text).map_err(|_| CredentialsError::Parse)?;
    let oauth = file.claude_ai_oauth.ok_or(CredentialsError::Parse)?;

    let expires_at = Utc
        .timestamp_millis_opt(oauth.expires_at)
        .single()
        .ok_or(CredentialsError::Parse)?;

    if expires_at <= Utc::now() {
        return Err(CredentialsError::Expired);
    }

    Ok(Credentials {
        token: AccessToken(oauth.access_token),
        expires_at,
        subscription_type: oauth.subscription_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_debug_is_redacted() {
        let t = AccessToken("super-secret-value".into());
        let rendered = format!("{t:?}");
        assert!(!rendered.contains("super-secret"));
        assert_eq!(rendered, "AccessToken(<redacted>)");
    }

    #[test]
    fn expires_at_is_milliseconds() {
        // 1785133196889ms = 2026-07-27T06:19:56Z. 초로 잘못 읽으면 서기 58000년쯤이 된다.
        let dt = Utc.timestamp_millis_opt(1_785_133_196_889).single().unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-07-27");
    }
}
