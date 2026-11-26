use crate::{
    play::{self, Clock, Installation, LaunchDecision, PlayBlocker, PlayMode, Selection},
    vault::{SavedAccounts, Secret, Vault, ACCOUNT_LIMIT},
    AccountError, AuthenticatedAccount, DeviceClient, DevicePoll,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountState {
    Unavailable,
    SignedOut,
    Starting,
    AwaitingBrowser,
    SignedIn,
    Error,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: Uuid,
    pub name: String,
    pub verified: bool,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub state: AccountState,
    pub message: String,
    pub name: Option<String>,
    pub user_code: Option<String>,
    pub accounts: Vec<AccountSummary>,
    pub active_account: Option<Uuid>,
    pub cancellable: bool,
    pub removal_retry: Option<Uuid>,
}

#[derive(Clone)]
pub struct Accounts {
    owner: Arc<Owner>,
}

struct Owner(Arc<Core>);

impl Drop for Owner {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.stopping = true;
        self.0.changed.notify_one();
    }
}

struct Core {
    subject: Arc<()>,
    registration: Option<Uuid>,
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Job {
    Restore,
    Add,
    Switch(Uuid),
    Refresh(Uuid),
    Remove(Uuid),
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Ticket {
    generation: u64,
    job: Job,
}

struct BrowserPage {
    address: String,
    deadline: Instant,
    code: String,
}

struct State {
    status: AccountState,
    message: String,
    ticket: Option<Ticket>,
    pending: Option<Ticket>,
    generation: u64,
    stopping: bool,
    committing: bool,
    retry_remove: Option<Uuid>,
    browser: Option<BrowserPage>,
    saved: SavedAccounts,
    active: Option<Uuid>,
    live: BTreeMap<Uuid, AuthenticatedAccount>,
    entitlement_denials: BTreeSet<Uuid>,
}

const SIGNED_OUT: &str = "Sign in with the Microsoft account that owns Minecraft: Java Edition.";
const SIGNED_IN: &str = "Minecraft: Java Edition account verified.";
const VAULT_UNAVAILABLE: &str =
    "Secure credential storage is unavailable. Sign-in can continue for this session.";
const VAULT_WRITE_FAILED: &str =
    "Signed in for this session. Secure storage did not save this account.";
const VAULT_CHANGED: &str =
    "The saved Microsoft account changed or could not be confirmed. Sign in again.";
const ACCOUNT_REMOVED: &str = "The Microsoft account was removed from Carbon Desktop.";
const NEED_VERIFY: &str = "Verify this Microsoft account to check its Java Edition entitlement.";
const ADD_LIMIT: &str = "Remove a Microsoft account before adding another one.";

impl State {
    fn new() -> Self {
        Self {
            status: AccountState::SignedOut,
            message: SIGNED_OUT.into(),
            ticket: None,
            pending: None,
            generation: 0,
            stopping: false,
            committing: false,
            retry_remove: None,
            browser: None,
            saved: SavedAccounts::default(),
            active: None,
            live: BTreeMap::new(),
            entitlement_denials: BTreeSet::new(),
        }
    }

    fn schedule(&mut self, job: Job, message: &str) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("account revision exhausted");
        let ticket = Ticket {
            generation: self.generation,
            job,
        };
        self.ticket = Some(ticket);
        self.pending = Some(ticket);
        self.browser = None;
        self.committing = false;
        self.status = AccountState::Starting;
        self.message = message.into();
    }

    fn finish(&mut self, status: AccountState, message: impl Into<String>) {
        self.ticket = None;
        self.pending = None;
        self.browser = None;
        self.committing = false;
        self.status = status;
        self.message = message.into();
    }

    fn current(&self, ticket: Ticket) -> bool {
        !self.stopping && self.ticket == Some(ticket)
    }

    fn cancellable(&self) -> bool {
        !self.committing
            && self
                .ticket
                .is_some_and(|ticket| !matches!(ticket.job, Job::Remove(_)))
    }

    fn verified(&self, id: Uuid) -> bool {
        self.live
            .get(&id)
            .is_some_and(AuthenticatedAccount::is_current)
    }

    fn knows(&self, id: Uuid) -> bool {
        self.saved.includes(id) || self.live.contains_key(&id)
    }

    fn stable_status(&self) -> AccountState {
        if self.active.is_some_and(|id| self.verified(id)) {
            AccountState::SignedIn
        } else {
            AccountState::SignedOut
        }
    }

    fn replace_saved(&mut self, saved: SavedAccounts) {
        self.live
            .retain(|id, _| !self.saved.includes(*id) || saved.includes(*id));
        self.entitlement_denials
            .retain(|id| saved.includes(*id) || self.live.contains_key(id));
        self.saved = saved;
    }

    fn view(&self) -> AccountView {
        let mut accounts: Vec<_> = self
            .saved
            .accounts
            .iter()
            .map(|profile| AccountSummary {
                id: profile.id,
                name: profile.name.clone(),
                verified: self.verified(profile.id),
            })
            .collect();
        accounts.extend(
            self.live
                .iter()
                .filter(|(id, _)| !self.saved.includes(**id))
                .map(|(id, account)| AccountSummary {
                    id: *id,
                    name: account.profile().name.clone(),
                    verified: account.is_current(),
                }),
        );
        let name = accounts
            .iter()
            .find(|profile| Some(profile.id) == self.active)
            .map(|profile| profile.name.clone());
        let (state, message) = if self.status == AccountState::SignedIn
            && self.stable_status() != AccountState::SignedIn
        {
            (AccountState::SignedOut, NEED_VERIFY)
        } else {
            (self.status, self.message.as_str())
        };
        AccountView {
            state,
            message: message.into(),
            name,
            user_code: self
                .browser
                .as_ref()
                .filter(|page| Instant::now() < page.deadline)
                .map(|page| page.code.clone()),
            accounts,
            active_account: self.active,
            cancellable: self.cancellable(),
            removal_retry: self.retry_remove,
        }
    }
}

impl Core {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn launch_selection<'a>(&'a self, state: &State) -> Selection<'a> {
        Selection {
            owner: &self.subject,
            revision: state.generation,
            registration: self.registration,
            profile: state.active,
            online_verified: state.active.is_some_and(|id| state.verified(id)),
            entitlement_denied: state
                .active
                .is_some_and(|id| state.entitlement_denials.contains(&id)),
            pending: state.ticket.is_some() || state.stopping,
        }
    }

    fn complete(&self, ticket: Ticket, status: AccountState, message: impl Into<String>) {
        let mut state = self.state();
        if state.current(ticket) {
            state.finish(status, message);
        }
    }

    fn wait(&self, ticket: Ticket, delay: Duration) -> bool {
        let (state, _) = self
            .changed
            .wait_timeout_while(self.state(), delay, |state| state.current(ticket))
            .unwrap_or_else(|poison| poison.into_inner());
        state.current(ticket)
    }

    /// Linearize an authenticated result before performing any vault mutation.
    /// Cancellation remains available during network and browser authorization.
    fn commit(&self, ticket: Ticket) -> bool {
        let mut state = self.state();
        if !state.current(ticket) {
            return false;
        }
        state.browser = None;
        state.committing = true;
        state.status = AccountState::Starting;
        state.message = "Finishing the Microsoft account change.".into();
        true
    }
}

