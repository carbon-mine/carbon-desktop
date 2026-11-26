use super::*;
use std::{cell::Cell, time::Duration};

const API: Uuid = Uuid::from_u128(1);
const FOREST: Uuid = Uuid::from_u128(2);
const COAST: Uuid = Uuid::from_u128(3);
const INSTANCE: Uuid = Uuid::from_u128(4);

struct Fixture {
    owner: Arc<()>,
    at: Clock,
}

impl Fixture {
    fn new() -> Self {
        Self {
            owner: Arc::new(()),
            at: Clock {
                wall: SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
                monotonic: Instant::now(),
            },
        }
    }

    fn after(&self, seconds: u64) -> Clock {
        let delay = Duration::from_secs(seconds);
        Clock {
            wall: self.at.wall + delay,
            monotonic: self.at.monotonic + delay,
        }
    }

    fn selection(&self) -> Selection<'_> {
        Selection {
            owner: &self.owner,
            revision: 10,
            registration: Some(API),
            profile: Some(FOREST),
            online_verified: true,
            entitlement_denied: false,
            pending: false,
            removal_pending: false,
        }
    }

    fn evidence(&self, mode: PlayMode) -> Evidence {
        Evidence {
            issuer_session: Arc::downgrade(&self.owner),
            profile: FOREST,
            registration: API,
            mode,
            policy_digest: [9; 32],
            observed: self.at,
            expires_wall: self.after(100).wall,
            expires_monotonic: self.after(100).monotonic,
            subscription_ends: None,
            multiplayer_permitted: true,
        }
    }
}

fn installation(mode: PlayMode) -> Installation {
    Installation {
        instance: INSTANCE,
        activation_digest: [7; 32],
        private_saves_verified: true,
        network: match mode {
            PlayMode::OfflineSinglePlayer => NetworkBoundary::OfflineEgressDenied,
            PlayMode::OnlineMultiplayer => NetworkBoundary::ProviderScopedOnline,
        },
    }
}

#[test]
fn verified_account_without_provider_evidence_cannot_play_either_mode() {
    let fixture = Fixture::new();
    for (mode, expected) in [
        (
            PlayMode::OfflineSinglePlayer,
            PlayBlocker::OfflinePolicyUnavailable,
        ),
        (
            PlayMode::OnlineMultiplayer,
            PlayBlocker::OnlineSessionUnavailable,
        ),
    ] {
        let selected = fixture.selection();
        assert_eq!(
            issue(selected, mode, Some(installation(mode)), None, fixture.at).err(),
            Some(expected)
        );
        assert_eq!(
            missing_evidence(
                Selection {
                    registration: None,
                    ..selected
                },
                mode
            ),
            PlayBlocker::RegistrationRequired
        );
        assert_eq!(
            missing_evidence(
                Selection {
                    profile: None,
                    ..selected
                },
                mode
            ),
            PlayBlocker::AccountSelectionRequired
        );
        assert_eq!(
            missing_evidence(
                Selection {
                    online_verified: false,
                    ..selected
                },
                mode
            ),
            PlayBlocker::AccountVerificationRequired
        );
    }
}

#[test]
fn synthetic_offline_evidence_requires_the_native_install_saves_and_egress_boundary() {
    let fixture = Fixture::new();
    let mode = PlayMode::OfflineSinglePlayer;
    let evidence = fixture.evidence(mode);
    let selected = Selection {
        online_verified: false,
        ..fixture.selection()
    };
    let install = installation(mode);
    for (candidate, expected) in [
        (None, PlayBlocker::InstallationRequired),
        (
            Some(Installation {
                activation_digest: [0; 32],
                ..install
            }),
            PlayBlocker::InstallationRequired,
        ),
        (
            Some(Installation {
                private_saves_verified: false,
                ..install
            }),
            PlayBlocker::PrivateSavesRequired,
        ),
        (
            Some(Installation {
                network: NetworkBoundary::Unverified,
                ..install
            }),
            PlayBlocker::IsolationRequired,
        ),
        (
            Some(installation(PlayMode::OnlineMultiplayer)),
            PlayBlocker::IsolationRequired,
        ),
    ] {
        assert_eq!(
            issue(selected, mode, candidate, Some(&evidence), fixture.at).err(),
            Some(expected)
        );
    }
    let permit = issue(selected, mode, Some(install), Some(&evidence), fixture.at).unwrap();
    assert_eq!(permit.profile_id(), FOREST);
    assert_eq!(permit.account_revision(), 10);
    assert_eq!(permit.mode(), mode);
    assert_eq!(
        admit(
            permit,
            selected,
            Some(install),
            Some(&evidence),
            fixture.after(1),
            || "synthetic admission"
        ),
        Ok("synthetic admission")
    );
}

