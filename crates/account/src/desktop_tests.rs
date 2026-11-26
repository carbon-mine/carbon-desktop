use super::*;
use crate::vault::MemoryCredentials;
use serde_json::{json, to_value};

const FOREST: Uuid = Uuid::from_u128(2);
const COAST: Uuid = Uuid::from_u128(3);
const CLIENT: Uuid = Uuid::from_u128(1);

fn isolated() -> Accounts {
    Accounts {
        owner: Arc::new(Owner(Arc::new(Core {
            subject: Arc::new(()),
            registration: Some(CLIENT),
            state: Mutex::new(State::new()),
            changed: Condvar::new(),
        }))),
    }
}

fn ticket(accounts: &Accounts, expected: Job) -> Ticket {
    let ticket = accounts.owner.0.state().pending.take().unwrap();
    assert!(ticket.job == expected);
    ticket
}

fn synthetic(id: Uuid, token: &str) -> AuthenticatedAccount {
    AuthenticatedAccount::synthetic(id, if id == FOREST { "Forest" } else { "Coast" }, token)
}

#[test]
fn missing_registration_is_idle_and_cannot_open_a_browser() {
    let accounts = Accounts::start(None, None);
    accounts.begin();
    accounts.cancel();
    accounts.sign_out();
    let view = accounts.snapshot();
    assert_eq!(view.state, AccountState::Unavailable);
    assert!(accounts.open_browser().is_err());
    assert!(view.name.is_none() && view.user_code.is_none());
}

#[test]
fn cancellation_discards_the_challenge_and_stale_completions() {
    let accounts = isolated();
    let core = &accounts.owner.0;
    accounts.begin();
    let original = ticket(&accounts, Job::Add);
    let mut state = core.state();
    state.browser = Some(BrowserPage {
        address: "https://microsoft.com/devicelogin".into(),
        deadline: Instant::now() + Duration::from_secs(60),
        code: "ABCD-EFGH".into(),
    });
    drop(state);
    accounts.cancel();
    accounts.begin();
    add(core, original, synthetic(FOREST, "old"), None);
    assert_eq!(accounts.snapshot().state, AccountState::Starting);
    assert!(accounts.snapshot().accounts.is_empty());
    assert!(accounts.open_browser().is_err());
    assert!(core.state().pending.is_some_and(|next| next != original));
}

#[test]
fn cancellation_interrupts_poll_delay_and_removal_cannot_be_cancelled() {
    let accounts = isolated();
    let core = Arc::clone(&accounts.owner.0);
    accounts.begin();
    let adding = ticket(&accounts, Job::Add);
    let (ready, waiting) = std::sync::mpsc::channel();
    let waiter = {
        let core = Arc::clone(&core);
        thread::spawn(move || {
            ready.send(()).unwrap();
            core.wait(adding, Duration::from_secs(300))
        })
    };
    waiting.recv().unwrap();
    accounts.cancel();
    assert!(!waiter.join().unwrap());

    accounts.begin();
    add(
        &core,
        ticket(&accounts, Job::Add),
        synthetic(FOREST, "forest"),
        None,
    );
    accounts.sign_out();
    accounts.cancel();
    accounts.begin();
    remove(&core, ticket(&accounts, Job::Remove(FOREST)), FOREST, None);
    assert!(accounts.snapshot().accounts.is_empty());
}

#[test]
fn serialized_challenge_contains_only_public_fields_and_expires() {
    let accounts = isolated();
    let mut state = accounts.owner.0.state();
    state.status = AccountState::AwaitingBrowser;
    state.message = "Open Microsoft sign-in.".into();
    state.browser = Some(BrowserPage {
        address: "https://microsoft.com/devicelogin".into(),
        deadline: Instant::now() + Duration::from_secs(60),
        code: "ABCD-EFGH".into(),
    });
    assert_eq!(
        to_value(state.view()).unwrap(),
        json!({
            "state": "awaitingBrowser",
            "message": "Open Microsoft sign-in.",
            "name": null,
            "userCode": "ABCD-EFGH",
            "accounts": [],
            "activeAccount": null,
            "cancellable": false,
            "removalRetry": null
        })
    );
    state.browser.as_mut().unwrap().deadline = Instant::now();
    drop(state);
    assert!(accounts.snapshot().user_code.is_none());
    assert!(accounts.open_browser().is_err());
}