impl Accounts {
    pub fn new(root: &Path) -> Self {
        Self::start(Some(root), configured_client())
    }

    pub fn session_only() -> Self {
        Self::start(None, configured_client())
    }

    fn start(root: Option<&Path>, client_id: Option<Uuid>) -> Self {
        let mut state = State::new();
        match client_id {
            None => state.finish(
                AccountState::Unavailable,
                "Microsoft sign-in requires Carbon's approved application registration.",
            ),
            Some(_) if root.is_some() => {
                state.schedule(Job::Restore, "Checking for saved Microsoft accounts.")
            }
            Some(_) => {}
        };
        let core = Arc::new(Core {
            subject: Arc::new(()),
            registration: client_id,
            state: Mutex::new(state),
            changed: Condvar::new(),
        });
        if let Some(client_id) = client_id {
            let worker = Arc::clone(&core);
            let root = root.map(Path::to_path_buf);
            if thread::Builder::new()
                .name("carbon-accounts".into())
                .spawn(move || run(&worker, root.as_deref(), client_id))
                .is_err()
            {
                core.state().finish(
                    AccountState::Unavailable,
                    "The Microsoft sign-in worker could not start.",
                );
            }
        }
        Self {
            owner: Arc::new(Owner(core)),
        }
    }

    pub fn snapshot(&self) -> AccountView {
        self.owner.0.state().view()
    }

    /// Reports a fixed, redacted reason for this build's unavailable play mode.
    pub fn play_blocker(&self, mode: PlayMode) -> PlayBlocker {
        let core = &self.owner.0;
        let state = core.state();
        play::missing_evidence(core.launch_selection(&state), mode)
    }

