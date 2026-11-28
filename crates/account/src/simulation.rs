// SPDX-License-Identifier: MPL-2.0
//! Deterministic, in-process provider for the developer account workflow.
//!
//! This module is compiled only with the non-default `dev-simulation` feature.
//! It implements the same transport contract as the production HTTP client so
//! the ordinary device flow, entitlement exchange and `Accounts` lifecycle run
//! unchanged. It never opens a browser, contacts a provider or reads a real
//! credential store.

use crate::{
    transport::{Request, Response, Transport},
    AccountError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use uuid::Uuid;
use zeroize::Zeroizing;

/// A fixed public client identifier used only by the developer simulation.
pub const CLIENT_ID: Uuid = Uuid::from_u128(0x5eed_1120_0000_4000_8000_0000_0000_0001);

/// Provenance marker written by the guarded GPUI startup.
pub const ROOT_MARKER_FILE: &str = "dev-account-simulation.json";
pub const ROOT_MARKER_KIND: &str = "carbon-desktop-dev-account-simulation";
pub const ROOT_MARKER_VERSION: u32 = 1;
const ROOT_MARKER_LIMIT: u64 = 16 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RootMarker {
    version: u32,
    kind: String,
    client_id: Uuid,
}

fn marker_is_linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.is_symlink()
}

fn read_root_marker(root: &Path) -> Result<RootMarker, ()> {
    let path = root.join(ROOT_MARKER_FILE);
    let metadata = fs::symlink_metadata(&path).map_err(|_| ())?;
    if marker_is_linked(&metadata) || !metadata.is_file() || metadata.len() > ROOT_MARKER_LIMIT {
        return Err(());
    }
    let bytes = fs::read(&path).map_err(|_| ())?;
    if bytes.len() as u64 > ROOT_MARKER_LIMIT {
        return Err(());
    }
    let marker: RootMarker = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if marker.version != ROOT_MARKER_VERSION
        || marker.kind != ROOT_MARKER_KIND
        || marker.client_id != CLIENT_ID
    {
        return Err(());
    }
    Ok(marker)
}

/// Whether a directory already carries this simulation's provenance marker.
pub fn root_is_marked(root: &Path) -> bool {
    read_root_marker(root).is_ok()
}

/// Write or validate the marker for a guarded simulation account root.
/// Callers still own the decision to use the developer feature.
pub fn mark_root(root: &Path) -> Result<(), String> {
    match read_root_marker(root) {
        Ok(_) => Ok(()),
        Err(()) if !root.join(ROOT_MARKER_FILE).exists() => {
            let marker = RootMarker {
                version: ROOT_MARKER_VERSION,
                kind: ROOT_MARKER_KIND.to_owned(),
                client_id: CLIENT_ID,
            };
            let bytes = serde_json::to_vec_pretty(&marker)
                .map_err(|_| "The developer account marker could not be serialized.".to_owned())?;
            fs::write(root.join(ROOT_MARKER_FILE), bytes)
                .map_err(|_| "The developer account marker could not be written.".to_owned())
        }
        Err(()) => {
            Err("The developer account root has an unreadable or unsupported marker.".to_owned())
        }
    }
}

const DEVICE_LOGIN_URI: &str = "https://www.microsoft.com/devicelogin";
const TOKEN_LIFETIME: u64 = 3600;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Persona {
    Forest,
    Coast,
}

