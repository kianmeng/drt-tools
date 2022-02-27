// Copyright 2022 Sebastian Ramacher
// SPDX-License-Identifier: LGPL-3.0-or-later

//! # Version handling
//!
//! This module handles versions of Debian packages.
//!
//! ```
//! use assorted_debian_utils::version::PackageVersion;
//!
//! let ver1 = PackageVersion::new(None, "1.0", Some("2")).expect("Failed to construct version");
//! assert_eq!(ver1.to_string(), "1.0-2");
//! assert!(!ver1.has_epoch());
//! assert!(!ver1.is_native());
//!
//! let ver2 = PackageVersion::new(Some(1), "0.2", Some("1.1")).expect("Failed to construct version");
//! assert_eq!(ver2.to_string(), "1:0.2-1.1");
//! assert!(ver2.has_epoch());
//! assert!(!ver2.is_native());
//!
//! #[cfg(feature="libdpkg-sys")]
//! {
//!     assert!(ver1 < ver2);
//!     assert_eq!(ver1, PackageVersion::new(Some(0), "1.0", Some("2")).expect("Failed to construct version"));
//! }
//! ```

use std::{
    error::Error,
    fmt::{Display, Formatter},
};

pub use crate::ParseError;

/// Version errors
#[derive(Debug)]
pub enum VersionError {
    /// Epoch is invalid
    InvalidEpoch,
    /// Upstream version is invalid
    InvalidUpstreamVersion,
    /// Debian revision is invalid
    InvalidDebianRevision,
}

impl Display for VersionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::InvalidEpoch => write!(f, "invalid epoch"),
            VersionError::InvalidUpstreamVersion => write!(f, "invalid upstream version"),
            VersionError::InvalidDebianRevision => write!(f, "invalid Debian revision"),
        }
    }
}

impl Error for VersionError {}

/// A version number of a Debian package
///
/// Version numbers consists of three components:
/// * an optional epoch
/// * the upstream version
/// * an optional debian revision
#[derive(Debug, Clone)]
pub struct PackageVersion {
    /// The (optional) epoch
    pub(crate) epoch: Option<u32>,
    /// The upstream version
    pub(crate) upstream_version: String,
    /// The (optional) Debian revision
    pub(crate) debian_revision: Option<String>,
}

impl PackageVersion {
    /// Create a new version struct from the individual components.
    pub fn new(
        epoch: Option<u32>,
        upstream_version: &str,
        debian_revision: Option<&str>,
    ) -> Result<Self, VersionError> {
        if upstream_version.is_empty()
            || upstream_version
                .chars()
                .any(|c| !c.is_alphanumeric() && !".+-~".contains(c))
        {
            return Err(VersionError::InvalidUpstreamVersion);
        }

        if let Some(rev) = debian_revision {
            if rev.is_empty()
                || rev
                    .chars()
                    .any(|c| !c.is_alphanumeric() && !".+~".contains(c))
            {
                return Err(VersionError::InvalidDebianRevision);
            }
        }

        Ok(Self {
            epoch,
            upstream_version: String::from(upstream_version),
            debian_revision: debian_revision.map(String::from),
        })
    }

    /// Returns whether version is a native version, i.e., there is no revision.
    pub fn is_native(&self) -> bool {
        self.debian_revision.is_none()
    }

    /// Return whether the version has an epoch.
    pub fn has_epoch(&self) -> bool {
        self.epoch.is_some()
    }

    /// Return epoch of 0 if none set.
    pub fn epoch_or_0(&self) -> u32 {
        self.epoch.unwrap_or(0)
    }
}

#[cfg(feature = "libdpkg-sys")]
use std::cmp::Ordering;

#[cfg(feature = "libdpkg-sys")]
use crate::cversion::CVersion;

#[cfg(feature = "libdpkg-sys")]
impl PartialOrd for PackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(feature = "libdpkg-sys")]
impl Ord for PackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        CVersion::from(self).cmp(&CVersion::from(other))
    }
}

#[cfg(feature = "libdpkg-sys")]
impl PartialEq for PackageVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

#[cfg(not(feature = "libdpkg-sys"))]
impl PartialEq for PackageVersion {
    fn eq(&self, other: &Self) -> bool {
        self.epoch_or_0() == other.epoch_or_0()
            && self.upstream_version == other.upstream_version
            && self.debian_revision == other.debian_revision
    }
}

impl Eq for PackageVersion {}

impl TryFrom<&str> for PackageVersion {
    type Error = ParseError;

    fn try_from(mut value: &str) -> Result<Self, Self::Error> {
        let epoch = if let Some((epoch_str, new_value)) = value.split_once(":") {
            value = new_value;
            Some(
                epoch_str
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidVersion(VersionError::InvalidEpoch))?,
            )
        } else {
            None
        };

        let debian_revision = if let Some((new_value, debian_revision_str)) = value.rsplit_once("-")
        {
            value = new_value;
            Some(debian_revision_str)
        } else {
            None
        };

        Self::new(epoch, value, debian_revision).map_err(ParseError::InvalidVersion)
    }
}

impl Display for PackageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(epoch) = self.epoch {
            write!(f, "{}:", epoch)?;
        }
        write!(f, "{}", self.upstream_version)?;
        if let Some(debian_revision) = &self.debian_revision {
            write!(f, "-{}", debian_revision)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::PackageVersion;

    #[test]
    fn conversion() {
        let version = PackageVersion::try_from("2:1.0+dfsg-1").unwrap();
        assert_eq!(version.epoch, Some(2));
        assert_eq!(version.upstream_version, "1.0+dfsg");
        assert_eq!(version.debian_revision, Some("1".into()));
    }

    #[cfg(feature = "libdpkg-sys")]
    #[test]
    fn epoch_compare() {
        let version1 = PackageVersion::try_from("2.0-1").unwrap();
        let version2 = PackageVersion::try_from("2:1.0+dfsg-1").unwrap();

        assert!(version2.has_epoch());
        assert!(!version1.has_epoch());
        assert!(version1 < version2);
    }

    #[test]
    fn zero_epoch_compare() {
        let version1 = PackageVersion::try_from("2.0-1").unwrap();
        let version2 = PackageVersion::try_from("0:2.0-1").unwrap();

        assert_eq!(version1, version2);
    }

    #[test]
    fn invalid_epoch() {
        assert!(PackageVersion::try_from("-1:1.0-1").is_err());
        assert!(PackageVersion::try_from(":1.0-1").is_err());
        assert!(PackageVersion::try_from("a1:1.0-1").is_err());
    }

    #[test]
    fn invalid_upstream_version() {
        assert!(PackageVersion::try_from("-1").is_err());
        assert!(PackageVersion::try_from("0:-1").is_err());
    }

    #[test]
    fn multi_dash() {
        let version = PackageVersion::try_from("1.0-2-1").unwrap();
        assert_eq!(version.upstream_version, "1.0-2");
        assert_eq!(version.debian_revision, Some("1".into()));
    }
}
