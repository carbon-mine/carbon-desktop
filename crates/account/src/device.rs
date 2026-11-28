use crate::transport::{Http, Request, Response, Transport, REQUEST_TIMEOUT};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, Instant},
};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroize;

const MIN_POLL: Duration = Duration::from_secs(1);
const DEFAULT_INTERVAL: u64 = 5;
const MAX_DEVICE_LIFETIME: u64 = 60 * 60;
const MAX_TOKEN_LIFETIME: u64 = 7 * 24 * 60 * 60;
const RATE_LIMIT_FLOOR: Duration = Duration::from_secs(30);
const TEMPORARY_DELAY_LIMIT: Duration = Duration::from_secs(60);

/// Errors contain only application-defined text. Provider responses and tokens stay private.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountError {
    InvalidRegistration,
    RegistrationRejected,
    Cancelled,
    Expired,
    Declined,
    Finished,
    InvalidChallenge,
    InvalidRefreshToken,
    RefreshRejected,
    TemporarilyUnavailable,
    InvalidResponse,
    SignInRejected,
    XboxProfileRequired,
    XboxRestricted,
    XboxRejected,
    MinecraftRejected,
    NotEntitled,
    MinecraftProfileRequired,
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRegistration => "Enter a public client ID to configure Minecraft sign-in.",
            Self::RegistrationRejected => "Microsoft rejected sign-in for the configured client. Check the client configuration or use the Minecraft Launcher.",
            Self::Cancelled => "Microsoft sign-in was cancelled.",
            Self::Expired => "The sign-in authorization expired. Start sign-in again.",
            Self::Declined => "Microsoft sign-in was declined.",
            Self::Finished => "That sign-in attempt has ended. Start sign-in again.",
            Self::InvalidChallenge => "The sign-in attempt belongs to a different client configuration.",
            Self::InvalidRefreshToken => "The saved Microsoft authorization is unreadable. Sign in again.",
            Self::RefreshRejected => "Microsoft no longer accepts the saved authorization. Sign in again.",
            Self::TemporarilyUnavailable => "The sign-in service is temporarily unavailable. Check your connection and retry.",
            Self::InvalidResponse => "The sign-in service returned an unexpected response. Try again later.",
            Self::SignInRejected => "Microsoft did not authorize this sign-in. Start sign-in again.",
            Self::XboxProfileRequired => "This Microsoft account needs an Xbox profile to continue.",
            Self::XboxRestricted => "Xbox restricted this account. Check the account's age, family, region or other Xbox settings.",
            Self::XboxRejected => "Xbox did not authorize this account. Check the Xbox account and try again.",
            Self::MinecraftRejected => "Minecraft Services rejected sign-in for the configured client. Check the client configuration or use the Minecraft Launcher.",
            Self::NotEntitled => "Minecraft Services did not confirm a Minecraft: Java Edition entitlement for this account.",
            Self::MinecraftProfileRequired => "Set up a Minecraft: Java Edition profile for this account, then try again.",
        })
    }
}

impl std::error::Error for AccountError {}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceChallenge {
    pub verification_uri: String,
    pub user_code: String,
    pub expires_in_seconds: u64,
}

/// Holds the opaque authorization state in memory. It is bound to the requesting client ID.
pub struct DeviceLogin {
    client_id: Uuid,
    challenge: DeviceChallenge,
    device_code: Option<Secret>,
    deadline: Instant,
    next_poll: Instant,
    interval: Duration,
    retry_delay: Duration,
}

impl DeviceLogin {
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn challenge(&self) -> &DeviceChallenge {
        &self.challenge
    }

    #[cfg(all(test, feature = "dev-simulation"))]
    pub(crate) fn poll_now(&mut self) {
        self.next_poll = Instant::now();
    }
}