#[test]
fn subscription_expiry_holds_when_the_wall_clock_stops_advancing() {
    let fixture = Fixture::new();
    let mode = PlayMode::OfflineSinglePlayer;
    let selected = fixture.selection();
    let mut evidence = fixture.evidence(mode);
    evidence.subscription_ends = Some(fixture.after(50).wall);
    let install = Some(installation(mode));
    let before = fixture.after(49);
    let decision = issue(selected, mode, install, Some(&evidence), before).unwrap();
    let at_expiry = Clock {
        wall: before.wall,
        monotonic: fixture.after(50).monotonic,
    };
    assert_eq!(
        admit(
            decision,
            selected,
            install,
            Some(&evidence),
            at_expiry,
            || ()
        ),
        Err(PlayBlocker::EvidenceExpired)
    );
    assert_eq!(
        issue(selected, mode, install, Some(&evidence), at_expiry).err(),
        Some(PlayBlocker::EvidenceExpired)
    );
}

#[test]
fn failed_removal_never_admits_a_previously_verified_slot() {
    let fixture = Fixture::new();
    let mode = PlayMode::OfflineSinglePlayer;
    let evidence = fixture.evidence(mode);
    let selected = fixture.selection();
    let install = Some(installation(mode));
    let started = Cell::new(false);
    for denied in [true, false] {
        let permit = issue(selected, mode, install, Some(&evidence), fixture.at).unwrap();
        let removing = Selection {
            entitlement_denied: denied,
            removal_pending: true,
            ..selected
        };
        let expected = if denied {
            PlayBlocker::EntitlementDenied
        } else {
            PlayBlocker::AccountRemovalPending
        };
        assert_eq!(
            issue(removing, mode, install, Some(&evidence), fixture.at).err(),
            Some(expected)
        );
        assert_eq!(
            admit(
                permit,
                removing,
                install,
                Some(&evidence),
                fixture.at,
                || started.set(true)
            ),
            Err(expected)
        );
    }
    assert!(!started.get());
}

#[test]
fn stale_account_or_install_never_invokes_admission() {
    let fixture = Fixture::new();
    let another_service = Fixture::new();
    let mode = PlayMode::OfflineSinglePlayer;
    let evidence = fixture.evidence(mode);
    let selected = fixture.selection();
    let install = installation(mode);
    let started = Cell::new(0);
    for (current, latest) in [
        (
            Selection {
                profile: Some(COAST),
                ..selected
            },
            Some(install),
        ),
        (
            Selection {
                revision: selected.revision + 2,
                ..selected
            },
            Some(install),
        ),
        (
            Selection {
                registration: None,
                ..selected
            },
            Some(install),
        ),
        (
            Selection {
                pending: true,
                ..selected
            },
            Some(install),
        ),
        (
            Selection {
                profile: None,
                ..selected
            },
            Some(install),
        ),
        (
            Selection {
                owner: &another_service.owner,
                ..selected
            },
            Some(install),
        ),
        (selected, None),
        (
            selected,
            Some(Installation {
                activation_digest: [8; 32],
                ..install
            }),
        ),
        (
            selected,
            Some(Installation {
                instance: Uuid::from_u128(99),
                ..install
            }),
        ),
    ] {
        let permit = issue(selected, mode, Some(install), Some(&evidence), fixture.at).unwrap();
        assert_eq!(
            admit(
                permit,
                current,
                latest,
                Some(&evidence),
                fixture.after(1),
                || started.set(started.get() + 1)
            ),
            Err(PlayBlocker::LaunchStateChanged)
        );
    }
    let permit = issue(selected, mode, Some(install), Some(&evidence), fixture.at).unwrap();
    let changed_policy = Evidence {
        policy_digest: [8; 32],
        ..evidence
    };
    assert_eq!(
        admit(
            permit,
            selected,
            Some(install),
            Some(&changed_policy),
            fixture.after(1),
            || started.set(1)
        ),
        Err(PlayBlocker::LaunchStateChanged)
    );
    assert_eq!(started.get(), 0);
}