    /// Requests a process-local typed decision from trusted account evidence.
    /// Provider-approved offline and scoped online evidence are unavailable in this build.
    pub fn prepare_launch(
        &self,
        mode: PlayMode,
        installation: Option<Installation>,
    ) -> Result<LaunchDecision, PlayBlocker> {
        let core = &self.owner.0;
        let state = core.state();
        play::issue(
            core.launch_selection(&state),
            mode,
            installation,
            None,
            Clock::now(),
        )
    }

    /// Runs the final bounded native spawn admission under the account state lock.
    /// The runner must already hold the exact protected installation lease. Keep
    /// blocking waits, downloads, reentrant account calls, and IPC outside this callback.
    pub fn with_launch_authority<T>(
        &self,
        decision: LaunchDecision,
        installation: Option<Installation>,
        start: impl FnOnce() -> T,
    ) -> Result<T, PlayBlocker> {
        let core = &self.owner.0;
        let state = core.state();
        play::admit(
            decision,
            core.launch_selection(&state),
            installation,
            None,
            Clock::now(),
            start,
        )
    }

    pub fn begin(&self) {
        let core = &self.owner.0;
        let mut state = core.state();
        if state.ticket.is_some() || state.status == AccountState::Unavailable {
            return;
        }
        if state.view().accounts.len() < ACCOUNT_LIMIT {
            state.schedule(Job::Add, "Starting Microsoft sign-in.");
        } else {
            state.finish(AccountState::Error, ADD_LIMIT);
            return;
        }
        core.changed.notify_one();
    }

    pub fn cancel(&self) {
        let core = &self.owner.0;
        let mut state = core.state();
        if state.cancellable() {
            let status = state.stable_status();
            state.finish(status, AccountError::Cancelled.to_string());
            core.changed.notify_one();
        }
    }

    pub fn switch(&self, id: Uuid) {
        self.profile_job(Some(id), Job::Switch, "Switching Microsoft accounts.");
    }

    pub fn refresh_selected(&self) {
        self.profile_job(None, Job::Refresh, "Verifying the Microsoft account.");
    }

    pub fn sign_out(&self) {
        self.profile_job(None, Job::Remove, "Removing the Microsoft account.");
    }

    pub fn sign_out_account(&self, id: Uuid) {
        self.profile_job(Some(id), Job::Remove, "Removing the Microsoft account.");
    }

    fn profile_job(&self, id: Option<Uuid>, job: impl FnOnce(Uuid) -> Job, message: &str) {
        let core = &self.owner.0;
        let mut state = core.state();
        if state.ticket.is_some() {
            return;
        }
        if let Some(id) = id.or(state.active).filter(|id| state.knows(*id)) {
            state.schedule(job(id), message);
            core.changed.notify_one();
        }
    }

    pub fn open_browser(&self) -> Result<(), String> {
        let address = self
            .owner
            .0
            .state()
            .browser
            .as_ref()
            .filter(|page| Instant::now() < page.deadline)
            .map(|page| page.address.clone())
            .ok_or_else(|| {
                "There is no current Microsoft sign-in page. Start sign-in again.".to_owned()
            })?;
        // DeviceClient validates the URI before the worker publishes it.
        open::that_detached(address).map_err(|_| {
            "The system browser could not open. Try again or use Microsoft's device sign-in page.".to_owned()
        })
    }
}

fn configured_client() -> Option<Uuid> {
    option_env!("CARBON_MICROSOFT_CLIENT_ID")
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|id| !id.is_nil())
}

fn run(core: &Core, root: Option<&Path>, client_id: Uuid) {
    let vault = root.map(|root| Vault::new(root, client_id));
    let vault = vault.as_ref();
    loop {
        let ticket = {
            let mut state = core
                .changed
                .wait_while(core.state(), |state| {
                    !state.stopping && state.pending.is_none()
                })
                .unwrap_or_else(|poison| poison.into_inner());
            if state.stopping {
                return;
            }
            state.pending.take().expect("worker has a queued operation")
        };
        let refresh = |token: &str| {
            DeviceClient::new(client_id)
                .and_then(|client| client.refresh(token, || !core.state().current(ticket)))
        };
        match ticket.job {
            Job::Restore => restore(core, ticket, vault, refresh),
            Job::Add => sign_in(core, ticket, client_id, vault),
            Job::Switch(id) => {
                if core.state().verified(id) {
                    select_current(core, ticket, id, vault);
                } else {
                    verify(core, ticket, id, vault, refresh);
                }
            }
            Job::Refresh(id) => verify(core, ticket, id, vault, refresh),
            Job::Remove(id) => remove(core, ticket, id, vault),
        }
    }
}

