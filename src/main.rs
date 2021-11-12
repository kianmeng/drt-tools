use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use serde::Deserialize;
use xdg::BaseDirectories;

use drt_tools::*;

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Excuses {
    generated_date: String,
    sources: Vec<ExcusesItem>,
}

#[derive(Debug, Deserialize, PartialEq)]
enum Verdict {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "PASS_HINTED")]
    PassHinted,
    #[serde(rename = "REJECTED_NEEDS_APPROVAL")]
    RejectedNeedsApproval,
    #[serde(rename = "REJECTED_PERMANENTLY")]
    RejectedPermanently,
    #[serde(rename = "REJECTED_TEMPORARILY")]
    RejectedTemporarily,
    #[serde(rename = "REJECTED_CANNOT_DETERMINE_IF_PERMANENT")]
    RejectedCannotDetermineIfPermanent,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Eq)]
#[serde(rename_all = "lowercase")]
enum Architecture {
    All,
    Amd64,
    Arm64,
    Armel,
    Armhf,
    I386,
    Mips64el,
    Mipsel,
    Ppc64el,
    S390x,
}

impl Display for Architecture {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Architecture::All => "all",
            Architecture::Amd64 => "amd64",
            Architecture::Arm64 => "arm64",
            Architecture::Armel => "armel",
            Architecture::Armhf => "armhf",
            Architecture::I386 => "i386",
            Architecture::Mips64el => "mips64el",
            Architecture::Mipsel => "mipsel",
            Architecture::Ppc64el => "ppc64el",
            Architecture::S390x => "s390x",
        })
    }
}

