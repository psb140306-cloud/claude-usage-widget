//! FR-3 폴링 서비스.
//!
//! 자격증명 로드 → usage 조회 → `AppState` 전이 → 프론트로 브로드캐스트.
//! 오류는 크래시가 아니라 상태 강등으로 흡수한다 (PRD 6 안정성).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::Client;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::credentials::{self, CredentialsError};
use crate::environment::{self, Environment};
use crate::model::{AppState, UsageSnapshot};
use crate::usage_client::{self, UsageError};

/// 프론트 이벤트 이름. `src/lib/types.ts` 의 `EVENT` 와 일치해야 한다.
pub const EVENT_STATE: &str = "usage://state";
/// 계정·플랜·현재 세션(모델/effort/thinking)
pub const EVENT_ENV: &str = "usage://env";

/// FR-3: 기본 60초, 하한 30초 / FR-8: 설정 범위 30초~10분
pub const DEFAULT_INTERVAL_SECS: u64 = 60;
pub const MIN_INTERVAL_SECS: u64 = 30;
pub const MAX_INTERVAL_SECS: u64 = 600;
/// FR-3: 수동 새로고침 연타 방지
pub const REFRESH_THROTTLE_SECS: u64 = 5;

/// 대기 루프가 깨어나 시계를 확인하는 주기.
///
/// 폴링 간격만큼 한 번에 자지 않고 잘게 나눠 자면서 **벽시계**를 비교한다.
/// 절전에서 복귀하면 벽시계가 크게 점프해 있으므로 즉시 폴링으로 넘어간다
/// (FR-3 "절전 복귀 시 즉시 1회 조회"). 깨어나서 뺄셈만 하므로 유휴 부하는 무시할 수준.
const WAKE_CHECK: Duration = Duration::from_secs(5);

/// 확인 사이의 벽시계 간격이 이보다 벌어졌으면 그 사이 OS 가 절전에 들어갔다고 본다.
/// (정상이라면 `WAKE_CHECK` 근처여야 한다)
const RESUME_GAP_SECS: i64 = 15;

