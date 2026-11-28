use super::*;
use crate::{
    client,
    vault::{MemoryCredentials, SavedAccounts, SavedProfile},
};
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
            backend: Backend::Real,
        }))),
        root: None,
        compiled: Some(CLIENT),
        source: Some(RegistrationSource::Compiled),
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
fn compiled_configuration_wins_over_a_stored_custom_client() {
    let root = tempfile::tempdir().unwrap();
    let custom = Uuid::from_u128(9);
    client::store(root.path(), custom).unwrap();

    let accounts = Accounts::start(Some(root.path()), Some(CLIENT));
    assert_eq!(accounts.owner.0.registration, Some(CLIENT));
    assert_eq!(accounts.source, Some(RegistrationSource::Compiled));
    assert!(accounts.compiled_client_configured());
    assert!(!accounts.custom_client_allowed());
    assert!(!accounts.custom_client_configured());
    assert_eq!(client::load(root.path()), Some(custom));
    assert!(accounts.set_custom_client(&custom.to_string()).is_err());
    assert!(
        accounts.clear_custom_client().is_err(),
        "a compiled build cannot clear a per-install setting either"
    );
    assert_eq!(client::load(root.path()), Some(custom));
}

#[test]
fn custom_client_is_validated_persisted_and_used_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let no_config = Accounts::start(Some(root.path()), None);
    assert_eq!(no_config.snapshot().state, AccountState::Unavailable);
    assert!(no_config.custom_client_allowed());
    assert!(!no_config.custom_client_active());

    assert!(no_config.set_custom_client("not-a-uuid").is_err());
    assert!(no_config
        .set_custom_client(&Uuid::nil().to_string())
        .is_err());
    assert!(client::load(root.path()).is_none());

    let custom = Uuid::from_u128(7);
    no_config
        .set_custom_client(&format!("  {custom}  "))
        .unwrap();
    let bytes = std::fs::read_to_string(root.path().join("account-client.json")).unwrap();
    assert_eq!(bytes, format!(r#"{{"clientId":"{custom}"}}"#));
    assert!(!bytes.contains("token") && !bytes.contains("secret"));
    assert!(no_config.custom_client_configured());
    assert!(
        !no_config.custom_client_active(),
        "a restart applies the setting"
    );
    assert!(no_config.custom_client_pending_restart());
    drop(no_config);

    let restarted = Accounts::start(Some(root.path()), None);
    assert_eq!(restarted.owner.0.registration, Some(custom));
    assert_eq!(restarted.source, Some(RegistrationSource::Custom));
    assert!(restarted.custom_client_active());
    assert!(restarted.custom_client_configured());
    assert!(!restarted.custom_client_pending_restart());

    restarted.clear_custom_client().unwrap();
    assert!(client::load(root.path()).is_none());
    assert!(!restarted.custom_client_configured());
    assert!(
        restarted.custom_client_active(),
        "the running worker keeps its original registration until restart"
    );
    assert!(restarted.custom_client_pending_restart());
    drop(restarted);

    let cleared = Accounts::start(Some(root.path()), None);
    assert_eq!(cleared.owner.0.registration, None);
    assert_eq!(cleared.snapshot().state, AccountState::Unavailable);
    assert!(!cleared.custom_client_pending_restart());
}

#[test]
fn stored_client_setting_is_bounded_and_ignores_unusable_values() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("account-client.json");
    assert!(client::load(root.path()).is_none(), "missing setting");

    let custom = Uuid::from_u128(11);
    let mut oversized = format!(r#"{{"clientId":"{custom}"}}"#);
    oversized.push_str(&" ".repeat(2048));
    let invalid = [
        String::new(),
        "{".into(),
        "null".into(),
        r#"{"clientId":null}"#.into(),
        r#"{"clientId":"00000000-0000-0000-0000-000000000000"}"#.into(),
        r#"{"clientId":"00000000-0000-0000-0000-000000000002","extra":true}"#.into(),
        r#"{"id":"00000000-0000-0000-0000-000000000002"}"#.into(),
        "x".repeat(2048),
        oversized,
    ];
    for text in invalid {
        std::fs::write(&path, &text).unwrap();
        assert!(client::load(root.path()).is_none(), "accepted {text:?}");
    }

    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(
        client::load(root.path()).is_none(),
        "a directory is not a setting file"
    );
    std::fs::remove_dir(&path).unwrap();

    std::fs::write(&path, format!(r#"{{"clientId":"{custom}"}}"#)).unwrap();
    assert_eq!(client::load(root.path()), Some(custom));
}

#[test]
fn a_changed_client_cannot_reuse_the_previous_vault_namespace() {
    let root = tempfile::tempdir().unwrap();
    let credentials = MemoryCredentials::default();
    let first_client = Uuid::from_u128(1);
    let second_client = Uuid::from_u128(2);
    let first = credentials.vault(root.path(), first_client);
    let second = credentials.vault(root.path(), second_client);
    first
        .add(
            SavedProfile {
                id: FOREST,
                name: "Forest".into(),
            },
            "private-token",
        )
        .unwrap();
    assert_eq!(
        first.read(FOREST).unwrap().unwrap().as_str(),
        "private-token"
    );

    // The second registration lists the same public account row but must not
    // resolve the credential slot keyed to the first registration and app root.
    let registry = root
        .path()
        .join(format!("accounts-{}.json", second_client.simple()));
    let rows = SavedAccounts {
        active: Some(FOREST),
        accounts: vec![SavedProfile {
            id: FOREST,
            name: "Forest".into(),
        }],
    };
    std::fs::write(&registry, serde_json::to_vec(&rows).unwrap()).unwrap();
    assert!(second.load().unwrap().includes(FOREST));
    assert!(
        second.read(FOREST).unwrap().is_none(),
        "credential slots include the client ID, not only the app root"
    );
}

#[test]
fn unavailable_and_signed_out_copy_avoids_application_review_wording() {
    let root = tempfile::tempdir().unwrap();
    let unavailable = Accounts::start(Some(root.path()), None).snapshot();
    assert_eq!(unavailable.state, AccountState::Unavailable);
    let signed_out = State::new().view().message;
    for text in [unavailable.message.as_str(), signed_out.as_str()] {
        let text = text.to_lowercase();
        for forbidden in ["approv", "review", "pending", "development", "waiting"] {
            assert!(!text.contains(forbidden), "{forbidden:?} in {text:?}");
        }
    }
    assert_eq!(
        MINECRAFT_DOWNLOAD_URL,
        "https://www.minecraft.net/en-us/download"
    );
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
        state.retry_removals.insert(FOREST);
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
fn failed_vault_removals_preserve_denials_and_block_only_affected_slots() {
    let root = tempfile::tempdir().unwrap();
    let credentials = MemoryCredentials::default();
    let vault = credentials.vault(root.path(), CLIENT);
    let accounts = isolated();
    let core = &accounts.owner.0;
    let mode = PlayMode::OfflineSinglePlayer;
    for id in [COAST, FOREST] {
        accounts.begin();
        add(
            core,
            ticket(&accounts, Job::Add),
            synthetic(id, "fixture"),
            Some(&vault),
        );
    }
    accounts.refresh_selected();
    verify(
        core,
        ticket(&accounts, Job::Refresh(FOREST)),
        FOREST,
        Some(&vault),
        |_| Err(AccountError::NotEntitled),
    );
    accounts.sign_out_account(FOREST);
    remove(core, ticket(&accounts, Job::Remove(FOREST)), FOREST, None);
    assert_eq!(accounts.play_blocker(mode), PlayBlocker::EntitlementDenied);

    accounts.switch(COAST);
    select_current(
        core,
        ticket(&accounts, Job::Switch(COAST)),
        COAST,
        Some(&vault),
    );
    assert_eq!(
        accounts.play_blocker(mode),
        PlayBlocker::OfflinePolicyUnavailable
    );
    accounts.sign_out_account(COAST);
    remove(core, ticket(&accounts, Job::Remove(COAST)), COAST, None);
    assert_eq!(
        accounts.play_blocker(mode),
        PlayBlocker::AccountRemovalPending
    );
    assert_eq!(accounts.snapshot().removal_retry, Some(COAST));
    accounts.sign_out_account(COAST);
    remove(
        core,
        ticket(&accounts, Job::Remove(COAST)),
        COAST,
        Some(&vault),
    );
    assert_eq!(accounts.snapshot().removal_retry, Some(FOREST));
    assert_eq!(accounts.play_blocker(mode), PlayBlocker::EntitlementDenied);
}

#[cfg(feature = "dev-simulation")]
#[test]
fn simulated_accounts_refuse_an_unmarked_root_without_writing_it() {
    use crate::simulation::Simulation;

    let root = tempfile::tempdir().unwrap();
    let accounts = Accounts::simulated(root.path(), Simulation::new());
    assert!(accounts.is_simulated());
    assert_eq!(accounts.snapshot().state, AccountState::Unavailable);
    assert!(accounts.simulation().is_none());
    assert!(!root.path().join("synthetic-credentials.json").exists());
    assert!(!root.path().join("account-vault.lock").exists());
}

#[cfg(feature = "dev-simulation")]
#[test]
fn simulated_accounts_ignore_real_client_settings_and_stay_on_the_test_store() {
    use crate::simulation::Simulation;

    let library = tempfile::tempdir().unwrap();
    let root = library.path().join("dev-account-simulation");
    std::fs::create_dir_all(&root).unwrap();
    let stored = Uuid::from_u128(9);
    client::store(library.path(), stored).unwrap();
    crate::simulation::mark_root(&root).unwrap();
    let accounts = Accounts::simulated(&root, Simulation::new());
    wait_for(&accounts, |view| view.state == AccountState::SignedOut);
    assert!(accounts.is_simulated());
    assert!(!accounts.custom_client_allowed());
    assert!(!accounts.custom_client_configured());
    assert!(!accounts.custom_client_pending_restart());
    assert_eq!(
        accounts.owner.0.registration,
        Some(crate::simulation::CLIENT_ID)
    );
    assert!(accounts.set_custom_client(&stored.to_string()).is_err());
    assert!(accounts.clear_custom_client().is_err());
    assert_eq!(client::load(library.path()), Some(stored));
    assert!(
        !root.join("account-client.json").exists(),
        "synthetic mode must not copy or rewrite the real client setting"
    );
}

#[cfg(feature = "dev-simulation")]
fn wait_for(accounts: &Accounts, predicate: impl Fn(&AccountView) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        if predicate(&accounts.snapshot()) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the simulated account state: {:?} {}",
            accounts.snapshot().state,
            accounts.snapshot().message
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(feature = "dev-simulation")]
#[test]
fn simulated_accounts_sign_in_switch_restore_deny_and_remove() {
    use crate::simulation::{EntitlementState, Persona, Simulation};

    let root = tempfile::tempdir().unwrap();
    let simulation = Simulation::new();
    crate::simulation::mark_root(root.path()).unwrap();
    let accounts = Accounts::simulated(root.path(), simulation.clone());
    wait_for(&accounts, |view| view.state == AccountState::SignedOut);

    accounts.begin();
    wait_for(&accounts, |view| {
        view.state == AccountState::AwaitingBrowser
    });
    assert_eq!(simulation.pending_persona(), Some(Persona::Forest));
    assert!(
        accounts
            .snapshot()
            .message
            .contains("Simulated authorization"),
        "the pending step must not ask for a real browser"
    );
    assert!(!accounts.snapshot().message.contains("Open the browser"));
    assert!(
        accounts.open_browser().is_err(),
        "the simulated service must reject a real browser action"
    );
    accounts.cancel();
    wait_for(&accounts, |view| view.state == AccountState::SignedOut);
    assert_eq!(simulation.phase(), crate::simulation::SimulationPhase::Idle);
    assert_eq!(simulation.pending_persona(), None);

    accounts.begin();
    wait_for(&accounts, |view| {
        view.state == AccountState::AwaitingBrowser
    });
    assert!(simulation.approve());
    wait_for(&accounts, |view| view.state == AccountState::SignedIn);
    assert_eq!(
        accounts.snapshot().active_account,
        Some(Persona::Forest.profile_id())
    );
    assert_eq!(
        accounts.snapshot().message,
        "Synthetic test account verified."
    );
    assert!(
        root.path().join("synthetic-credentials.json").is_file(),
        "the simulation keeps credentials in its test root, not the OS vault"
    );

    simulation.set_next_persona(Persona::Coast);
    accounts.begin();
    wait_for(&accounts, |view| {
        view.state == AccountState::AwaitingBrowser
    });
    assert!(simulation.approve());
    wait_for(&accounts, |view| {
        view.state == AccountState::SignedIn
            && view.active_account == Some(Persona::Coast.profile_id())
    });
    assert_eq!(accounts.snapshot().accounts.len(), 2);

    accounts.switch(Persona::Forest.profile_id());
    wait_for(&accounts, |view| {
        view.state == AccountState::SignedIn
            && view.active_account == Some(Persona::Forest.profile_id())
    });

    assert!(matches!(
        accounts.prepare_launch(PlayMode::OfflineSinglePlayer, None),
        Err(PlayBlocker::SimulationUnavailable)
    ));
    assert!(matches!(
        accounts.prepare_launch(PlayMode::OnlineMultiplayer, None),
        Err(PlayBlocker::SimulationUnavailable)
    ));

    drop(accounts);
    let restarted_simulation = Simulation::new();
    let restarted = Accounts::simulated(root.path(), restarted_simulation.clone());
    assert_eq!(restarted_simulation.next_persona(), Persona::Forest);
    assert_eq!(
        restarted_simulation.entitlement(Persona::Forest),
        EntitlementState::Granted
    );
    wait_for(&restarted, |view| {
        view.state == AccountState::SignedIn
            && view.active_account == Some(Persona::Forest.profile_id())
    });
    assert_eq!(restarted.snapshot().accounts.len(), 2);

    let decision = crate::play::synthetic_decision_for_test(
        Persona::Forest.profile_id(),
        crate::simulation::CLIENT_ID,
    );
    let mut started = false;
    assert_eq!(
        restarted
            .with_launch_authority(decision, None, || started = true)
            .err(),
        Some(PlayBlocker::SimulationUnavailable)
    );
    assert!(!started, "synthetic admission must never start a process");

    restarted_simulation.set_entitlement(Persona::Forest, EntitlementState::Denied);
    restarted.refresh_selected();
    wait_for(&restarted, |view| view.state == AccountState::Error);
    assert_eq!(
        restarted.play_blocker(PlayMode::OfflineSinglePlayer),
        PlayBlocker::SimulationUnavailable
    );

    restarted.sign_out_account(Persona::Coast.profile_id());
    wait_for(&restarted, |view| {
        view.accounts
            .iter()
            .all(|account| account.id != Persona::Coast.profile_id())
    });
    assert!(restarted
        .snapshot()
        .accounts
        .iter()
        .any(|account| account.id == Persona::Forest.profile_id()));
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