#[test]
fn two_accounts_restore_switch_rotate_and_remove_independently() {
    let root = tempfile::tempdir().unwrap();
    let credentials = MemoryCredentials::default();
    let vault = credentials.vault(root.path(), CLIENT);
    let accounts = isolated();
    let core = &accounts.owner.0;
    for (id, token) in [(FOREST, "forest-old"), (COAST, "coast-old")] {
        accounts.begin();
        add(
            core,
            ticket(&accounts, Job::Add),
            synthetic(id, token),
            Some(&vault),
        );
    }
    assert_eq!(accounts.snapshot().active_account, Some(COAST));
    assert!(accounts
        .snapshot()
        .accounts
        .iter()
        .all(|account| account.verified));

    accounts.switch(FOREST);
    select_current(
        core,
        ticket(&accounts, Job::Switch(FOREST)),
        FOREST,
        Some(&vault),
    );
    assert_eq!(vault.load().unwrap().active, Some(FOREST));
    drop(accounts);
    drop(vault);

    let vault = credentials.vault(root.path(), CLIENT);
    let restarted = isolated();
    let core = &restarted.owner.0;
    core.state().schedule(Job::Restore, "restore");
    restore(
        core,
        ticket(&restarted, Job::Restore),
        Some(&vault),
        |token| {
            assert_eq!(token, "forest-old");
            Ok(synthetic(FOREST, "forest-new"))
        },
    );
    let restored = restarted.snapshot();
    assert_eq!(restored.active_account, Some(FOREST));
    assert_eq!(restored.state, AccountState::SignedIn);
    assert!(restored
        .accounts
        .iter()
        .any(|profile| profile.id == FOREST && profile.verified));
    assert!(restored
        .accounts
        .iter()
        .any(|profile| profile.id == COAST && !profile.verified));

    restarted.switch(COAST);
    verify(
        core,
        ticket(&restarted, Job::Switch(COAST)),
        COAST,
        Some(&vault),
        |token| {
            assert_eq!(token, "coast-old");
            Ok(synthetic(COAST, "coast-new"))
        },
    );
    assert!(restarted
        .snapshot()
        .accounts
        .iter()
        .all(|profile| profile.verified));

    restarted.refresh_selected();
    verify(
        core,
        ticket(&restarted, Job::Refresh(COAST)),
        COAST,
        Some(&vault),
        |_| Err(AccountError::NotEntitled),
    );
    assert_eq!(restarted.snapshot().state, AccountState::Error);
    assert!(core.state().verified(FOREST) && !core.state().verified(COAST));
    assert!(vault.read(COAST).unwrap().is_some());

    restarted.refresh_selected();
    verify(
        core,
        ticket(&restarted, Job::Refresh(COAST)),
        COAST,
        Some(&vault),
        |token| {
            assert_eq!(token, "coast-new");
            Err(AccountError::RefreshRejected)
        },
    );
    assert!(vault.read(COAST).unwrap().is_none());
    assert_eq!(restarted.snapshot().active_account, Some(FOREST));
    assert!(core.state().verified(FOREST));

    restarted.sign_out_account(FOREST);
    remove(
        core,
        ticket(&restarted, Job::Remove(FOREST)),
        FOREST,
        Some(&vault),
    );
    assert!(restarted.snapshot().accounts.is_empty());
    assert!(vault.load().unwrap().accounts.is_empty());
}