pub struct Poller {
    client: Client,
    state: Mutex<AppState>,
    /// 마지막으로 성공한 스냅샷. 실패 시 스테일 표시에 쓴다.
    last_snapshot: Mutex<Option<UsageSnapshot>>,
    /// 계정·세션 정보 (usage API 가 아니라 로컬 파일에서 온다)
    env: Mutex<Environment>,
    /// 수동 새로고침 신호
    refresh: Notify,
    /// 수동 새로고침 스로틀용
    last_manual: Mutex<Option<Instant>>,
    /// 폴링 주기. M4 설정에서 런타임 변경할 수 있도록 원자값으로 둔다.
    interval_secs: AtomicU64,
    /// 상태 변경 관찰자 (트레이 아이콘·툴팁 갱신).
    /// `Arc` 로 들고 있어 호출할 때 잠금을 잡은 채 실행하지 않는다.
    #[allow(clippy::type_complexity)]
    observer: Mutex<Option<Arc<dyn Fn(&AppHandle, &AppState) + Send + Sync>>>,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            // 클라이언트 생성 실패는 사실상 발생하지 않지만, 실패해도 앱은 떠야 한다.
            // 기본 클라이언트로 물러나면 타임아웃이 없으므로 폴링이 매달릴 수 있어
            // 그 경우에도 5초 타임아웃을 잃지 않도록 build_client 를 우선한다.
            client: usage_client::build_client().unwrap_or_default(),
            state: Mutex::new(AppState::Loading),
            last_snapshot: Mutex::new(None),
            env: Mutex::new(Environment::default()),
            refresh: Notify::new(),
            last_manual: Mutex::new(None),
            interval_secs: AtomicU64::new(DEFAULT_INTERVAL_SECS),
            observer: Mutex::new(None),
        }
    }

    pub fn state(&self) -> AppState {
        self.state.lock().unwrap().clone()
    }

    pub fn environment(&self) -> Environment {
        self.env.lock().unwrap().clone()
    }

    pub fn interval_secs(&self) -> u64 {
        self.interval_secs.load(Ordering::Relaxed)
    }

    /// M4 설정에서 호출. FR-8 범위(30초~10분)로 강제한다.
    ///
    /// 상한을 두는 건 UX 뿐 아니라 안전장치이기도 하다 — 대기 루프가 주기를
    /// `i64` 로 변환하므로, 터무니없이 큰 값이 들어오면 부호가 뒤집혀
    /// 5초마다 폴링하는 상태가 된다.
    pub fn set_interval_secs(&self, secs: u64) {
        self.interval_secs
            .store(secs.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS), Ordering::Relaxed);
    }

    /// 수동 새로고침 요청. 5초 스로틀에 걸리면 `Err` 를 돌려준다.
    pub fn request_refresh(&self) -> Result<(), String> {
        self.request_refresh_at(Instant::now())
    }

    /// 스로틀 판정 (테스트에서 시각을 주입할 수 있게 분리).
    fn request_refresh_at(&self, now: Instant) -> Result<(), String> {
        let mut last = self.last_manual.lock().unwrap();
        if let Some(prev) = *last {
            let since = now.saturating_duration_since(prev);
            if since < Duration::from_secs(REFRESH_THROTTLE_SECS) {
                let left = REFRESH_THROTTLE_SECS.saturating_sub(since.as_secs()).max(1);
                return Err(format!("{left}초 후에 다시 시도해 주세요"));
            }
        }
        *last = Some(now);
        drop(last);

        self.refresh.notify_one();
        Ok(())
    }

    /// 상태가 바뀔 때마다 불릴 관찰자를 등록한다 (트레이 갱신용).
    ///
    /// 이벤트를 다시 역직렬화해 받는 대신 콜백으로 넘긴다 — 왕복이 없고,
    /// poller 가 트레이를 몰라도 된다.
    pub fn set_observer(&self, observer: impl Fn(&AppHandle, &AppState) + Send + Sync + 'static) {
        *self.observer.lock().unwrap() = Some(Arc::new(observer));
    }

    /// 폴링 루프를 시작한다. 앱이 살아 있는 동안 계속 돈다.
    pub fn start(self: Arc<Self>, app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            loop {
                self.refresh_env(&app).await;
                self.poll_once(&app).await;
                self.wait_for_next_tick().await;
            }
        });
    }

    /// 다음 폴링까지 대기. 수동 새로고침 신호가 오면 즉시 깨어난다.
    async fn wait_for_next_tick(&self) {
        let started = Utc::now();
        let mut last_check = started;

        loop {
            let interval = chrono::Duration::seconds(self.interval_secs() as i64);
            tokio::select! {
                _ = tokio::time::sleep(WAKE_CHECK) => {
                    let now = Utc::now();
                    let elapsed = now.signed_duration_since(started);
                    let gap = now.signed_duration_since(last_check);
                    last_check = now;

                    if should_poll(elapsed, gap, interval) {
                        return;
                    }
                }
                _ = self.refresh.notified() => return,
            }
        }
    }

    /// 계정·세션 정보를 다시 읽어 프론트로 보낸다.
    ///
    /// 디렉터리 순회 + 파일 읽기라 usage 조회와 같은 이유로 블로킹 풀에서 돌린다.
    /// 실패해도 조용히 넘어간다 — 사용량 표시를 막을 이유가 없다.
    async fn refresh_env(&self, app: &AppHandle) {
        let Ok(env) = tokio::task::spawn_blocking(environment::load).await else {
            return;
        };
        // 트랜스크립트가 로테이션되는 순간처럼 일시적으로 아무것도 못 읽는 경우가 있다.
        // 그때 빈 값으로 덮으면 위젯의 계정·모델 줄이 깜빡 사라진다.
        if env.is_empty() {
            return;
        }
        *self.env.lock().unwrap() = env.clone();
        if let Err(e) = app.emit(EVENT_ENV, &env) {
            eprintln!("환경 이벤트 발행 실패: {e}");
        }
    }

    /// 1회 조회 후 상태를 전이시킨다.
    async fn poll_once(&self, app: &AppHandle) {
        // 자격증명 로드는 동기 파일 I/O 다. 폴링 태스크에서 그대로 부르면
        // 백신 검사·로밍 프로필 등으로 지연될 때 런타임 워커를 막아
        // 타이머와 커맨드까지 함께 밀린다. 블로킹 풀로 넘긴다.
        let creds = match tokio::task::spawn_blocking(credentials::load).await {
            Ok(Ok(c)) => c,
            // 만료는 "재인증 필요"로, 나머지(파일 없음/포맷 불일치)는 강등으로 나눈다
            Ok(Err(CredentialsError::Expired)) => {
                return self.publish(app, AppState::NeedsReauth)
            }
            Ok(Err(e)) => return self.degrade(app, e.to_string(), true),
            Err(e) => return self.degrade(app, format!("자격증명 로드 실패: {e}"), true),
        };

        match usage_client::fetch_with_retry(&self.client, &creds.token).await {
            Ok(snapshot) => {
                *self.last_snapshot.lock().unwrap() = Some(snapshot.clone());
                self.publish(app, AppState::Ok { snapshot });
            }
            Err(UsageError::Auth) => self.publish(app, AppState::NeedsReauth),
            // 스키마 불일치는 API 가 바뀌었다는 뜻이라 값이 영영 갱신되지 않는다.
            // 마지막 값을 스테일로 계속 보여주면 고장을 숨기게 되므로 조회 불가로 간다
            // (PRD FR-2 "필수 필드 누락 시 조회 불가", api-schema.md §5).
            Err(e @ UsageError::Schema) => self.degrade(app, e.to_string(), false),
            Err(e) => self.degrade(app, e.to_string(), true),
        }
    }

    /// 실패 시 상태 강등.
    ///
    /// `allow_stale` 이 참이면 마지막 성공 값을 스테일로 계속 보여준다
    /// (네트워크 일시 장애 등 곧 복구될 수 있는 경우).
    fn degrade(&self, app: &AppHandle, reason: String, allow_stale: bool) {
        let last = self.last_snapshot.lock().unwrap().clone();
        self.publish(app, degraded_state(last, reason, allow_stale));
    }

    fn publish(&self, app: &AppHandle, state: AppState) {
        *self.state.lock().unwrap() = state.clone();

        // 이벤트 발행 실패가 폴링을 멈추게 해서는 안 된다 (창이 아직 없을 수 있다)
        if let Err(e) = app.emit(EVENT_STATE, &state) {
            eprintln!("상태 이벤트 발행 실패: {e}");
        }

        // 관찰자는 잠금을 놓고 부른다 — 안에서 다시 poller 를 건드려도 교착이 없도록
        let observer = self.observer.lock().unwrap().clone();
        if let Some(f) = observer {
            f(app, &state);
        }
    }
}