const RELEASE_ARCHITECTURES: [Architecture; 9] = [
    Architecture::Amd64,
    Architecture::Arm64,
    Architecture::Armel,
    Architecture::Armhf,
    Architecture::I386,
    Architecture::Ppc64el,
    Architecture::Mipsel,
    Architecture::Mips64el,
    Architecture::S390x,
];

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Component {
    Main,
    Contrib,
    #[serde(rename = "non-free")]
    NonFree,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct AgeInfo {
    age_requirement: u32,
    current_age: u32,
    verdict: Verdict,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct UnspecfiedPolicyInfo {
    verdict: Verdict,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuiltOnBuildd {
    signed_by: HashMap<Architecture, String>,
    verdict: Verdict,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PolicyInfo {
    age: Option<AgeInfo>,
    builtonbuildd: Option<BuiltOnBuildd>,
    #[serde(flatten)]
    extras: HashMap<String, UnspecfiedPolicyInfo>,
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

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct MissingBuilds {
    on_architectures: Vec<Architecture>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ExcusesItem {
    is_candidate: bool,
    new_version: String,
    old_version: String,
    item_name: String,
    source: String,
    invalidated_by_other_package: Option<bool>,
    component: Option<Component>,
    missing_builds: Option<MissingBuilds>,
    #[serde(rename = "policy_info")]
    policy_info: Option<PolicyInfo>,
}

fn check_if_binnmu_required(policy_info: &PolicyInfo) -> bool {
    if let Some(b) = &policy_info.builtonbuildd {
        if b.verdict == Verdict::Pass {
            // nothing to do
            return false;
        }
    }
    if let Some(a) = &policy_info.age {
        if a.current_age < min(a.age_requirement / 2, a.age_requirement - 1) {
            // too young
            return false;
        }
    }

    // if they others do not pass, would not migrate even if binNMUed
    policy_info
        .extras
        .values()
        .all(|info| info.verdict == Verdict::Pass)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ToBinNMU {
    source: String,
    version: String,
    architectures: Vec<Architecture>,
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
struct BinaryPackage {
    // until https://github.com/Kixunil/rfc822-like/issues/1 is fixed, use an empty string as default value instead of Option<String>
    #[serde(default = "String::new")]
    source: String,
    package: String,
    #[serde(default = "String::new")]
    multi_arch: String,
}

struct SourcePackages {
    ma_same_sources: HashSet<String>,
}

impl SourcePackages {
    fn new<P>(paths: &[P]) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let pb_style = ProgressStyle::default_bar()
            .template("{msg}: {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({per_sec}, {eta})")
            .progress_chars("█  ");

        let mut ma_same_sources = HashSet::<String>::new();
        for path in paths {
            let sources = Self::parse_packages(path, &pb_style);
            ma_same_sources.extend(sources?);
        }

        Ok(Self { ma_same_sources })
    }

    fn parse_packages<P>(path: P, pb_style: &ProgressStyle) -> Result<HashSet<String>>
    where
        P: AsRef<Path>,
    {
        let mut ma_same_sources = HashSet::<String>::new();

        let package_content = fs::read_to_string(&path)?;
        let binary_packages: Vec<BinaryPackage> = rfc822_like::from_str(&package_content)?;
        let pb = ProgressBar::new(binary_packages.len() as u64);
        pb.set_style(pb_style.clone());
        pb.set_message(&format!("Processing {}", path.as_ref().display()));
        for binary_package in binary_packages.into_iter().progress_with(pb) {
            if binary_package.multi_arch == "same" {
                if binary_package.source.len() > 0 {
                    ma_same_sources.insert(
                        binary_package
                            .source
                            .split_whitespace()
                            .next()
                            .unwrap()
                            .into(),
                    );
                } else {
                    // not Source set, so Source == Package
                    ma_same_sources.insert(binary_package.package);
                }
            }
        }

        Ok(ma_same_sources)
    }

    fn is_ma_same(&self, source: &str) -> bool {
        self.ma_same_sources.contains(source)
    }
}

struct ProcessExcuses {
    base_directory: BaseDirectories,
}

impl ProcessExcuses {
    fn new() -> Result<Self> {
        Ok(Self {
            base_directory: BaseDirectories::with_prefix("Debian-RT-tools")?,
        })
    }

    async fn download_to_cache(&self) -> Result<bool> {
        let downloader = Downloader::new(true);

        let urls = [(
            "https://release.debian.org/britney/excuses.yaml",
            "excuses.yaml",
        )];
        for (url, dst) in urls {
            if !downloader
                .download_file(
                    url,
                    self.get_cache_path(dst)?
                        .to_str()
                        .ok_or_else(|| anyhow!("Failed to produce path"))?,
                )
                .await?
            {
                return Ok(false);
            }
        }
        for architecture in RELEASE_ARCHITECTURES {
            let url = format!(
                "https://deb.debian.org/debian/dists/unstable/main/binary-{}/Packages.xz",
                architecture
            );
            let dest = format!("Packages_{}", architecture);
            downloader
                .download_file_unxz(
                    &url,
                    self.get_cache_path(&dest)?
                        .to_str()
                        .ok_or_else(|| anyhow!("Failed to produce path"))?,
                )
                .await?;
        }

        Ok(true)
    }

    fn get_cache_path<P>(&self, path: P) -> Result<PathBuf>
    where
        P: AsRef<Path>,
    {
        Ok(self.base_directory.place_cache_file(path)?)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let process_excuses = ProcessExcuses::new()?;
    if !process_excuses.download_to_cache().await? {
        // nothing to do
        return Ok(());
    }

    let mut all_paths = vec![];
    for architecture in RELEASE_ARCHITECTURES {
        all_paths.push(process_excuses.get_cache_path(format!("Packages_{}", architecture))?);
    }
    let source_packages = SourcePackages::new(&all_paths)?;

    let mut to_binnmu = vec![];
    let excuses: Excuses = serde_yaml::from_reader(BufReader::new(
        File::open(process_excuses.get_cache_path("excuses.yaml")?).unwrap(),
    ))
    .unwrap();
    let pb = ProgressBar::new(excuses.sources.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{msg}: {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({per_sec}, {eta})")
        .progress_chars("█  "));
    pb.set_message("Processing excuses");
    for item in excuses.sources.iter().progress_with(pb) {
        if item.new_version == "-" {
            // skip removals
            continue;
        }
        if item.new_version == item.old_version {
            // skip binNMUs
            continue;
        }
        if item.item_name.ends_with("_pu") {
            // skip PU requests
            continue;
        }
        match item.component {
            Some(Component::Main) => {}
            None => {}
            _ => {
                // skip non-free and contrib
                continue;
            }
        }
        if let Some(true) = item.invalidated_by_other_package {
            // skip otherwise blocked packages
            continue;
        }
        if item.missing_builds.is_some() {
            // skip packages with missing builds
            continue;
        }

        if let Some(policy_info) = &item.policy_info {
            if !check_if_binnmu_required(policy_info) {
                continue;
            }

            let mut archs: Vec<Architecture> = vec![];
            for (arch, signer) in policy_info.builtonbuildd.as_ref().unwrap().signed_by.iter() {
                if !signer.ends_with("@buildd.debian.org") {
                    archs.push(arch.to_owned());
                }
            }
            if archs.contains(&Architecture::All) {
                // cannot binNMU arch:all
                continue;
            }

            to_binnmu.push(ToBinNMU {
                source: item.source.clone(),
                version: item.new_version.clone(),
                architectures: archs,
            });
        }
    }

    for info in to_binnmu {
        println!(
            "nmu {}_{} . {} . unstable . -m \"Rebuild on buildd\"",
            info.source,
            info.version,
            if source_packages.is_ma_same(&info.source) {
                "ANY".to_string()
            } else {
                info.architectures
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            }
        );
    }

    Ok(())
}
