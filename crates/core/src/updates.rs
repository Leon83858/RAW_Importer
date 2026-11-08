//! Stable-release comparison for the optional GitHub update checker.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(tag = "status", rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum UpdateStatus {
    UpToDate {
        current_version: String,
        latest_version: String,
        release_url: String,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        release_url: String,
    },
    LocalVersionAhead {
        current_version: String,
        latest_version: String,
        release_url: String,
    },
    NoStableRelease {
        current_version: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VersionCompareError {
    #[error("invalid current version {0:?}")]
    InvalidCurrent(String),
    #[error("invalid GitHub release version {0:?}")]
    InvalidRelease(String),
}

pub fn compare_release(
    current: &str,
    release_tag: &str,
    release_url: &str,
) -> Result<UpdateStatus, VersionCompareError> {
    let current_version = semver::Version::parse(current)
        .map_err(|_| VersionCompareError::InvalidCurrent(current.to_string()))?;
    let normalized_tag = release_tag.strip_prefix('v').unwrap_or(release_tag);
    let latest_version = semver::Version::parse(normalized_tag)
        .map_err(|_| VersionCompareError::InvalidRelease(release_tag.to_string()))?;
    let current_text = current_version.to_string();
    let latest_text = latest_version.to_string();
    let status = match latest_version.cmp(&current_version) {
        std::cmp::Ordering::Greater => UpdateStatus::UpdateAvailable {
            current_version: current_text,
            latest_version: latest_text,
            release_url: release_url.to_string(),
        },
        std::cmp::Ordering::Equal => UpdateStatus::UpToDate {
            current_version: current_text,
            latest_version: latest_text,
            release_url: release_url.to_string(),
        },
        std::cmp::Ordering::Less => UpdateStatus::LocalVersionAhead {
            current_version: current_text,
            latest_version: latest_text,
            release_url: release_url.to_string(),
        },
    };
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_release_is_available_for_prerelease_build() {
        assert_eq!(
            compare_release("3.0.0-dev", "v3.0.0", "https://github.com/example/release").unwrap(),
            UpdateStatus::UpdateAvailable {
                current_version: "3.0.0-dev".into(),
                latest_version: "3.0.0".into(),
                release_url: "https://github.com/example/release".into(),
            }
        );
    }

    #[test]
    fn current_or_newer_development_build_is_not_offered_an_older_release() {
        assert!(matches!(
            compare_release("3.0.0-dev", "v2.1.0", "https://github.com/example/release"),
            Ok(UpdateStatus::LocalVersionAhead { .. })
        ));
        assert!(matches!(
            compare_release("3.0.0", "v2.1.0", "https://github.com/example/release"),
            Ok(UpdateStatus::LocalVersionAhead { .. })
        ));
        assert!(matches!(
            compare_release("2.1.0", "v2.1.0", "https://github.com/example/release"),
            Ok(UpdateStatus::UpToDate { .. })
        ));
    }

    #[test]
    fn invalid_semver_is_reported() {
        assert_eq!(
            compare_release("3.0", "v2.1.0", "https://github.com/example/release"),
            Err(VersionCompareError::InvalidCurrent("3.0".into()))
        );
        assert_eq!(
            compare_release(
                "3.0.0",
                "release-next",
                "https://github.com/example/release"
            ),
            Err(VersionCompareError::InvalidRelease("release-next".into()))
        );
    }
}