impl Default for Poller {
    fn default() -> Self {
        Self::new()
    }
}

/// 조회 실패 시 어떤 상태로 강등할지 결정한다.
///
/// 마지막 성공 값이 있고 `allow_stale` 이면 그 값을 계속 보여주되 스테일로 표시한다
/// (PRD 시나리오 9: 네트워크 끊김). 값이 없거나 스테일이 허용되지 않으면 조회 불가.
fn degraded_state(last: Option<UsageSnapshot>, reason: String, allow_stale: bool) -> AppState {
    match last {
        Some(snapshot) if allow_stale => AppState::Stale { snapshot, reason },
        _ => AppState::Unavailable { reason },
    }
}

/// 대기 중 한 번의 확인에서 "지금 폴링해야 하는가"를 판정한다.
///
/// - `elapsed`: 마지막 폴링 이후 벽시계 경과
/// - `gap`: 직전 확인 이후 벽시계 경과 (정상이면 [`WAKE_CHECK`] 근처)
///
/// `gap` 이 크게 벌어졌다면 그 사이 OS 가 절전에 들어갔다는 뜻이다.
/// 이때는 남은 주기를 기다리지 않고 즉시 조회한다
/// (FR-3 "절전 모드 복귀 시 즉시 1회 조회").
fn should_poll(
    elapsed: chrono::Duration,
    gap: chrono::Duration,
    interval: chrono::Duration,
) -> bool {
    elapsed >= interval || gap >= chrono::Duration::seconds(RESUME_GAP_SECS)
}