impl Persona {
    pub const ALL: [Self; 2] = [Self::Forest, Self::Coast];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Forest => "Forest",
            Self::Coast => "Coast",
        }
    }

    pub const fn profile_id(self) -> Uuid {
        match self {
            Self::Forest => Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111),
            Self::Coast => Uuid::from_u128(0x2222_2222_2222_4222_8222_2222_2222_2222),
        }
    }

    pub fn from_profile_id(id: Uuid) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|persona| persona.profile_id() == id)
    }

    const fn key(self) -> &'static str {
        match self {
            Self::Forest => "forest",
            Self::Coast => "coast",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "forest" => Some(Self::Forest),
            "coast" => Some(Self::Coast),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntitlementState {
    Granted,
    Denied,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationPhase {
    Idle,
    AwaitingApproval,
    Approved,
    Declined,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingOutcome {
    AwaitingApproval,
    Approved,
    Declined,
    Expired,
}

struct Pending {
    persona: Persona,
    sequence: u64,
    device_code: String,
    outcome: PendingOutcome,
}

struct State {
    next: Persona,
    pending: Option<Pending>,
    sequence: u64,
    entitlements: BTreeMap<Persona, EntitlementState>,
}

impl State {
    fn new() -> Self {
        Self {
            next: Persona::Forest,
            pending: None,
            sequence: 0,
            entitlements: Persona::ALL
                .into_iter()
                .map(|persona| (persona, EntitlementState::Granted))
                .collect(),
        }
    }

    fn entitlement(&self, persona: Persona) -> EntitlementState {
        self.entitlements
            .get(&persona)
            .copied()
            .unwrap_or(EntitlementState::Granted)
    }
}

/// Shared developer controls for the in-process provider.
#[derive(Clone)]
pub struct Simulation {
    state: Arc<Mutex<State>>,
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulation {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::new())),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn next_persona(&self) -> Persona {
        self.state().next
    }

    pub fn set_next_persona(&self, persona: Persona) {
        self.state().next = persona;
    }

    pub fn phase(&self) -> SimulationPhase {
        let state = self.state();
        let Some(pending) = state.pending.as_ref() else {
            return SimulationPhase::Idle;
        };
        match pending.outcome {
            PendingOutcome::AwaitingApproval => SimulationPhase::AwaitingApproval,
            PendingOutcome::Approved => SimulationPhase::Approved,
            PendingOutcome::Declined => SimulationPhase::Declined,
            PendingOutcome::Expired => SimulationPhase::Expired,
        }
    }

    pub fn pending_persona(&self) -> Option<Persona> {
        self.state().pending.as_ref().map(|pending| pending.persona)
    }

    pub fn approve(&self) -> bool {
        self.finish_pending(PendingOutcome::Approved)
    }

    pub fn decline(&self) -> bool {
        self.finish_pending(PendingOutcome::Declined)
    }

    pub fn expire(&self) -> bool {
        self.finish_pending(PendingOutcome::Expired)
    }

    fn finish_pending(&self, outcome: PendingOutcome) -> bool {
        let mut state = self.state();
        let Some(pending) = state.pending.as_mut() else {
            return false;
        };
        if pending.outcome != PendingOutcome::AwaitingApproval {
            return false;
        }
        pending.outcome = outcome;
        true
    }

    pub fn entitlement(&self, persona: Persona) -> EntitlementState {
        self.state().entitlement(persona)
    }

    pub fn set_entitlement(&self, persona: Persona, entitlement: EntitlementState) {
        self.state().entitlements.insert(persona, entitlement);
    }

    pub fn reset(&self) {
        *self.state() = State::new();
    }

    /// Drop an outstanding challenge when the account service cancels it.
    pub fn cancel_pending(&self) -> bool {
        self.state().pending.take().is_some()
    }

    pub(crate) fn transport(&self) -> impl Transport + 'static {
        SimulatedTransport {
            simulation: self.clone(),
        }
    }
}

struct SimulatedTransport {
    simulation: Simulation,
}

impl SimulatedTransport {
    fn reply(status: u16, body: serde_json::Value) -> Response {
        Response {
            status,
            body: Zeroizing::new(serde_json::to_vec(&body).expect("simulation reply serializes")),
            retry_after: None,
        }
    }

    fn oauth_fault(error: &str) -> Response {
        Self::reply(400, json!({ "error": error }))
    }
}

impl Transport for SimulatedTransport {
    fn send(&self, request: Request<'_>, timeout: Duration) -> Result<Response, AccountError> {
        if timeout.is_zero() {
            return Err(AccountError::TemporarilyUnavailable);
        }
        let mut state = self.simulation.state();
        Ok(match request {
            Request::Device(client_id) => {
                if client_id != CLIENT_ID {
                    return Ok(Self::oauth_fault("invalid_client"));
                }
                state.sequence = state.sequence.saturating_add(1);
                let sequence = state.sequence;
                let persona = state.next;
                let device_code = format!("sim-device-{sequence}");
                let user_code = format!("TEST-{sequence:04}");
                state.pending = Some(Pending {
                    persona,
                    sequence,
                    device_code: device_code.clone(),
                    outcome: PendingOutcome::AwaitingApproval,
                });
                Self::reply(
                    200,
                    json!({
                        "device_code": device_code,
                        "user_code": user_code,
                        "verification_uri": DEVICE_LOGIN_URI,
                        "expires_in": 900,
                        "interval": 1,
                    }),
                )
            }
            Request::Poll(client_id, device_code) => {
                if client_id != CLIENT_ID {
                    return Ok(Self::oauth_fault("invalid_client"));
                }
                let Some(pending) = state
                    .pending
                    .as_ref()
                    .filter(|pending| pending.device_code == device_code)
                else {
                    return Ok(Self::oauth_fault("expired_token"));
                };
                match pending.outcome {
                    PendingOutcome::AwaitingApproval => Self::oauth_fault("authorization_pending"),
                    PendingOutcome::Declined => Self::oauth_fault("access_denied"),
                    PendingOutcome::Expired => Self::oauth_fault("expired_token"),
                    PendingOutcome::Approved => {
                        let persona = pending.persona;
                        let sequence = pending.sequence;
                        Self::reply(
                            200,
                            json!({
                                "access_token": access_token(persona, sequence),
                                "refresh_token": refresh_token(persona, sequence),
                                "token_type": "Bearer",
                                "expires_in": TOKEN_LIFETIME,
                            }),
                        )
                    }
                }
            }
            Request::Refresh(client_id, token) => {
                if client_id != CLIENT_ID {
                    return Ok(Self::oauth_fault("invalid_client"));
                }
                let Some(persona) = persona_from_token(token, "sim-refresh-") else {
                    return Ok(Self::oauth_fault("invalid_grant"));
                };
                if state.entitlement(persona) == EntitlementState::Revoked {
                    return Ok(Self::oauth_fault("invalid_grant"));
                }
                state.sequence = state.sequence.saturating_add(1);
                let sequence = state.sequence;
                Self::reply(
                    200,
                    json!({
                        "access_token": access_token(persona, sequence),
                        "refresh_token": refresh_token(persona, sequence),
                        "token_type": "Bearer",
                        "expires_in": TOKEN_LIFETIME,
                    }),
                )
            }
            Request::XboxUser(token) => {
                let Some(persona) = persona_from_token(token, "sim-msa-") else {
                    return Ok(Self::reply(401, json!({ "XErr": 2148916233u64 })));
                };
                Self::reply(
                    200,
                    json!({
                        "Token": xbox_token(persona),
                        "DisplayClaims": { "xui": [{ "uhs": "1001" }] },
                    }),
                )
            }
            Request::XboxTitle(token) => {
                let Some(persona) = persona_from_token(token, "sim-xbox-") else {
                    return Ok(Self::reply(401, json!({ "XErr": 2148916233u64 })));
                };
                Self::reply(
                    200,
                    json!({
                        "Token": xsts_token(persona),
                        "DisplayClaims": { "xui": [{ "uhs": "1001" }] },
                    }),
                )
            }
            Request::Minecraft(hash, token) => {
                let Some(persona) = persona_from_token(token, "sim-xsts-") else {
                    return Ok(Self::reply(401, json!({ "error": "invalid_token" })));
                };
                if hash != "1001" {
                    return Ok(Self::reply(400, json!({ "error": "invalid_token" })));
                }
                Self::reply(
                    200,
                    json!({
                        "access_token": game_token(persona),
                        "token_type": "Bearer",
                        "expires_in": TOKEN_LIFETIME,
                    }),
                )
            }
            Request::Entitlements(token) => {
                let Some(persona) = persona_from_token(token, "sim-game-") else {
                    return Ok(Self::reply(401, json!({ "error": "invalid_token" })));
                };
                let items = if state.entitlement(persona) == EntitlementState::Granted {
                    json!([{ "name": "game_minecraft" }])
                } else {
                    json!([])
                };
                Self::reply(200, json!({ "items": items }))
            }
            Request::Profile(token) => {
                let Some(persona) = persona_from_token(token, "sim-game-") else {
                    return Ok(Self::reply(401, json!({ "error": "invalid_token" })));
                };
                Self::reply(
                    200,
                    json!({
                        "id": persona.profile_id(),
                        "name": persona.name(),
                    }),
                )
            }
        })
    }
}

fn access_token(persona: Persona, sequence: u64) -> String {
    format!("sim-msa-{}-{sequence}", persona.key())
}

fn refresh_token(persona: Persona, sequence: u64) -> String {
    format!("sim-refresh-{}-{sequence}", persona.key())
}

fn xbox_token(persona: Persona) -> String {
    format!("sim-xbox-{}", persona.key())
}

fn xsts_token(persona: Persona) -> String {
    format!("sim-xsts-{}", persona.key())
}

fn game_token(persona: Persona) -> String {
    format!("sim-game-{}", persona.key())
}

fn persona_from_token(token: &str, prefix: &str) -> Option<Persona> {
    let key = token.strip_prefix(prefix)?.split('-').next()?;
    Persona::from_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthenticatedAccount, DeviceClient, DevicePoll};

    fn client_for(simulation: &Simulation) -> DeviceClient {
        DeviceClient::with_transport(CLIENT_ID, simulation.transport()).expect("simulation client")
    }

    fn begin(client: &DeviceClient) -> crate::DeviceLogin {
        client.begin(|| false).expect("simulated challenge")
    }

    fn authenticate(simulation: &Simulation) -> AuthenticatedAccount {
        let client = client_for(simulation);
        let mut login = begin(&client);
        assert!(simulation.approve());
        login.poll_now();
        match client.poll(&mut login, || false).expect("simulated poll") {
            DevicePoll::Authenticated(account) => account,
            DevicePoll::Pending { .. } => panic!("approved simulation is still pending"),
        }
    }

    #[test]
    fn personas_are_fixed_synthetic_test_identities() {
        assert_eq!(Persona::Forest.name(), "Forest");
        assert_eq!(Persona::Coast.name(), "Coast");
        assert_ne!(Persona::Forest.profile_id(), Persona::Coast.profile_id());
        assert_eq!(
            Persona::from_profile_id(Persona::Forest.profile_id()),
            Some(Persona::Forest)
        );
        assert!(Persona::ALL.iter().all(|persona| {
            persona.name().len() <= 16
                && persona
                    .name()
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }));
    }

    #[test]
    fn the_challenge_stays_pending_until_the_developer_approves_it() {
        let simulation = Simulation::new();
        let client = client_for(&simulation);
        let mut login = begin(&client);
        let challenge = login.challenge();
        assert_eq!(challenge.verification_uri, DEVICE_LOGIN_URI);
        assert!(challenge.user_code.starts_with("TEST-"));
        assert_eq!(simulation.phase(), SimulationPhase::AwaitingApproval);
        assert_eq!(simulation.pending_persona(), Some(Persona::Forest));

        login.poll_now();
        assert!(matches!(
            client.poll(&mut login, || false).expect("pending poll"),
            DevicePoll::Pending { .. }
        ));
        assert!(simulation.approve());
        assert!(!simulation.approve(), "approval is one-shot");
        login.poll_now();
        let DevicePoll::Authenticated(account) =
            client.poll(&mut login, || false).expect("approved poll")
        else {
            panic!("approved simulation stays pending");
        };
        assert_eq!(account.profile().id, Persona::Forest.profile_id());
        assert_eq!(account.profile().name, "Forest");
        assert!(account.refresh_token().is_some());
    }

    #[test]
    fn decline_and_expiry_are_distinct_terminal_outcomes() {
        let simulation = Simulation::new();
        let client = client_for(&simulation);

        let mut login = begin(&client);
        assert!(simulation.decline());
        assert_eq!(simulation.phase(), SimulationPhase::Declined);
        login.poll_now();
        assert_eq!(
            client.poll(&mut login, || false).map(|_| ()),
            Err(AccountError::Declined)
        );

        let mut login = begin(&client);
        assert!(simulation.expire());
        assert_eq!(simulation.phase(), SimulationPhase::Expired);
        login.poll_now();
        assert_eq!(
            client.poll(&mut login, || false).map(|_| ()),
            Err(AccountError::Expired)
        );
    }

    #[test]
    fn two_personas_sign_in_independently() {
        let simulation = Simulation::new();
        let forest = authenticate(&simulation);
        assert_eq!(forest.profile().name, "Forest");

        simulation.set_next_persona(Persona::Coast);
        let coast = authenticate(&simulation);
        assert_eq!(coast.profile().id, Persona::Coast.profile_id());
        assert_eq!(coast.profile().name, "Coast");
        assert_ne!(forest.profile().id, coast.profile().id);
    }

    #[test]
    fn entitlement_denial_and_revocation_keep_their_typed_errors() {
        let simulation = Simulation::new();
        simulation.set_entitlement(Persona::Forest, EntitlementState::Denied);
        let client = client_for(&simulation);
        let mut login = begin(&client);
        assert!(simulation.approve());
        login.poll_now();
        assert_eq!(
            client.poll(&mut login, || false).map(|_| ()),
            Err(AccountError::NotEntitled)
        );

        simulation.set_entitlement(Persona::Forest, EntitlementState::Granted);
        let account = authenticate(&simulation);
        let token = account.refresh_token().expect("refresh token").to_owned();
        simulation.set_entitlement(Persona::Forest, EntitlementState::Revoked);
        assert_eq!(
            client_for(&simulation)
                .refresh(&token, || false)
                .map(|_| ()),
            Err(AccountError::RefreshRejected)
        );
    }
}