#[test]
fn wrong_profile_or_superseded_refresh_cannot_replace_a_vault_slot() {
    let root = tempfile::tempdir().unwrap();
    let credentials = MemoryCredentials::default();
    let vault = credentials.vault(root.path(), CLIENT);
    let accounts = isolated();
    let core = &accounts.owner.0;
    accounts.begin();
    add(
        core,
        ticket(&accounts, Job::Add),
        synthetic(FOREST, "first"),
        Some(&vault),
    );

    accounts.refresh_selected();
    verify(
        core,
        ticket(&accounts, Job::Refresh(FOREST)),
        FOREST,
        Some(&vault),
        |_| Ok(synthetic(COAST, "wrong-profile")),
    );
    assert_eq!(accounts.snapshot().state, AccountState::Error);
    assert_eq!(vault.read(FOREST).unwrap().unwrap().as_str(), "first");
    assert!(vault.read(COAST).unwrap().is_none());

    accounts.refresh_selected();
    verify(
        core,
        ticket(&accounts, Job::Refresh(FOREST)),
        FOREST,
        Some(&vault),
        |_| {
            accounts.cancel();
            Ok(synthetic(FOREST, "late"))
        },
    );
    assert_eq!(vault.read(FOREST).unwrap().unwrap().as_str(), "first");
    assert!(!accounts.snapshot().accounts[0].verified);
}

#[test]
fn cancelling_add_preserves_the_previously_verified_account() {
    let accounts = isolated();
    let core = &accounts.owner.0;
    accounts.begin();
    add(
        core,
        ticket(&accounts, Job::Add),
        synthetic(FOREST, "forest"),
        None,
    );
    accounts.begin();
    accounts.cancel();
    assert_eq!(accounts.snapshot().state, AccountState::SignedIn);
    assert_eq!(accounts.snapshot().active_account, Some(FOREST));
}

#[test]
fn retry_removal_is_explicit_and_signout_has_no_cancel_action() {
    let accounts = isolated();
    let core = &accounts.owner.0;
    accounts.begin();
    assert!(accounts.snapshot().cancellable);
    add(
        core,
        ticket(&accounts, Job::Add),
        synthetic(FOREST, "forest"),
        None,
    );
    {
        let mut state = core.state();
        state.retry_remove = Some(FOREST);
        state.finish(AccountState::Error, "secure storage unavailable");
    }
    assert_eq!(accounts.snapshot().removal_retry, Some(FOREST));

    accounts.begin();
    ticket(&accounts, Job::Add);
    accounts.cancel();
    accounts.sign_out_account(FOREST);
    let removing = ticket(&accounts, Job::Remove(FOREST));
    assert!(!accounts.snapshot().cancellable);
    remove(core, removing, FOREST, None);
    assert!(accounts.snapshot().removal_retry.is_none());
}

#[test]
fn play_blocker_uses_real_service_state_and_remembers_definitive_entitlement_denial() {
    let accounts = isolated();
    let core = &accounts.owner.0;
    let mode = PlayMode::OfflineSinglePlayer;
    assert_eq!(
        accounts.play_blocker(mode),
        PlayBlocker::AccountSelectionRequired
    );

    accounts.begin();
    assert_eq!(
        accounts.play_blocker(mode),
        PlayBlocker::AccountChangePending
    );
    add(
        core,
        ticket(&accounts, Job::Add),
        synthetic(FOREST, "forest"),
        None,
    );
    assert_eq!(
        accounts.play_blocker(mode),
        PlayBlocker::OfflinePolicyUnavailable
    );
    assert_eq!(
        accounts.prepare_launch(mode, None).err(),
        Some(PlayBlocker::OfflinePolicyUnavailable)
    );

    accounts.refresh_selected();
    verify(
        core,
        ticket(&accounts, Job::Refresh(FOREST)),
        FOREST,
        None,
        |_| Err(AccountError::NotEntitled),
    );
    assert_eq!(accounts.play_blocker(mode), PlayBlocker::EntitlementDenied);
    accounts.refresh_selected();
    verify(
        core,
        ticket(&accounts, Job::Refresh(FOREST)),
        FOREST,
        None,
        |_| Err(AccountError::TemporarilyUnavailable),
    );
    assert_eq!(accounts.play_blocker(mode), PlayBlocker::EntitlementDenied);

    accounts.refresh_selected();
    verify(
        core,
        ticket(&accounts, Job::Refresh(FOREST)),
        FOREST,
        None,
        |_| Ok(synthetic(FOREST, "updated")),
    );
    assert_eq!(
        accounts.play_blocker(mode),
        PlayBlocker::OfflinePolicyUnavailable
    );
}