/// 스테일 판정용: 스냅샷이 얼마나 오래됐는지.
pub fn snapshot_age(fetched_at: DateTime<Utc>, now: DateTime<Utc>) -> chrono::Duration {
    now.signed_duration_since(fetched_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LimitWindow;

    fn snapshot() -> UsageSnapshot {
        UsageSnapshot {
            fetched_at: Utc::now(),
            session: Some(LimitWindow {
                utilization: 17.0,
                resets_at: None,
            }),
            weekly: None,
            weekly_opus: None,
            model_scoped: vec![],
        }
    }

    #[test]
    fn starts_in_loading() {
        assert!(matches!(Poller::new().state(), AppState::Loading));
    }

    #[test]
    fn manual_refresh_is_throttled() {
        let p = Poller::new();
        let t0 = Instant::now();

        assert!(p.request_refresh_at(t0).is_ok(), "첫 요청은 통과");
        assert!(
            p.request_refresh_at(t0 + Duration::from_secs(1)).is_err(),
            "1초 뒤는 스로틀"
        );
        assert!(
            p.request_refresh_at(t0 + Duration::from_secs(REFRESH_THROTTLE_SECS))
                .is_ok(),
            "5초 뒤는 통과"
        );
    }

    #[test]
    fn interval_is_clamped_to_spec_range() {
        let p = Poller::new();
        assert_eq!(p.interval_secs(), DEFAULT_INTERVAL_SECS);

        p.set_interval_secs(5); // 하한 미만
        assert_eq!(p.interval_secs(), MIN_INTERVAL_SECS);

        p.set_interval_secs(300);
        assert_eq!(p.interval_secs(), 300);

        // 상한 초과. i64 로 변환할 때 부호가 뒤집혀 5초마다 폴링하는 사고를 막는다
        p.set_interval_secs(u64::MAX);
        assert_eq!(p.interval_secs(), MAX_INTERVAL_SECS);
        assert!(p.interval_secs() as i64 > 0);
    }

    /// 네트워크가 끊겨도 마지막 값을 계속 보여줘야 한다 (PRD 시나리오 9).
    #[test]
    fn degrades_to_stale_when_snapshot_exists() {
        let state = degraded_state(Some(snapshot()), "타임아웃".into(), true);
        match state {
            AppState::Stale { snapshot, reason } => {
                assert_eq!(snapshot.session.unwrap().utilization, 17.0);
                assert_eq!(reason, "타임아웃");
            }
            other => panic!("Stale 이어야 하는데 {other:?}"),
        }
    }

    /// 한 번도 성공한 적이 없으면 보여줄 값이 없으므로 조회 불가.
    #[test]
    fn degrades_to_unavailable_without_snapshot() {
        assert!(matches!(
            degraded_state(None, "타임아웃".into(), true),
            AppState::Unavailable { .. }
        ));
    }

    /// 스키마가 깨졌으면 마지막 값을 계속 보여주며 고장을 숨기면 안 된다
    /// (PRD FR-2 "필수 필드 누락 시 조회 불가").
    #[test]
    fn schema_error_never_shows_stale_value() {
        assert!(matches!(
            degraded_state(Some(snapshot()), "스키마".into(), false),
            AppState::Unavailable { .. }
        ));
    }

    #[test]
    fn polls_when_interval_elapsed() {
        let d = chrono::Duration::seconds;
        assert!(should_poll(d(60), d(5), d(60)), "주기 도달");
        assert!(!should_poll(d(30), d(5), d(60)), "아직 주기 전");
    }

    /// 절전 복귀: 확인 간격이 크게 벌어졌으면 남은 주기를 기다리지 않는다.
    #[test]
    fn polls_immediately_after_resume_gap() {
        let d = chrono::Duration::seconds;
        // 60초 주기의 중간(30초)이지만 직전 확인 이후 20초가 점프 → 절전 복귀로 판단
        assert!(should_poll(d(30), d(20), d(60)));
        // 정상 확인 간격(5초)이면 기다린다
        assert!(!should_poll(d(30), d(5), d(60)));
    }

    #[test]
    fn snapshot_age_is_positive_for_past() {
        let now = Utc::now();
        let past = now - chrono::Duration::minutes(3);
        assert_eq!(snapshot_age(past, now).num_minutes(), 3);
    }
}
