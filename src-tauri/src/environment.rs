//! Claude Code 환경 정보 수집 — 계정·플랜·현재 세션(모델/effort/thinking).
//!
//! usage 엔드포인트에는 없는 정보라 **로컬 파일**에서 읽는다.
//! PRD 3.3 의 "로컬 트랜스크립트 파싱 하이브리드"를 최소 범위로 구현한 것이다.
//!
//! | 항목 | 출처 |
//! |---|---|
//! | 계정(이름/이메일), 플랜 | `%USERPROFILE%\.claude.json` → `oauthAccount` |
//! | 모델 / effort / thinking | `%USERPROFILE%\.claude\projects\**\*.jsonl` 중 가장 최근 파일의 꼬리 |
//!
//! **개인정보 원칙**: 트랜스크립트에는 대화 내용이 들어 있다.
//! 이 모듈은 메타데이터 필드만 뽑고 대화 본문은 읽지도, 보관하지도, 내보내지도 않는다.
//!
//! 자격증명과 마찬가지로 경로·포맷이 바뀔 수 있으므로 어댑터로 격리한다.
//! 어떤 항목이든 못 읽으면 `None` 이며, 그 때문에 위젯이 죽지 않는다.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 트랜스크립트 꼬리에서 읽을 크기. 최근 몇십 개 메시지면 충분하다.
const TAIL_BYTES: u64 = 256 * 1024;

/// thinking 활성 판정에 쓸 최근 assistant 메시지 수.
const THINKING_LOOKBACK: usize = 20;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub display_name: Option<String>,
    pub email: Option<String>,
    /// 원본 값 (`max`, `pro` …)
    pub plan: Option<String>,
    /// 사람이 읽는 라벨 (`Max 20x`)
    pub plan_label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// 원본 모델 ID (`claude-opus-5`)
    pub model: Option<String>,
    /// 사람이 읽는 라벨 (`Opus 5`)
    pub model_label: Option<String>,
    /// `low` / `medium` / `high` / `xhigh` / `max`
    pub effort: Option<String>,
    /// 최근 응답에 thinking 블록이 있었는지
    pub thinking: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub account: Account,
    pub session: Session,
}

impl Environment {
    /// 읽어낸 게 하나도 없는지. 트랜스크립트가 잠깐 안 읽힐 때
    /// 위젯의 계정·모델 줄을 지우지 않기 위해 쓴다.
    pub fn is_empty(&self) -> bool {
        self.account.display_name.is_none()
            && self.account.email.is_none()
            && self.account.plan_label.is_none()
            && self.session.model.is_none()
            && self.session.effort.is_none()
    }
}

// ─────────────────────────── 온디스크 스키마 ───────────────────────────

#[derive(Deserialize)]
struct ClaudeJson {
    #[serde(rename = "oauthAccount")]
    oauth_account: Option<OauthAccount>,
}

#[derive(Deserialize)]
struct OauthAccount {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
    /// `claude_max` / `claude_pro` …
    #[serde(rename = "organizationType")]
    organization_type: Option<String>,
    /// `default_claude_max_20x`
    #[serde(rename = "organizationRateLimitTier")]
    organization_rate_limit_tier: Option<String>,
}

/// 트랜스크립트 한 줄. 필요한 필드만 선언하고 나머지는 무시한다.
#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    effort: Option<String>,
    message: Option<TranscriptMessage>,
}

#[derive(Deserialize)]
struct TranscriptMessage {
    model: Option<String>,
    /// 본문은 보지 않는다 — 블록의 `type` 만 본다.
    content: Option<Vec<ContentBlock>>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: Option<String>,
}

// ─────────────────────────── 수집 ───────────────────────────

pub fn home() -> Option<PathBuf> {
    let h = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(h))
}

/// 계정 + 세션 정보를 모은다. 실패한 항목은 `None` 으로 남는다.
pub fn load() -> Environment {
    Environment {
        account: load_account().unwrap_or_default(),
        session: load_session().unwrap_or_default(),
    }
}

