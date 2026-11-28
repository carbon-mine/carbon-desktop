// SPDX-License-Identifier: MPL-2.0
//! Selects the real or opt-in developer account service for the native shell.
//!
//! The simulation variant exists only with the non-default
//! `dev-account-simulation` feature. Production builds always construct
//! [`AccountMode::production`].

use crate::account_panel::AccountPanel;
use std::path::Path;

pub struct AccountMode {
    #[cfg(feature = "dev-account-simulation")]
    simulation: Option<crate::dev_account_simulation::Session>,
}

impl AccountMode {
    pub fn production() -> Self {
        Self {
            #[cfg(feature = "dev-account-simulation")]
            simulation: None,
        }
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulated(session: crate::dev_account_simulation::Session) -> Self {
        Self {
            simulation: Some(session),
        }
    }

    pub fn panel(&self, library_root: &Path) -> AccountPanel {
        #[cfg(feature = "dev-account-simulation")]
        if let Some(session) = &self.simulation {
            return AccountPanel::new_simulated(&session.account_root, session.simulation.clone());
        }
        AccountPanel::new(library_root)
    }
}