fn restore(
    core: &Core,
    ticket: Ticket,
    vault: Option<&Vault>,
    refresh: impl FnOnce(&str) -> Result<AuthenticatedAccount, AccountError>,
) {
    let saved = match vault.map_or_else(|| Ok(SavedAccounts::default()), Vault::load) {
        Ok(saved) => saved,
        Err(()) => {
            core.complete(ticket, AccountState::SignedOut, VAULT_UNAVAILABLE);
            return;
        }
    };
    let id = saved.active;
    {
        let mut state = core.state();
        if !state.current(ticket) {
            return;
        }
        state.active = id;
        state.replace_saved(saved);
    }
    if let Some(id) = id {
        verify(core, ticket, id, vault, refresh);
    } else {
        core.complete(ticket, AccountState::SignedOut, SIGNED_OUT);
    }
}

fn sign_in(core: &Core, ticket: Ticket, client_id: Uuid, vault: Option<&Vault>) {
    let cancelled = || !core.state().current(ticket);
    let result = (|| {
        if cancelled() {
            return Err(AccountError::Cancelled);
        }
        let client = DeviceClient::new(client_id)?;
        let mut login = client.begin(cancelled)?;
        {
            let mut state = core.state();
            if !state.current(ticket) {
                return Err(AccountError::Cancelled);
            }
            let challenge = login.challenge();
            state.browser = Some(BrowserPage {
                address: challenge.verification_uri.clone(),
                deadline: login.deadline(),
                code: challenge.user_code.clone(),
            });
            state.status = AccountState::AwaitingBrowser;
            state.message = "Open the browser and enter this code to authorize Carbon.".into();
        }
        loop {
            match client.poll(&mut login, cancelled)? {
                DevicePoll::Pending { retry_after } if core.wait(ticket, retry_after) => {}
                DevicePoll::Pending { .. } => return Err(AccountError::Cancelled),
                DevicePoll::Authenticated(account) => return Ok(account),
            }
        }
    })();
    match result {
        Ok(account) => add(core, ticket, account, vault),
        Err(error) => core.complete(ticket, AccountState::Error, error.to_string()),
    }
}

fn add(core: &Core, ticket: Ticket, account: AuthenticatedAccount, vault: Option<&Vault>) {
    if !core.commit(ticket) {
        return;
    }
    let persisted = vault.and_then(|vault| {
        let token = account.refresh_token()?;
        vault.add(account.profile().into(), token).ok()
    });
    let message = if vault.is_some() && persisted.is_none() {
        VAULT_WRITE_FAILED
    } else {
        SIGNED_IN
    };
    publish(core, ticket, account, persisted, message);
}

fn publish(
    core: &Core,
    ticket: Ticket,
    account: AuthenticatedAccount,
    persisted: Option<SavedAccounts>,
    message: &str,
) {
    let mut state = core.state();
    if !state.current(ticket) {
        return;
    }
    if let Some(persisted) = persisted {
        state.replace_saved(persisted);
    }
    let id = account.profile().id;
    state.active = Some(id);
    state.entitlement_denials.remove(&id);
    state.live.insert(id, account);
    let status = state.stable_status();
    state.finish(status, message);
}

fn select_current(core: &Core, ticket: Ticket, id: Uuid, vault: Option<&Vault>) {
    if !core.commit(ticket) {
        return;
    }
    let persisted = if core.state().saved.includes(id) {
        match vault.and_then(|vault| vault.select(id).ok().flatten()) {
            Some(saved) => Some(saved),
            None => {
                core.complete(ticket, AccountState::Error, VAULT_CHANGED);
                return;
            }
        }
    } else {
        None
    };
    let mut state = core.state();
    if state.current(ticket) {
        if let Some(saved) = persisted {
            state.replace_saved(saved);
        }
        state.active = Some(id);
        let status = state.stable_status();
        state.finish(
            status,
            if status == AccountState::SignedIn {
                SIGNED_IN
            } else {
                NEED_VERIFY
            },
        );
    }
}