fn load_account() -> Option<Account> {
    let path = home()?.join(".claude.json");
    let text = std::fs::read_to_string(path).ok()?;
    // 50KB 넘는 파일이지만 필요한 키 외에는 serde 가 건너뛴다
    let parsed: ClaudeJson = serde_json::from_str(&text).ok()?;
    let acc = parsed.oauth_account?;

    Some(Account {
        display_name: acc.display_name,
        email: acc.email_address,
        plan: acc
            .organization_type
            .as_deref()
            .map(|t| t.trim_start_matches("claude_").to_string()),
        plan_label: acc
            .organization_rate_limit_tier
            .as_deref()
            .map(plan_label_from_tier),
    })
}

fn load_session() -> Option<Session> {
    let path = latest_transcript()?;
    let text = read_tail(&path, TAIL_BYTES)?;
    Some(parse_session(&text))
}

/// 트랜스크립트를 찾을 때 내려갈 최대 깊이.
///
/// 실제 배치는 `projects/<프로젝트>/<세션>.jsonl` 이지만, Claude Code 가 하위
/// 디렉터리를 더 두는 경우(도구 결과 등)가 있어 여유를 준다. 깊이를 고정해
/// 심볼릭 링크 순환에 빠지지 않게 한다.
const MAX_SCAN_DEPTH: usize = 3;

/// `~/.claude/projects` 아래에서 가장 최근에 수정된 `.jsonl` 을 찾는다.
///
/// 여러 Claude Code 세션이 동시에 떠 있을 수 있으므로 "가장 최근 활동한 세션"을
/// 현재 세션으로 본다.
fn latest_transcript() -> Option<PathBuf> {
    let root = home()?.join(".claude").join("projects");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    scan_dir(&root, 0, &mut best);
    best.map(|(_, p)| p)
}

fn scan_dir(dir: &Path, depth: usize, best: &mut Option<(std::time::SystemTime, PathBuf)>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        // symlink 을 따라가지 않으려고 file_type() 을 쓴다 (metadata 는 따라간다)
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            scan_dir(&path, depth + 1, best);
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            *best = Some((modified, path));
        }
    }
}

/// 파일 끝에서 최대 `max` 바이트를 읽는다. 큰 트랜스크립트를 통째로 읽지 않기 위함.
///
/// 파일은 계속 append 되고 있으므로 metadata 와 read 사이에 길이가 바뀔 수 있다.
/// `take(max)` 로 실제 읽는 양도 함께 묶어, 그 사이 덧붙은 내용까지 전부 읽어
/// 메모리가 부풀지 않게 한다.
fn read_tail(path: &Path, max: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max);
    file.seek(SeekFrom::Start(start)).ok()?;

    let mut buf = Vec::new();
    file.take(max).read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        return None; // 파일이 그새 잘렸거나 로테이션됨
    }

    // 중간부터 잘랐으면 첫 줄이 깨져 있다. from_utf8_lossy 가 UTF-8 경계도 정리한다.
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        text.find('\n').map(|i| text[i + 1..].to_string())
    } else {
        Some(text)
    }
}

/// 트랜스크립트 꼬리에서 모델·effort·thinking 을 뽑는다.
///
/// 뒤에서부터 훑어 **가장 최근 값**을 취한다.
pub fn parse_session(tail: &str) -> Session {
    let mut session = Session::default();
    let mut assistant_seen = 0usize;

    for line in tail.lines().rev() {
        let Ok(parsed) = serde_json::from_str::<TranscriptLine>(line) else {
            continue;
        };

        if session.effort.is_none() {
            session.effort = parsed.effort;
        }

        let is_assistant = parsed.kind.as_deref() == Some("assistant");
        if !is_assistant {
            continue;
        }
        let Some(message) = parsed.message else {
            continue;
        };

        if session.model.is_none() {
            if let Some(model) = message.model {
                session.model_label = Some(model_label(&model));
                session.model = Some(model);
            }
        }

        // thinking 은 "최근 N개 응답 중 하나라도 thinking 블록이 있었는가"로 본다.
        // 도구 호출만 있는 응답에는 thinking 블록이 없을 수 있어 한 개만 보면 흔들린다.
        if assistant_seen < THINKING_LOOKBACK {
            assistant_seen += 1;
            if let Some(content) = message.content {
                if content.iter().any(|b| b.kind.as_deref() == Some("thinking")) {
                    session.thinking = true;
                }
            }
        }

        if session.model.is_some() && session.effort.is_some() && assistant_seen >= THINKING_LOOKBACK
        {
            break;
        }
    }

    session
}

