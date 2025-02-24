// Copyright 2021-2022 Sebastian Ramacher
// SPDX-License-Identifier: LGPL-3.0-or-later

//! # Helpers to handle `excuses.yaml` for testing migration
//!
//! This module provides helpers to deserialize [excuses.yaml](https://release.debian.org/britney/excuses.yaml)
//! with [serde]. Note however, that this module only handles a biased selection of fields.

use std::{collections::HashMap, io};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{architectures::Architecture, archive::Component, utils::DateTimeVisitor};

/// Deserialize a datetime string into a `DateTime<Utc>`
fn deserialize_datetime<'de, D>(deserializer: D) -> std::result::Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(DateTimeVisitor("%Y-%m-%d %H:%M:%S%.f"))
}

/// The excuses.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Excuses {
    /// Date of the run that produced `excuses.yaml`
    #[serde(deserialize_with = "deserialize_datetime")]
    pub generated_date: DateTime<Utc>,
    /// All excuses
    ///
    /// While not every excuses item relates to a source package, the field is still named that way in `excuses.yaml`
    pub sources: Vec<ExcusesItem>,
}

/// A policy's verdict
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum Verdict {
    /// Policy passed
    #[serde(rename = "PASS")]
    Pass,
    /// Policy passed due to a hint
    #[serde(rename = "PASS_HINTED")]
    PassHinted,
    /// Rejected due to a block hint or because the upload requires explicit approval (e.g.,
    /// uploads to proposed-updates or testing-proposed-updates)
    #[serde(rename = "REJECTED_NEEDS_APPROVAL")]
    RejectedNeedsApproval,
    /// Rejected tu to a permanent issue
    #[serde(rename = "REJECTED_PERMANENTLY")]
    RejectedPermanently,
    /// Rejected due to a transient issue
    #[serde(rename = "REJECTED_TEMPORARILY")]
    RejectedTemporarily,
    /// Rejected, but not able to determine if the issue is transient
    #[serde(rename = "REJECTED_CANNOT_DETERMINE_IF_PERMANENT")]
    RejectedCannotDetermineIfPermanent,
}

/// Age policy info
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AgeInfo {
    /// The required age
    pub age_requirement: u32,
    /// The current age
    pub current_age: u32,
    /// The verdict
    pub verdict: Verdict,
}

/// Catch-all policy info
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UnspecfiedPolicyInfo {
    /// The verdict
    pub verdict: Verdict,
}

/// Built-on-buildd policy info
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BuiltOnBuildd {
    /// The signers for each architecture
    pub signed_by: HashMap<Architecture, Option<String>>,
    /// The verdict
    pub verdict: Verdict,
}

/// Collected policy infos
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyInfo {
    /// The age policy
    pub age: Option<AgeInfo>,
    /// The buildt-on-buildd policy
    pub builtonbuildd: Option<BuiltOnBuildd>,
    /// All remaining policies
    #[serde(flatten)]
    pub extras: HashMap<String, UnspecfiedPolicyInfo>,
    /*
        autopkgtest: Option<UnspecfiedPolicyInfo>,
        block: Option<UnspecfiedPolicyInfo>,
        build_depends: Option<UnspecfiedPolicyInfo>,
        built_using:  Option<UnspecfiedPolicyInfo>,
        depends: Option<UnspecfiedPolicyInfo>,
        piuparts: Option<UnspecfiedPolicyInfo>,
        rc_bugs: Option<UnspecfiedPolicyInfo>,
    */
}

/// List of missing builds
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MissingBuilds {
    /// Architectures where builds are missing
    pub on_architectures: Vec<Architecture>,
}

/// A source package's excuses
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExcusesItem {
    /// Maintainer of the package
    pub maintainer: Option<String>,
    /// The item is a candidate for migration
    pub is_candidate: bool,
    /// Version in the source suite, i.e., the version to migrate
    pub new_version: String,
    /// Version in the target suite
    pub old_version: String,
    /// Migration item name
    pub item_name: String,
    /// Source package name
    pub source: String,
    /// Migration is blocked by another package
    pub invalidated_by_other_package: Option<bool>,
    /// Component of the source package
    pub component: Option<Component>,
    /// Missing builds
    pub missing_builds: Option<MissingBuilds>,
    /// Policy info
    #[serde(rename = "policy_info")]
    pub policy_info: Option<PolicyInfo>,
    /// The excuses
    pub excuses: Vec<String>,
}

/// Result type
pub type Result<T> = serde_yaml::Result<T>;

/// Read excuses from a reader
pub fn from_reader(reader: impl io::Read) -> Result<Excuses> {
    serde_yaml::from_reader(reader)
}

/// Read excuses from a string
pub fn from_str(data: &str) -> Result<Excuses> {
    serde_yaml::from_str(data)
}