#[test]
fn authority_expiry_is_exclusive_on_wall_monotonic_and_subscription_bounds() {
    let fixture = Fixture::new();
    let mode = PlayMode::OfflineSinglePlayer;
    let selected = fixture.selection();
    let install = Some(installation(mode));
    let evidence = fixture.evidence(mode);
    assert_eq!(
        issue(selected, mode, install, Some(&evidence), fixture.after(100)).err(),
        Some(PlayBlocker::EvidenceExpired)
    );
    assert_eq!(
        issue(
            selected,
            mode,
            install,
            Some(&evidence),
            Clock {
                wall: fixture.after(99).wall,
                monotonic: fixture.after(100).monotonic
            }
        )
        .err(),
        Some(PlayBlocker::EvidenceExpired)
    );
    let subscription = Evidence {
        subscription_ends: Some(fixture.after(50).wall),
        ..evidence
    };
    let prior = issue(
        selected,
        mode,
        install,
        Some(&subscription),
        fixture.after(49),
    )
    .unwrap();
    assert_eq!(
        admit(
            prior,
            selected,
            install,
            Some(&subscription),
            fixture.after(50),
            || panic!("expired admission")
        ),
        Err::<(), _>(PlayBlocker::EvidenceExpired)
    );
}

#[test]
fn rollback_foreign_evidence_and_definitive_denial_leave_no_offline_fallback() {
    let fixture = Fixture::new();
    let restarted = Fixture::new();
    let mode = PlayMode::OfflineSinglePlayer;
    let evidence = fixture.evidence(mode);
    let selected = fixture.selection();
    let install = Some(installation(mode));
    let early = Clock {
        wall: fixture.at.wall - Duration::from_secs(1),
        monotonic: fixture.at.monotonic,
    };
    assert_eq!(
        issue(selected, mode, install, Some(&evidence), early).err(),
        Some(PlayBlocker::ClockUntrusted)
    );
    let prior = issue(selected, mode, install, Some(&evidence), fixture.after(10)).unwrap();
    assert_eq!(
        admit(
            prior,
            selected,
            install,
            Some(&evidence),
            Clock {
                wall: fixture.after(9).wall,
                monotonic: fixture.after(11).monotonic
            },
            || panic!("rolled-back admission")
        ),
        Err::<(), _>(PlayBlocker::ClockUntrusted)
    );
    assert_eq!(
        issue(
            restarted.selection(),
            mode,
            install,
            Some(&evidence),
            restarted.at
        )
        .err(),
        Some(PlayBlocker::EvidenceMismatch)
    );
    let denied = Selection {
        entitlement_denied: true,
        ..selected
    };
    assert_eq!(
        issue(denied, mode, install, Some(&evidence), fixture.after(20)).err(),
        Some(PlayBlocker::EntitlementDenied)
    );
}

#[test]
fn online_requires_current_entitlement_and_separate_multiplayer_scope() {
    let fixture = Fixture::new();
    let online = PlayMode::OnlineMultiplayer;
    let selected = fixture.selection();
    let install = Some(installation(online));
    let offline = fixture.evidence(PlayMode::OfflineSinglePlayer);
    assert_eq!(
        issue(selected, online, install, Some(&offline), fixture.at).err(),
        Some(PlayBlocker::EvidenceMismatch)
    );
    let no_family_permission = Evidence {
        multiplayer_permitted: false,
        ..fixture.evidence(online)
    };
    assert_eq!(
        issue(
            selected,
            online,
            install,
            Some(&no_family_permission),
            fixture.at
        )
        .err(),
        Some(PlayBlocker::MultiplayerPermissionRequired)
    );
    assert_eq!(
        issue(
            Selection {
                online_verified: false,
                ..selected
            },
            online,
            install,
            Some(&fixture.evidence(online)),
            fixture.at
        )
        .err(),
        Some(PlayBlocker::AccountVerificationRequired)
    );
}
