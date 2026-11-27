//! The in-app updater: what it is doing (`get_update_state`, `updates:state`)
//! and when it checks on its own.

use serde::{Deserialize, Serialize};

use crate::Millis;

/// Automatic checks wait this long after launch, so they never slow it down.
pub const FIRST_CHECK_DELAY_MS: Millis = 60_000;
/// Automatic checks run at most this often.
pub const CHECK_INTERVAL_MS: Millis = 24 * 60 * 60 * 1000;

/// A newer version on the update server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    /// What changed, as Markdown.
    pub notes: Option<String>,
    pub published_at: Option<Millis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UpdateState {
    /// Nothing checked since launch.
    #[default]
    Idle,
    Checking,
    UpToDate {
        checked_at: Millis,
    },
    Available {
        release: Release,
    },
    Downloading {
        release: Release,
        received: u64,
        total: Option<u64>,
    },
    /// Downloaded and verified. Installed when the app restarts.
    Ready {
        release: Release,
    },
    /// Replacing the app, then restarting.
    Installing {
        release: Release,
    },
    /// `release` is set when a download failed, so it can be retried.
    Failed {
        message: String,
        release: Option<Release>,
    },
}

impl UpdateState {
    /// Checking again is allowed unless something is running or an update
    /// is already downloaded.
    pub fn can_check(&self) -> bool {
        matches!(
            self,
            Self::Idle | Self::UpToDate { .. } | Self::Available { .. } | Self::Failed { .. }
        )
    }

    /// The release a download would fetch.
    pub fn downloadable(&self) -> Option<&Release> {
        match self {
            Self::Available { release }
            | Self::Failed {
                release: Some(release),
                ..
            } => Some(release),
            _ => None,
        }
    }

    /// The release that is downloaded and waiting for a restart.
    pub fn installable(&self) -> Option<&Release> {
        match self {
            Self::Ready { release } => Some(release),
            _ => None,
        }
    }

    /// The state after a check: `found` is a newer release, if any.
    pub fn checked(found: Option<Release>, now: Millis) -> Self {
        match found {
            Some(release) => Self::Available { release },
            None => Self::UpToDate { checked_at: now },
        }
    }
}

/// Whether an automatic check should run now. `last_check` is the last
/// finished check, automatic or not, since `launched_at`.
pub fn auto_check_due(
    enabled: bool,
    launched_at: Millis,
    last_check: Option<Millis>,
    now: Millis,
) -> bool {
    if !enabled {
        return false;
    }
    match last_check {
        Some(last) => now.saturating_sub(last) >= CHECK_INTERVAL_MS,
        None => now.saturating_sub(launched_at) >= FIRST_CHECK_DELAY_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Release {
        Release {
            version: "0.2.1".to_owned(),
            notes: Some("- Faster start".to_owned()),
            published_at: Some(1_700_000_000_000),
        }
    }

    #[test]
    fn serialises_with_a_state_tag() {
        let json = serde_json::to_value(UpdateState::Downloading {
            release: release(),
            received: 10,
            total: Some(20),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "state": "downloading",
                "release": {
                    "version": "0.2.1",
                    "notes": "- Faster start",
                    "publishedAt": 1_700_000_000_000_i64,
                },
                "received": 10,
                "total": 20,
            })
        );
        let json = serde_json::to_value(UpdateState::UpToDate { checked_at: 5 }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "state": "upToDate", "checkedAt": 5 })
        );
    }

    #[test]
    fn only_idle_and_finished_states_can_check() {
        assert!(UpdateState::Idle.can_check());
        assert!(UpdateState::UpToDate { checked_at: 1 }.can_check());
        assert!(UpdateState::Available { release: release() }.can_check());
        assert!(
            UpdateState::Failed {
                message: "x".into(),
                release: None
            }
            .can_check()
        );
        assert!(!UpdateState::Checking.can_check());
        assert!(
            !UpdateState::Downloading {
                release: release(),
                received: 0,
                total: None
            }
            .can_check()
        );
        assert!(!UpdateState::Ready { release: release() }.can_check());
        assert!(!UpdateState::Installing { release: release() }.can_check());
    }

    #[test]
    fn a_failed_download_can_be_retried() {
        let failed = UpdateState::Failed {
            message: "x".into(),
            release: Some(release()),
        };
        assert_eq!(failed.downloadable(), Some(&release()));
        let failed_check = UpdateState::Failed {
            message: "x".into(),
            release: None,
        };
        assert_eq!(failed_check.downloadable(), None);
        assert_eq!(
            UpdateState::Ready { release: release() }.downloadable(),
            None
        );
    }

    #[test]
    fn only_a_downloaded_update_installs() {
        assert_eq!(
            UpdateState::Ready { release: release() }.installable(),
            Some(&release())
        );
        assert_eq!(
            UpdateState::Available { release: release() }.installable(),
            None
        );
    }

    #[test]
    fn a_check_finds_a_release_or_nothing() {
        assert_eq!(
            UpdateState::checked(Some(release()), 9),
            UpdateState::Available { release: release() }
        );
        assert_eq!(
            UpdateState::checked(None, 9),
            UpdateState::UpToDate { checked_at: 9 }
        );
    }

    #[test]
    fn automatic_checks_wait_for_launch_then_a_day() {
        let launched = 1_000_000;
        let due = |last, now| auto_check_due(true, launched, last, now);
        assert!(!due(None, launched + FIRST_CHECK_DELAY_MS - 1));
        assert!(due(None, launched + FIRST_CHECK_DELAY_MS));
        let last = launched + 5_000_000;
        assert!(!due(Some(last), last + CHECK_INTERVAL_MS - 1));
        assert!(due(Some(last), last + CHECK_INTERVAL_MS));
    }

    #[test]
    fn automatic_checks_need_the_setting() {
        let now = 10 * CHECK_INTERVAL_MS;
        assert!(!auto_check_due(false, 0, None, now));
        assert!(auto_check_due(true, 0, None, now));
    }
}
