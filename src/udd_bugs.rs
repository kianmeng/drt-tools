use std::{collections::HashMap, fmt::Display, io::Read};

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Wishlist,
    Normal,
    Important,
    Serious,
    Grave,
    Critical,
}

/*
impl Severity {
    fn is_rc(&self) -> bool {
        self >= &Severity::Serious
    }
}
*/

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Wishlist => write!(f, "wishlist"),
            Severity::Normal => write!(f, "normal"),
            Severity::Important => write!(f, "important"),
            Severity::Serious => write!(f, "serious"),
            Severity::Grave => write!(f, "grave"),
            Severity::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UDDBug {
    pub id: u32,
    pub source: String,
    pub severity: Severity,
    pub title: String,
}

#[derive(Debug, Default)]
pub struct UDDBugs {
    bugs: Vec<UDDBug>,
    source_index: HashMap<String, Vec<usize>>,
}

impl UDDBugs {
    pub fn new(bugs: Vec<UDDBug>) -> Self {
        let mut udd_bugs = Self {
            bugs,
            ..Default::default()
        };

        for (idx, bug) in (&udd_bugs.bugs).iter().enumerate() {
            if udd_bugs.source_index.contains_key(&bug.source) {
                udd_bugs
                    .source_index
                    .get_mut(&bug.source)
                    .unwrap()
                    .push(idx);
            } else {
                udd_bugs.source_index.insert(bug.source.clone(), vec![idx]);
            }
        }

        udd_bugs
    }

    pub fn bugs_for_source(&self, source: &str) -> Option<Vec<UDDBug>> {
        self.source_index
            .get(source)
            .map(|indices| indices.iter().map(|idx| self.bugs[*idx].clone()).collect())
    }
}

pub fn load_bugs_from_reader(reader: impl Read) -> Result<UDDBugs> {
    serde_yaml::from_reader(reader)
        .map_err(|e| e.into())
        .map(UDDBugs::new)
}

#[cfg(test)]
mod test {
    use super::{load_bugs_from_reader, Severity};

    const TEST_DATA: &str = r#"
---
- id: 743062
  package: src:mutextrace
  source: mutextrace
  severity: serious
  title: 'mutextrace: sometimes FTBFS: testsuite races'
  last_modified: '2021-08-16'
  status: pending
  affects_stable: false
  affects_testing: false
  affects_unstable: true
  affects_experimental: false
  last_modified_full: '2021-08-16 07:03:39 +0000'
  autormdate: ''
- id: 778111
  package: src:scheme2c
  source: scheme2c
  severity: serious
  title: 'scheme2c: ftbfs with GCC-5'
  last_modified: '2021-08-16'
  status: pending
  affects_stable: false
  affects_testing: false
  affects_unstable: true
  affects_experimental: false
  last_modified_full: '2021-08-16 07:03:46 +0000'
  autormdate: ''
- id: 789292
  package: src:dmtcp
  source: dmtcp
  severity: serious
  title: 'dmtcp: FTBFS with glibc-2.21 and gcc-5'
  last_modified: '2021-11-16'
  status: forwarded
  affects_stable: false
  affects_testing: false
  affects_unstable: true
  affects_experimental: false
  last_modified_full: '2021-11-16 23:03:16 +0000'
  autormdate: ''
"#;

    #[test]
    fn read_bugs() {
        let bugs = load_bugs_from_reader(TEST_DATA.as_bytes()).unwrap();

        assert!(bugs.bugs_for_source("dmtcp").is_some());
        assert!(bugs.bugs_for_source("zathura").is_none());

        for bug in bugs.bugs_for_source("mutextrace").unwrap() {
            assert!(bug.severity >= Severity::Serious);
            // assert!(bug.severity.is_rc());
        }
    }
}
