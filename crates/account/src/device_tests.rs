use super::*;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use zeroize::Zeroizing;

const CLIENT: Uuid = Uuid::from_u128(1);
const PROFILE: Uuid = Uuid::from_u128(2);
const MSA: &str = "synthetic-microsoft-access";
const DEVICE_CODE: &str = "synthetic-device-code";
const SAVED: &str = "synthetic-saved-refresh";
const NEXT: &str = "synthetic-rotated-refresh";
const XBOX: &str = "synthetic-xbox-user";
const XSTS: &str = "synthetic-xbox-title";
const GAME: &str = "synthetic-game-bearer";

type Exchange = (&'static str, Result<Response, AccountError>);

struct Fixture {
    now: Cell<Instant>,
    pending: RefCell<VecDeque<Exchange>>,
    observed: Cell<usize>,
    latency: Cell<Duration>,
}

impl Fixture {
    fn new(steps: impl IntoIterator<Item = Exchange>) -> Self {
        Self {
            now: Cell::new(Instant::now()),
            pending: RefCell::new(steps.into_iter().collect()),
            observed: Cell::new(0),
            latency: Cell::new(Duration::ZERO),
        }
    }

    fn advance(&self, seconds: u64) {
        self.now.set(self.now.get() + Duration::from_secs(seconds));
    }

    fn begin(&self) -> DeviceLogin {
        Flow::new(self, &|| self.now.get(), &|| false)
            .begin(CLIENT)
            .expect("challenge")
    }

    fn poll(&self, login: &mut DeviceLogin) -> Result<DevicePoll, AccountError> {
        Flow::new(self, &|| self.now.get(), &|| false).poll(CLIENT, login)
    }

    fn refresh(&self, token: &str) -> Result<AuthenticatedAccount, AccountError> {
        Flow::new(self, &|| self.now.get(), &|| false).refresh(CLIENT, token)
    }

    fn complete(&self) {
        assert!(self.pending.borrow().is_empty(), "scripted requests remain");
    }
}

impl Transport for Fixture {
    fn send(&self, request: Request<'_>, timeout: Duration) -> Result<Response, AccountError> {
        assert!(timeout > Duration::ZERO && timeout <= REQUEST_TIMEOUT);
        let kind = match request {
            Request::Device(id) if id == CLIENT => "device",
            Request::Poll(id, DEVICE_CODE) if id == CLIENT => "poll",
            Request::Refresh(id, SAVED) if id == CLIENT => "refresh",
            Request::XboxUser(MSA) => "xbox",
            Request::XboxTitle(XBOX) => "xsts",
            Request::Minecraft("42", XSTS) => "minecraft",
            Request::Entitlements(GAME) => "entitlements",
            Request::Profile(GAME) => "profile",
            _ => panic!("wrong request arguments"),
        };
        let (expected, result) = self
            .pending
            .borrow_mut()
            .pop_front()
            .expect("unexpected request");
        assert_eq!(expected, kind);
        self.observed.set(self.observed.get() + 1);
        self.now.set(self.now.get() + self.latency.get());
        result
    }
}

fn reply(kind: &'static str, status: u16, body: Value) -> Exchange {
    (
        kind,
        Ok(Response {
            status,
            body: Zeroizing::new(serde_json::to_vec(&body).unwrap()),
            retry_after: None,
        }),
    )
}

fn challenge(seconds: u64) -> Exchange {
    reply(
        "device",
        200,
        json!({
            "device_code": DEVICE_CODE, "user_code": "ABCD-EFGH",
            "verification_uri": "https://www.microsoft.com/devicelogin",
            "expires_in": seconds,
        }),
    )
}

fn failure(kind: &'static str, status: u16, code: &str) -> Exchange {
    reply(
        kind,
        status,
        json!({"error": code, "error_description": MSA}),
    )
}

fn access(kind: &'static str, refresh: Option<&str>) -> Exchange {
    reply(
        kind,
        200,
        json!({
            "access_token": MSA, "refresh_token": refresh, "token_type": "Bearer", "expires_in": 3600,
        }),
    )
}

fn xbox(kind: &'static str, token: &str, hash: &str) -> Exchange {
    reply(
        kind,
        200,
        json!({"Token": token, "DisplayClaims": {"xui": [{"uhs": hash}]}}),
    )
}

fn game() -> Exchange {
    reply(
        "minecraft",
        200,
        json!({
            "access_token": GAME, "token_type": "Bearer", "expires_in": 3600,
        }),
    )
}

fn entitled(name: &str) -> Exchange {
    reply(
        "entitlements",
        200,
        json!({"items": [{"name":name, "signature":"ignored-signed-claim"}]}),
    )
}

fn profile() -> Exchange {
    reply(
        "profile",
        200,
        json!({"id":PROFILE.simple().to_string(), "name":"Sample_42"}),
    )
}

fn exchange() -> [Exchange; 5] {
    [
        xbox("xbox", XBOX, "17"),
        xbox("xsts", XSTS, "42"),
        game(),
        entitled("game_minecraft"),
        profile(),
    ]
}

fn pending(result: Result<DevicePoll, AccountError>, expected: u64) {
    match result {
        Ok(DevicePoll::Pending { retry_after }) => {
            assert_eq!(retry_after, Duration::from_secs(expected))
        }
        _ => panic!("expected a pending challenge"),
    }
}

#[test]
fn device_poll_reports_challenge_and_requires_entitlement_and_profile() {
    let fixture = Fixture::new(
        [challenge(180), access("poll", Some(NEXT))]
            .into_iter()
            .chain(exchange()),
    );
    let mut login = fixture.begin();
    assert_eq!(
        serde_json::to_value(login.challenge()).unwrap(),
        json!({
            "verificationUri": "https://www.microsoft.com/devicelogin",
            "userCode": "ABCD-EFGH",
            "expiresInSeconds": 180
        })
    );
    pending(fixture.poll(&mut login), 5);
    fixture.advance(4);
    pending(fixture.poll(&mut login), 1);
    assert_eq!(fixture.observed.get(), 1);
    fixture.advance(1);
    let Ok(DevicePoll::Authenticated(account)) = fixture.poll(&mut login) else {
        panic!("expected a verified account");
    };
    assert_eq!(
        account.profile(),
        &MinecraftProfile {
            id: PROFILE,
            name: "Sample_42".into()
        }
    );
    assert_eq!(account.refresh_token(), Some(NEXT));
    assert_eq!(fixture.poll(&mut login).err(), Some(AccountError::Finished));
    fixture.complete();
}

#[test]
fn provider_pending_slowdown_and_connection_backoff_are_monotonic() {
    let mut rate = failure("poll", 429, "rate_limited");
    rate.1.as_mut().unwrap().retry_after = Some(Duration::from_secs(50));
    let fixture = Fixture::new([
        challenge(180),
        failure("poll", 400, "authorization_pending"),
        failure("poll", 400, "slow_down"),
        ("poll", Err(AccountError::TemporarilyUnavailable)),
        rate,
        failure("poll", 400, "authorization_pending"),
        failure("poll", 400, "authorization_declined"),
    ]);
    let mut login = fixture.begin();
    for (wait, next) in [(5, 5), (5, 10), (10, 20), (20, 50), (50, 10)] {
        fixture.advance(wait);
        pending(fixture.poll(&mut login), next);
    }
    fixture.advance(10);
    assert_eq!(fixture.poll(&mut login).err(), Some(AccountError::Declined));
    fixture.complete();
}

#[test]
fn wrong_client_leaves_the_attempt_intact_and_retry_never_extends_expiry() {
    let mut rate = failure("poll", 503, "unavailable");
    rate.1.as_mut().unwrap().retry_after = Some(Duration::MAX);
    let fixture = Fixture::new([challenge(20), rate]);
    let mut login = fixture.begin();
    let now = || fixture.now.get();
    let flow = Flow::new(&fixture, &now, &|| false);
    assert_eq!(
        flow.poll(Uuid::from_u128(9), &mut login).err(),
        Some(AccountError::InvalidChallenge)
    );
    fixture.advance(5);
    pending(fixture.poll(&mut login), 15);
    fixture.advance(15);
    assert_eq!(fixture.poll(&mut login).err(), Some(AccountError::Expired));
    assert_eq!(fixture.poll(&mut login).err(), Some(AccountError::Finished));
    fixture.complete();
}

#[test]
fn late_provider_reply_and_mid_exchange_cancellation_publish_nothing() {
    let fixture = Fixture::new([challenge(7), access("poll", Some(NEXT))]);
    let mut login = fixture.begin();
    fixture.latency.set(Duration::from_secs(2));
    fixture.advance(5);
    assert_eq!(fixture.poll(&mut login).err(), Some(AccountError::Expired));
    fixture.complete();

    let fixture = Fixture::new([
        challenge(120),
        access("poll", Some(NEXT)),
        xbox("xbox", XBOX, "42"),
    ]);
    let mut login = fixture.begin();
    fixture.advance(5);
    let now = || fixture.now.get();
    let cancelled = || fixture.observed.get() >= 3;
    let flow = Flow::new(&fixture, &now, &cancelled);
    assert_eq!(
        flow.poll(CLIENT, &mut login).err(),
        Some(AccountError::Cancelled)
    );
    assert_eq!(fixture.poll(&mut login).err(), Some(AccountError::Finished));
    fixture.complete();
}

#[test]
fn refresh_rotates_or_retains_the_previous_token_after_reverification() {
    for replacement in [None, Some(NEXT)] {
        let fixture = Fixture::new(
            [access("refresh", replacement)]
                .into_iter()
                .chain(exchange()),
        );
        let account = fixture.refresh(SAVED).expect("verified account");
        assert_eq!(account.profile().id, PROFILE);
        assert_eq!(account.refresh_token(), Some(replacement.unwrap_or(SAVED)));
        assert!(account.is_current());
        fixture.complete();
    }
}

#[test]
fn refresh_rejection_is_distinct_from_service_failure_or_registration_rejection() {
    for (status, code, expected) in [
        (400, "invalid_grant", AccountError::RefreshRejected),
        (401, "invalid_grant", AccountError::SignInRejected),
        (
            400,
            "unauthorized_client",
            AccountError::RegistrationRejected,
        ),
        (429, "invalid_grant", AccountError::TemporarilyUnavailable),
        (503, "invalid_grant", AccountError::TemporarilyUnavailable),
    ] {
        let fixture = Fixture::new([failure("refresh", status, code)]);
        let error = fixture.refresh(SAVED).err().unwrap();
        assert_eq!(error, expected);
        assert!(!error.to_string().contains(MSA));
        assert!(!error.to_string().contains(SAVED));
        fixture.complete();
    }
    let fixture = Fixture::new([]);
    assert_eq!(
        fixture.refresh("broken token").err(),
        Some(AccountError::InvalidRefreshToken)
    );
}

#[test]
fn invalid_xsts_hash_prevents_minecraft_exchange() {
    let fixture = Fixture::new([
        access("refresh", None),
        xbox("xbox", XBOX, "42"),
        xbox("xsts", XSTS, "42;wrong"),
    ]);
    assert_eq!(
        fixture.refresh(SAVED).err(),
        Some(AccountError::InvalidResponse)
    );
    fixture.complete();
}

#[test]
fn xbox_restriction_has_a_redacted_distinct_error() {
    let fixture = Fixture::new([
        access("refresh", None),
        xbox("xbox", XBOX, "42"),
        reply("xsts", 401, json!({"XErr":2148916233u64, "Message": MSA})),
    ]);
    assert_eq!(
        fixture.refresh(SAVED).err(),
        Some(AccountError::XboxProfileRequired)
    );
    fixture.complete();
}

#[test]
fn unknown_entitlement_or_missing_profile_never_returns_an_account() {
    let fixture = Fixture::new([
        access("refresh", Some(NEXT)),
        xbox("xbox", XBOX, "17"),
        xbox("xsts", XSTS, "42"),
        game(),
        entitled("product_unrelated"),
    ]);
    assert_eq!(
        fixture.refresh(SAVED).err(),
        Some(AccountError::NotEntitled)
    );
    fixture.complete();

    let fixture = Fixture::new([
        access("refresh", Some(NEXT)),
        xbox("xbox", XBOX, "17"),
        xbox("xsts", XSTS, "42"),
        game(),
        entitled("product_minecraft"),
        reply("profile", 404, json!({})),
    ]);
    assert_eq!(
        fixture.refresh(SAVED).err(),
        Some(AccountError::MinecraftProfileRequired)
    );
    fixture.complete();
}

#[test]
fn browser_target_stays_with_microsoft_and_excludes_parameters() {
    for address in [
        "https://microsoft.com.evil.test/devicelogin",
        "https://evil.test@www.microsoft.com/devicelogin",
        "https://www.microsoft.com:8443/devicelogin",
        "https://microsoft.com/devicelogin?code=misdirection",
        "https://microsoft.com/devicelogin#where",
        "http://www.microsoft.com/devicelogin",
        "https://microsoft.com/different",
    ] {
        assert_eq!(
            allowed_browser_url(address),
            Err(AccountError::InvalidResponse)
        );
    }
    assert!(allowed_browser_url("https://microsoft.com/devicelogin").is_ok());
}