pub enum DevicePoll {
    Pending { retry_after: Duration },
    Authenticated(AuthenticatedAccount),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MinecraftProfile {
    pub id: Uuid,
    pub name: String,
}

/// This result is Rust-only. Expose only its verified profile to the UI.
pub struct AuthenticatedAccount {
    profile: MinecraftProfile,
    refresh_token: Option<Secret>,
    until: Instant,
}

impl AuthenticatedAccount {
    #[cfg(test)]
    pub(crate) fn synthetic(id: Uuid, name: &str, refresh: &str) -> Self {
        Self {
            profile: MinecraftProfile {
                id,
                name: name.into(),
            },
            refresh_token: Some(Secret(refresh.into())),
            until: Instant::now() + Duration::from_secs(3600),
        }
    }
    pub fn profile(&self) -> &MinecraftProfile {
        &self.profile
    }

    /// This is a secret for the shell's OS credential vault.
    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_ref().map(Secret::as_str)
    }

    pub(crate) fn invalidate_verification(&mut self) {
        self.until = Instant::now();
    }

    /// Time bound of this online verification. Revocation still requires a provider check.
    pub fn is_current(&self) -> bool {
        Instant::now() < self.until
    }
}

pub struct DeviceClient {
    client_id: Uuid,
    transport: Box<dyn Transport>,
}

impl DeviceClient {
    /// Use the configured public-client application registration.
    pub fn new(client_id: Uuid) -> Result<Self, AccountError> {
        if client_id.is_nil() {
            return Err(AccountError::InvalidRegistration);
        }
        Ok(Self {
            client_id,
            transport: Box::new(Http::new()?),
        })
    }

    /// Use an in-process transport. The developer simulation is the only caller
    /// in shipped code; it never creates a network client.
    #[cfg(feature = "dev-simulation")]
    pub(crate) fn with_transport(
        client_id: Uuid,
        transport: impl Transport + 'static,
    ) -> Result<Self, AccountError> {
        if client_id.is_nil() {
            return Err(AccountError::InvalidRegistration);
        }
        Ok(Self {
            client_id,
            transport: Box::new(transport),
        })
    }

    pub fn begin(&self, cancelled: impl Fn() -> bool) -> Result<DeviceLogin, AccountError> {
        Flow::new(self.transport.as_ref(), &Instant::now, &cancelled).begin(self.client_id)
    }

    /// Returns promptly until the provider's polling delay has elapsed. A due poll can block
    /// for the bounded request timeout. Cancellation is checked between network operations.
    pub fn poll(
        &self,
        login: &mut DeviceLogin,
        cancelled: impl Fn() -> bool,
    ) -> Result<DevicePoll, AccountError> {
        Flow::new(self.transport.as_ref(), &Instant::now, &cancelled).poll(self.client_id, login)
    }

    /// Obtain a fresh provider response and recheck entitlement/profile before returning.
    pub fn refresh(
        &self,
        refresh_token: &str,
        cancelled: impl Fn() -> bool,
    ) -> Result<AuthenticatedAccount, AccountError> {
        Flow::new(self.transport.as_ref(), &Instant::now, &cancelled)
            .refresh(self.client_id, refresh_token)
    }
}

