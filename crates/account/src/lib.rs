//! Microsoft account verification and independent synthetic entitlement fixtures.
pub mod desktop;
mod device;
mod transport;
mod vault;

pub use device::{
    AccountError, AuthenticatedAccount, DeviceChallenge, DeviceClient, DeviceLogin, DevicePoll,
    MinecraftProfile,
};

/// Stable explanation shared with the native Play command.
pub fn fixture_play_blocker() -> &'static str {
    "Live Minecraft entitlement is unavailable. This build has no approved account registration or session broker."
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Registration {
    Missing,
    Synthetic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleAccount {
    Forest,
    Coast,
}
impl SampleAccount {
    fn index(self) -> usize {
        match self {
            Self::Forest => 0,
            Self::Coast => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleResult {
    Entitled {
        online_until: u64,
        offline_until: Option<u64>,
    },
    NotEntitled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Observation {
    at: u64,
    result: SampleResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    Online,
    Offline { complete_install: bool },
}

/// Every result is a denial of actual play. The last variant records the positive policy fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayBlocker {
    RegistrationMissing,
    NoSelectedAccount,
    ProviderUnavailable,
    EntitlementUnknown,
    NotEntitled,
    InvalidObservationTime,
    EntitlementExpired,
    OfflineInstallIncomplete,
    OfflineGrantUnavailable,
    SyntheticEvidenceOnly,
}

pub struct FixtureAccounts {
    registration: Registration,
    selected: Option<SampleAccount>,
    observations: [Option<Observation>; 2],
    provider_available: bool,
    latest_time: Option<u64>,
}

impl FixtureAccounts {
    fn check_time(&mut self, now: u64) -> Result<(), PlayBlocker> {
        if self.latest_time.is_some_and(|previous| now < previous) {
            return Err(PlayBlocker::InvalidObservationTime);
        }
        self.latest_time = Some(now);
        Ok(())
    }

    pub fn new(registration: Registration) -> Self {
        Self {
            registration,
            selected: None,
            observations: [None; 2],
            provider_available: true,
            latest_time: None,
        }
    }

    pub fn selected(&self) -> Option<SampleAccount> {
        self.selected
    }

    /// Select a fixed identity. Each identity has an independent observation history.
    pub fn select(&mut self, account: SampleAccount) {
        self.selected = Some(account);
    }

    pub fn set_provider_available(&mut self, available: bool) {
        self.provider_available = available;
    }

    /// Save the result for the selected synthetic identity. A definitive negative supersedes a prior grant.
    pub fn observe(&mut self, now: u64, result: SampleResult) -> Result<(), PlayBlocker> {
        let account = self.account_for_request()?;
        self.check_time(now)?;
        if !self.provider_available {
            return Err(PlayBlocker::ProviderUnavailable);
        }
        let slot = &mut self.observations[account.index()];
        if slot.is_some_and(|last| now == last.at && result != last.result) {
            return Err(PlayBlocker::InvalidObservationTime);
        }
        if let SampleResult::Entitled {
            online_until,
            offline_until,
        } = result
        {
            if online_until <= now || offline_until.is_some_and(|end| end <= now) {
                return Err(PlayBlocker::InvalidObservationTime);
            }
        }
        *slot = Some(Observation { at: now, result });
        Ok(())
    }

    /// Remove this identity's fixture evidence. Signing out the selected identity clears selection.
    pub fn sign_out(&mut self, account: SampleAccount) {
        self.observations[account.index()] = None;
        if self.selected == Some(account) {
            self.selected = None;
        }
    }

    fn account_for_request(&self) -> Result<SampleAccount, PlayBlocker> {
        if self.registration == Registration::Missing {
            return Err(PlayBlocker::RegistrationMissing);
        }
        self.selected.ok_or(PlayBlocker::NoSelectedAccount)
    }

    /// Evaluate a request against synthetic timestamps. Successful fixture checks still block real play.
    pub fn play_blocker(&mut self, request: Request, now: u64) -> PlayBlocker {
        let account = match self.account_for_request() {
            Ok(account) => account,
            Err(reason) => return reason,
        };
        if let Err(reason) = self.check_time(now) {
            return reason;
        }
        if request == Request::Online && !self.provider_available {
            return PlayBlocker::ProviderUnavailable;
        }
        let Some(observation) = self.observations[account.index()] else {
            return PlayBlocker::EntitlementUnknown;
        };
        let SampleResult::Entitled {
            online_until,
            offline_until,
        } = observation.result
        else {
            return PlayBlocker::NotEntitled;
        };
        let expires_at = match request {
            Request::Online => online_until,
            Request::Offline {
                complete_install: false,
            } => return PlayBlocker::OfflineInstallIncomplete,
            Request::Offline {
                complete_install: true,
            } => match offline_until {
                Some(end) => end,
                None => return PlayBlocker::OfflineGrantUnavailable,
            },
        };
        if now >= expires_at {
            return PlayBlocker::EntitlementExpired;
        }
        PlayBlocker::SyntheticEvidenceOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PlayBlocker::*;
    use SampleAccount::*;
    const COMPLETE: Request = Request::Offline {
        complete_install: true,
    };
    const GRANT: SampleResult = SampleResult::Entitled {
        online_until: 40,
        offline_until: Some(30),
    };

    #[test]
    fn missing_registration_blocks_observation_and_every_play_mode() {
        let mut accounts = FixtureAccounts::new(Registration::Missing);
        accounts.select(Forest);
        assert_eq!(accounts.observe(10, GRANT), Err(RegistrationMissing));
        for mode in [Request::Online, COMPLETE] {
            assert_eq!(accounts.play_blocker(mode, 11), RegistrationMissing);
        }
        assert!(fixture_play_blocker().contains("no approved account registration"));
    }

    #[test]
    fn switching_uses_selected_identity_and_signout_forgets_only_its_evidence() {
        let mut accounts = FixtureAccounts::new(Registration::Synthetic);
        assert_eq!(
            accounts.play_blocker(Request::Online, 10),
            NoSelectedAccount
        );
        accounts.select(Forest);
        accounts.observe(10, GRANT).unwrap();
        assert_eq!(
            accounts.play_blocker(Request::Online, 11),
            SyntheticEvidenceOnly
        );
        accounts.select(Coast);
        assert_eq!(accounts.selected(), Some(Coast));
        assert_eq!(
            accounts.play_blocker(Request::Online, 11),
            EntitlementUnknown
        );
        accounts.observe(11, SampleResult::NotEntitled).unwrap();
        assert_eq!(accounts.play_blocker(COMPLETE, 11), NotEntitled);
        accounts.select(Forest);
        assert_eq!(accounts.play_blocker(COMPLETE, 11), SyntheticEvidenceOnly);
        accounts.sign_out(Coast);
        assert_eq!(accounts.selected(), Some(Forest));
        accounts.sign_out(Forest);
        assert_eq!(accounts.play_blocker(COMPLETE, 11), NoSelectedAccount);
        accounts.select(Forest);
        assert_eq!(accounts.play_blocker(COMPLETE, 11), EntitlementUnknown);
    }

    #[test]
    fn offline_requires_an_explicit_current_grant_and_complete_install() {
        let mut accounts = FixtureAccounts::new(Registration::Synthetic);
        accounts.select(Forest);
        accounts
            .observe(
                10,
                SampleResult::Entitled {
                    online_until: 40,
                    offline_until: None,
                },
            )
            .unwrap();
        assert_eq!(accounts.play_blocker(COMPLETE, 11), OfflineGrantUnavailable);
        accounts.observe(11, GRANT).unwrap();
        assert_eq!(
            accounts.play_blocker(
                Request::Offline {
                    complete_install: false
                },
                12
            ),
            OfflineInstallIncomplete
        );
        accounts.set_provider_available(false);
        assert_eq!(
            accounts.play_blocker(Request::Online, 12),
            ProviderUnavailable
        );
        assert_eq!(
            accounts.observe(12, SampleResult::NotEntitled),
            Err(ProviderUnavailable)
        );
        assert_eq!(accounts.play_blocker(COMPLETE, 12), SyntheticEvidenceOnly);
        assert_eq!(accounts.play_blocker(COMPLETE, 30), EntitlementExpired);
        accounts.set_provider_available(true);
        assert_eq!(
            accounts.play_blocker(Request::Online, 39),
            SyntheticEvidenceOnly
        );
        assert_eq!(
            accounts.play_blocker(Request::Online, 40),
            EntitlementExpired
        );
    }

    #[test]
    fn expired_observation_stays_expired_after_clock_reversal_and_switching() {
        let mut accounts = FixtureAccounts::new(Registration::Synthetic);
        accounts.select(Forest);
        accounts.observe(10, GRANT).unwrap();
        assert_eq!(
            accounts.play_blocker(Request::Online, 40),
            EntitlementExpired
        );
        accounts.select(Coast);
        accounts.select(Forest);
        assert_eq!(
            accounts.play_blocker(Request::Online, 39),
            InvalidObservationTime
        );
        assert_eq!(accounts.play_blocker(COMPLETE, 12), InvalidObservationTime);
        assert_eq!(
            accounts.play_blocker(Request::Online, 41),
            EntitlementExpired
        );
    }

    #[test]
    fn stale_observations_and_clock_reversal_cannot_reinstate_a_grant() {
        let mut accounts = FixtureAccounts::new(Registration::Synthetic);
        accounts.select(Forest);
        accounts.observe(10, GRANT).unwrap();
        assert_eq!(accounts.play_blocker(COMPLETE, 9), InvalidObservationTime);
        accounts.observe(15, SampleResult::NotEntitled).unwrap();
        assert_eq!(accounts.observe(12, GRANT), Err(InvalidObservationTime));
        assert_eq!(
            accounts.observe(
                16,
                SampleResult::Entitled {
                    online_until: 16,
                    offline_until: None
                }
            ),
            Err(InvalidObservationTime)
        );
        assert_eq!(accounts.play_blocker(Request::Online, 16), NotEntitled);
        assert_eq!(accounts.play_blocker(COMPLETE, 16), NotEntitled);
    }
}