fn verify(
    core: &Core,
    ticket: Ticket,
    id: Uuid,
    vault: Option<&Vault>,
    refresh: impl FnOnce(&str) -> Result<AuthenticatedAccount, AccountError>,
) {
    let (is_saved, memory) = {
        let state = core.state();
        if !state.current(ticket) {
            return;
        }
        (
            state.saved.includes(id),
            state
                .live
                .get(&id)
                .and_then(AuthenticatedAccount::refresh_token)
                .map(|token| Secret::new(token.to_owned())),
        )
    };
    let vault = vault.filter(|_| is_saved);
    let token = match vault.map_or(Ok(memory), |vault| vault.read(id)) {
        Ok(Some(token)) => token,
        Ok(None) => {
            failed_verification(core, ticket, id, vault, None, false, NEED_VERIFY);
            return;
        }
        Err(()) => {
            failed_verification(core, ticket, id, vault, None, false, VAULT_UNAVAILABLE);
            return;
        }
    };
    if !core.state().current(ticket) {
        return;
    }
    let result = refresh(&token).and_then(|account| {
        if account.profile().id == id {
            Ok(account)
        } else {
            Err(AccountError::InvalidResponse)
        }
    });
    match result {
        Ok(account) => {
            if !core.commit(ticket) {
                return;
            }
            let saved = if let Some(vault) = vault {
                let next = account.refresh_token().unwrap_or(&token);
                let required_active = (ticket.job == Job::Restore).then_some(id);
                match vault.update(
                    account.profile().into(),
                    &token,
                    next,
                    true,
                    required_active,
                ) {
                    Ok(Some(saved)) => Some(saved),
                    _ => {
                        failed_verification(core, ticket, id, None, None, false, VAULT_CHANGED);
                        return;
                    }
                }
            } else {
                None
            };
            publish(core, ticket, account, saved, SIGNED_IN);
        }
        Err(error) => {
            let rejected = (error == AccountError::RefreshRejected).then_some(token.as_str());
            let denied = matches!(
                error,
                AccountError::NotEntitled | AccountError::RefreshRejected
            );
            failed_verification(
                core,
                ticket,
                id,
                vault,
                rejected,
                denied,
                &error.to_string(),
            );
        }
    }
}

fn failed_verification(
    core: &Core,
    ticket: Ticket,
    id: Uuid,
    vault: Option<&Vault>,
    rejected: Option<&str>,
    denied: bool,
    message: &str,
) {
    if !core.commit(ticket) {
        return;
    }
    let saved = if let Some(vault) = vault {
        if let Some(rejected) = rejected {
            vault.remove(id, Some(rejected)).ok().flatten()
        } else if matches!(ticket.job, Job::Switch(_)) {
            vault.select(id).ok().flatten()
        } else {
            None
        }
    } else {
        None
    };
    let mut state = core.state();
    if state.current(ticket) {
        if denied {
            state.entitlement_denials.insert(id);
        }
        // Keep a session-only row for retry after transient failures, with its verification invalidated.
        if rejected.is_some() || state.saved.includes(id) {
            state.live.remove(&id);
        } else if let Some(account) = state.live.get_mut(&id) {
            account.invalidate_verification();
        }
        if let Some(saved) = saved {
            state.replace_saved(saved);
        }
        state.active = if state.knows(id) {
            Some(id)
        } else {
            state
                .saved
                .active
                .or_else(|| state.live.keys().next().copied())
        };
        state.finish(AccountState::Error, message);
    }
}

fn remove(core: &Core, ticket: Ticket, id: Uuid, vault: Option<&Vault>) {
    let saved = if core.state().saved.includes(id) {
        vault.map(|vault| vault.remove(id, None)).unwrap_or(Err(()))
    } else {
        Ok(None)
    };
    let mut state = core.state();
    if !state.current(ticket) {
        return;
    }
    state.live.remove(&id);
    state.entitlement_denials.remove(&id);
    match saved {
        Ok(saved) => {
            if state.retry_remove == Some(id) {
                state.retry_remove = None;
            }
            if let Some(saved) = saved {
                state.replace_saved(saved);
            }
            if state.active.is_some_and(|active| !state.knows(active)) {
                state.active = state
                    .saved
                    .active
                    .or_else(|| state.live.keys().next().copied());
            }
            let status = state.stable_status();
            state.finish(status, ACCOUNT_REMOVED);
        }
        Err(()) => {
            state.retry_remove = Some(id);
            state.finish(
                AccountState::Error,
                "Secure storage could not remove this account. Try again.",
            );
        }
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