struct Flow<'a> {
    transport: &'a dyn Transport,
    now: &'a dyn Fn() -> Instant,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> Flow<'a> {
    fn new(
        transport: &'a dyn Transport,
        now: &'a dyn Fn() -> Instant,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            transport,
            now,
            cancelled,
        }
    }

    fn check(&self, deadline: Option<Instant>) -> Result<Duration, AccountError> {
        if (self.cancelled)() {
            return Err(AccountError::Cancelled);
        }
        let Some(deadline) = deadline else {
            return Ok(REQUEST_TIMEOUT);
        };
        let remaining = deadline.saturating_duration_since((self.now)());
        if remaining.is_zero() {
            return Err(AccountError::Expired);
        }
        Ok(remaining.min(REQUEST_TIMEOUT))
    }

    fn send(
        &self,
        request: Request<'_>,
        deadline: Option<Instant>,
    ) -> Result<Response, AccountError> {
        let timeout = self.check(deadline)?;
        let response = self.transport.send(request, timeout);
        self.check(deadline)?;
        response
    }

    fn begin(&self, client_id: Uuid) -> Result<DeviceLogin, AccountError> {
        let started = (self.now)();
        let response = self.send(Request::Device(client_id), None)?;
        if response.status != 200 {
            return Err(oauth_error(&response, false));
        }
        let payload: DeviceResponse = decode(&response)?;
        if !(1..=MAX_DEVICE_LIFETIME).contains(&payload.expires_in)
            || !valid_token(payload.device_code.as_str())
            || payload.user_code.is_empty()
            || payload.user_code.len() > 32
            || !payload
                .user_code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(AccountError::InvalidResponse);
        }
        let uri = allowed_browser_url(&payload.verification_uri)?;
        let interval = Duration::from_secs(payload.interval.unwrap_or(DEFAULT_INTERVAL));
        if !(MIN_POLL..=Duration::from_secs(MAX_DEVICE_LIFETIME)).contains(&interval) {
            return Err(AccountError::InvalidResponse);
        }
        let now = (self.now)();
        let deadline = expires_at(started, payload.expires_in)?;
        self.check(Some(deadline))?;
        let remaining = deadline.duration_since(now);
        Ok(DeviceLogin {
            client_id,
            challenge: DeviceChallenge {
                verification_uri: uri,
                user_code: payload.user_code,
                expires_in_seconds: remaining.as_secs(),
            },
            device_code: Some(payload.device_code),
            deadline,
            next_poll: now + interval.min(remaining),
            interval,
            retry_delay: interval,
        })
    }

    fn poll(&self, client_id: Uuid, login: &mut DeviceLogin) -> Result<DevicePoll, AccountError> {
        if login.client_id != client_id {
            return Err(AccountError::InvalidChallenge);
        }
        let result = self.poll_once(login);
        if !matches!(&result, Ok(DevicePoll::Pending { .. })) {
            login.device_code = None;
        }
        result
    }

    fn poll_once(&self, login: &mut DeviceLogin) -> Result<DevicePoll, AccountError> {
        let device_code = login.device_code.as_ref().ok_or(AccountError::Finished)?;
        self.check(Some(login.deadline))?;
        let now = (self.now)();
        if now < login.next_poll {
            return Ok(DevicePoll::Pending {
                retry_after: login.next_poll.duration_since(now),
            });
        }
        let started = now;
        let response = match self.send(
            Request::Poll(login.client_id, device_code.as_str()),
            Some(login.deadline),
        ) {
            Ok(response) => response,
            Err(AccountError::TemporarilyUnavailable) => {
                return Ok(login.transient((self.now)(), None))
            }
            Err(reason) => return Err(reason),
        };
        let now = (self.now)();
        if response.status == 200 {
            return self
                .exchange(decode(&response)?, None, started)
                .map(DevicePoll::Authenticated);
        }
        if response.status == 429 || response.status >= 500 {
            let delay = if response.status == 429 {
                Some(response.retry_after.unwrap_or(RATE_LIMIT_FLOOR))
            } else {
                response.retry_after
            };
            return Ok(login.transient(now, delay));
        }
        if response.status == 400 {
            match oauth_code(&response) {
                Some("authorization_pending") => {
                    login.retry_delay = login.interval;
                    return Ok(login.reschedule(
                        now,
                        login.interval.max(response.retry_after.unwrap_or_default()),
                    ));
                }
                Some("slow_down") => {
                    login.interval = login.interval.saturating_add(Duration::from_secs(5));
                    login.retry_delay = login.interval;
                    return Ok(login.reschedule(
                        now,
                        login.interval.max(response.retry_after.unwrap_or_default()),
                    ));
                }
                Some("authorization_declined" | "access_denied") => {
                    return Err(AccountError::Declined)
                }
                Some("expired_token" | "bad_verification_code") => {
                    return Err(AccountError::Expired)
                }
                _ => {}
            }
        }
        Err(oauth_error(&response, false))
    }

    fn refresh(
        &self,
        client_id: Uuid,
        old_token: &str,
    ) -> Result<AuthenticatedAccount, AccountError> {
        if !valid_token(old_token) {
            return Err(AccountError::InvalidRefreshToken);
        }
        let started = (self.now)();
        let response = self.send(Request::Refresh(client_id, old_token), None)?;
        if response.status != 200 {
            return Err(oauth_error(&response, true));
        }
        self.exchange(decode(&response)?, Some(old_token), started)
    }

    fn exchange(
        &self,
        oauth: TokenReply,
        prior: Option<&str>,
        issued: Instant,
    ) -> Result<AuthenticatedAccount, AccountError> {
        oauth.validate()?;
        let oauth_until = expires_at(issued, oauth.expires_in)?;
        let xbox_user = self.send(
            Request::XboxUser(oauth.access_token.as_str()),
            Some(oauth_until),
        )?;
        let user = xbox_token(&xbox_user)?;
        let xsts = self.send(Request::XboxTitle(user.token.as_str()), Some(oauth_until))?;
        let title = xbox_token(&xsts)?;
        // The hash selects a single user inside this title token. Xbox does not
        // guarantee that another token issued for the same user has the same hash.
        let hash = title.hash()?;

        let issued = (self.now)();
        let minecraft = self.send(
            Request::Minecraft(hash, title.token.as_str()),
            Some(oauth_until),
        )?;
        let token: TokenReply = decode(&service_success(
            minecraft,
            AccountError::MinecraftRejected,
        )?)?;
        token.validate()?;
        let until = expires_at(issued, token.expires_in)?;
        let bearer = token.access_token.as_str();

        let response = self.send(Request::Entitlements(bearer), Some(until))?;
        let response = service_success(response, AccountError::MinecraftRejected)?;
        let entitlements: Entitlements = decode(&response)?;
        if entitlements.items.len() > 1024 {
            return Err(AccountError::InvalidResponse);
        }
        if !entitlements
            .items
            .iter()
            .any(|item| matches!(item.name.as_str(), "product_minecraft" | "game_minecraft"))
        {
            return Err(AccountError::NotEntitled);
        }

        let response = self.send(Request::Profile(bearer), Some(until))?;
        if response.status == 404 {
            return Err(AccountError::MinecraftProfileRequired);
        }
        let response = service_success(response, AccountError::MinecraftRejected)?;
        let profile: ProfileReply = decode(&response)?;
        let id = Uuid::parse_str(&profile.id).map_err(|_| AccountError::InvalidResponse)?;
        if id.is_nil()
            || !(1..=16).contains(&profile.name.len())
            || !profile
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(AccountError::InvalidResponse);
        }
        self.check(Some(until))?;
        Ok(AuthenticatedAccount {
            profile: MinecraftProfile {
                id,
                name: profile.name,
            },
            refresh_token: oauth
                .refresh_token
                .or_else(|| prior.map(|token| Secret(token.to_owned()))),
            until,
        })
    }
}