/// `default_claude_max_20x` → `Max 20x`
pub fn plan_label_from_tier(tier: &str) -> String {
    let rest = tier
        .trim_start_matches("default_")
        .trim_start_matches("claude_");

    rest.split('_')
        .map(|part| {
            // 20x, 5x 같은 배수 표기는 그대로 두고, 단어만 첫 글자를 올린다
            if part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                part.to_string()
            } else {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `claude-opus-5` → `Opus 5`, `claude-haiku-4-5-20251001` → `Haiku 4.5`
pub fn model_label(model: &str) -> String {
    let stripped = model.trim_start_matches("claude-");
    let mut parts = stripped.split('-');

    let Some(family) = parts.next() else {
        return model.to_string();
    };
    let family = {
        let mut c = family.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => return model.to_string(),
        }
    };

    // 날짜 스탬프(8자리)는 버리고 버전 숫자만 점으로 잇는다
    let version: Vec<&str> = parts
        .filter(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() < 8)
        .collect();

    if version.is_empty() {
        family
    } else {
        format!("{family} {}", version.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_labels() {
        assert_eq!(plan_label_from_tier("default_claude_max_20x"), "Max 20x");
        assert_eq!(plan_label_from_tier("default_claude_max_5x"), "Max 5x");
        assert_eq!(plan_label_from_tier("default_claude_pro"), "Pro");
    }

    #[test]
    fn model_labels() {
        assert_eq!(model_label("claude-opus-5"), "Opus 5");
        assert_eq!(model_label("claude-sonnet-5"), "Sonnet 5");
        assert_eq!(model_label("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(model_label("claude-fable-5"), "Fable 5");
    }

    #[test]
    fn parses_model_effort_and_thinking() {
        // 실제 트랜스크립트 형태 (본문은 읽지 않으므로 빈 값으로 둔다)
        let tail = concat!(
            r#"{"type":"user","message":{"role":"user"}}"#,
            "\n",
            r#"{"type":"assistant","effort":"xhigh","message":{"model":"claude-opus-5","content":[{"type":"thinking"},{"type":"text"}]}}"#,
            "\n"
        );

        let s = parse_session(tail);
        assert_eq!(s.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(s.model_label.as_deref(), Some("Opus 5"));
        assert_eq!(s.effort.as_deref(), Some("xhigh"));
        assert!(s.thinking);
    }

    #[test]
    fn thinking_false_when_absent() {
        let tail = r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[{"type":"text"}]}}"#;
        let s = parse_session(tail);
        assert_eq!(s.model_label.as_deref(), Some("Sonnet 5"));
        assert!(!s.thinking);
    }

    /// 꼬리를 중간부터 잘라 첫 줄이 깨져도 나머지는 살아야 한다.
    #[test]
    fn broken_first_line_is_skipped() {
        let tail = concat!(
            r#"del":"claude-opus-5"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[]}}"#,
        );
        assert_eq!(parse_session(tail).model_label.as_deref(), Some("Sonnet 5"));
    }

    #[test]
    fn empty_tail_yields_defaults() {
        let s = parse_session("");
        assert!(s.model.is_none());
        assert!(s.effort.is_none());
        assert!(!s.thinking);
    }
}