impl DeviceLogin {
    fn reschedule(&mut self, now: Instant, delay: Duration) -> DevicePoll {
        let retry_after = delay.min(self.deadline.saturating_duration_since(now));
        self.next_poll = now + retry_after;
        DevicePoll::Pending { retry_after }
    }

    fn transient(&mut self, now: Instant, advertised: Option<Duration>) -> DevicePoll {
        self.retry_delay = self
            .retry_delay
            .saturating_mul(2)
            .min(TEMPORARY_DELAY_LIMIT)
            .max(self.interval);
        self.reschedule(now, self.retry_delay.max(advertised.unwrap_or_default()))
    }
}

#[derive(Deserialize)]
#[serde(transparent)]
struct Secret(String);

impl Secret {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Deserialize)]
struct DeviceResponse {
    device_code: Secret,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Secret,
    refresh_token: Option<Secret>,
    token_type: String,
    expires_in: u64,
}

impl TokenReply {
    fn validate(&self) -> Result<(), AccountError> {
        if self.token_type.eq_ignore_ascii_case("bearer")
            && (1..=MAX_TOKEN_LIFETIME).contains(&self.expires_in)
            && valid_token(self.access_token.as_str())
            && self
                .refresh_token
                .as_ref()
                .is_none_or(|token| valid_token(token.as_str()))
        {
            Ok(())
        } else {
            Err(AccountError::InvalidResponse)
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxReply {
    token: Secret,
    display_claims: Option<XboxClaims>,
}

impl XboxReply {
    fn hash(&self) -> Result<&str, AccountError> {
        let claims = self
            .display_claims
            .as_ref()
            .ok_or(AccountError::InvalidResponse)?;
        let [item] = claims.xui.as_slice() else {
            return Err(AccountError::InvalidResponse);
        };
        if (1..=32).contains(&item.uhs.len()) && item.uhs.bytes().all(|byte| byte.is_ascii_digit())
        {
            Ok(&item.uhs)
        } else {
            Err(AccountError::InvalidResponse)
        }
    }
}

#[derive(Deserialize)]
struct XboxClaims {
    xui: Vec<XboxIdentity>,
}

#[derive(Deserialize)]
struct XboxIdentity {
    uhs: String,
}

#[derive(Deserialize)]
struct Entitlements {
    items: Vec<Entitlement>,
}

#[derive(Deserialize)]
struct Entitlement {
    name: String,
}

#[derive(Deserialize)]
struct ProfileReply {
    id: String,
    name: String,
}

fn decode<T: DeserializeOwned>(response: &Response) -> Result<T, AccountError> {
    serde_json::from_slice(&response.body).map_err(|_| AccountError::InvalidResponse)
}

fn valid_token(token: &str) -> bool {
    (1..=64 * 1024).contains(&token.len()) && token.bytes().all(|byte| byte.is_ascii_graphic())
}

fn expires_at(started: Instant, seconds: u64) -> Result<Instant, AccountError> {
    started
        .checked_add(Duration::from_secs(seconds))
        .ok_or(AccountError::InvalidResponse)
}

fn allowed_browser_url(value: &str) -> Result<String, AccountError> {
    let url = Url::parse(value).map_err(|_| AccountError::InvalidResponse)?;
    if url.scheme() == "https"
        && matches!(url.host_str(), Some("microsoft.com" | "www.microsoft.com"))
        && url.path() == "/devicelogin"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
    {
        Ok(url.into())
    } else {
        Err(AccountError::InvalidResponse)
    }
}

fn oauth_code(response: &Response) -> Option<&str> {
    #[derive(Deserialize)]
    struct Fault<'a> {
        error: &'a str,
    }
    serde_json::from_slice::<Fault<'_>>(&response.body)
        .ok()
        .map(|fault| fault.error)
}

fn oauth_error(response: &Response, refresh: bool) -> AccountError {
    match response.status {
        429 | 500..=599 => AccountError::TemporarilyUnavailable,
        400 | 401 | 403 => match oauth_code(response) {
            Some("invalid_client" | "unauthorized_client" | "invalid_scope") => {
                AccountError::RegistrationRejected
            }
            Some("invalid_grant") if refresh && response.status == 400 => {
                AccountError::RefreshRejected
            }
            _ => AccountError::SignInRejected,
        },
        _ => AccountError::InvalidResponse,
    }
}

fn service_error(status: u16, rejection: AccountError) -> AccountError {
    match status {
        429 | 500..=599 => AccountError::TemporarilyUnavailable,
        400 | 401 | 403 => rejection,
        _ => AccountError::InvalidResponse,
    }
}

fn service_success(response: Response, rejection: AccountError) -> Result<Response, AccountError> {
    if response.status == 200 {
        Ok(response)
    } else {
        Err(service_error(response.status, rejection))
    }
}

fn xbox_token(response: &Response) -> Result<XboxReply, AccountError> {
    match response.status {
        200 => {
            let token: XboxReply = decode(response)?;
            if valid_token(token.token.as_str()) && !token.token.as_str().contains(';') {
                Ok(token)
            } else {
                Err(AccountError::InvalidResponse)
            }
        }
        401 | 403 => {
            #[derive(Deserialize)]
            struct XboxFault {
                #[serde(rename = "XErr")]
                code: u64,
            }
            let code = decode::<XboxFault>(response).ok().map(|fault| fault.code);
            Err(match code {
                Some(2148916233) => AccountError::XboxProfileRequired,
                Some(2148916222 | 2148916227 | 2148916229 | 2148916234..=2148916238) => {
                    AccountError::XboxRestricted
                }
                _ => AccountError::XboxRejected,
            })
        }
        status => Err(service_error(status, AccountError::XboxRejected)),
    }
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
