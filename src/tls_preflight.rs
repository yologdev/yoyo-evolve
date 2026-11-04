//! TLS CA-environment pre-flight (#1138, yoyo's half).
//!
//! On Linux-like targets reqwest's TLS verifier (rustls-platform-verifier)
//! builds its trust store from `rustls_native_certs::load_native_certs()`. When
//! `SSL_CERT_FILE` or `SSL_CERT_DIR` is set, that loader reads ONLY those paths
//! (not the system store), and if they yield zero certificates the verifier
//! errors and yoagent's `reqwest::Client::new()` panics inside a tokio worker.
//! The user then sees a panic backtrace line and `(reply cut off: ...)`, exit 1,
//! and no word about which variable is wrong. Even an `http://` base URL hits
//! it: the client builds its TLS config whether or not it will use it.
//!
//! The root fix (no panic) is upstream, on #1138. This module refuses the
//! panicking environment before any agent is built and names the variable,
//! the path and what is wrong with it.
//!
//! **The decision is the loader's own outcome, not a re-implementation of it.**
//! [`loaded_cert_count`] calls the same `rustls-native-certs` crate the
//! verifier calls, on the same paths, and the refusal fires only when it loads
//! zero certificates — the exact condition of the panic. Measured on the built
//! binary (Day 249), every shape below followed that rule:
//!
//! | env                                                   | outcome   |
//! |-------------------------------------------------------|-----------|
//! | `SSL_CERT_FILE` missing / empty / text, no PEM        | panic     |
//! | `SSL_CERT_FILE` = `""` / a directory / bad base64 PEM | panic     |
//! | `SSL_CERT_DIR` missing / empty / only non-PEM files   | panic     |
//! | `SSL_CERT_FILE` = system bundle                       | works     |
//! | bundle + garbage appended, or garbage + bundle        | works     |
//! | bad `SSL_CERT_FILE` + valid `SSL_CERT_DIR`            | works     |
//! | valid `SSL_CERT_FILE` + missing `SSL_CERT_DIR`        | works     |
//! | `SSL_CERT_DIR=missing:/etc/ssl/certs`                 | works     |
//! | `SSL_CERT_DIR=""`                                     | works     |
//!
//! So a mixed environment where any path loads a certificate is never refused.
//! The per-path wording ([`PathProblem`]) is only diagnosis for the message;
//! it never decides.
//!
//! Known gap, stated rather than implied: a PEM block whose base64 decodes but
//! is not a parsable X.509 certificate counts as loaded here, while the
//! verifier's `add_parsable_certificates` drops it. That shape still panics as
//! before (fails open to the old behaviour, never refuses a working env).
//!
//! On macOS, Windows and Android the verifier does not read these variables,
//! so the pre-flight is a no-op there: refusing would be a new bug.

#![cfg_attr(
    not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))),
    allow(dead_code)
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::cli::sanitize_for_display;

pub(crate) const ENV_CERT_FILE: &str = "SSL_CERT_FILE";
pub(crate) const ENV_CERT_DIR: &str = "SSL_CERT_DIR";

/// What is wrong with one configured path. Diagnosis only: by the time one of
/// these is rendered, the loader has already reported zero certificates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathProblem {
    /// The variable is set to the empty string.
    Blank,
    Missing,
    Empty,
    NoPem,
    WrongKind(&'static str),
    Unreadable(String),
}

impl PathProblem {
    fn describe(&self) -> String {
        match self {
            PathProblem::Blank => "which is not a path".to_string(),
            PathProblem::Missing => "which does not exist".to_string(),
            PathProblem::Empty => "which is empty".to_string(),
            PathProblem::NoPem => "which contains no PEM certificates".to_string(),
            PathProblem::WrongKind(what) => format!("which {what}"),
            PathProblem::Unreadable(e) => {
                format!("which could not be read ({})", sanitize_for_display(e))
            }
        }
    }
}

/// Read the two variables the way `rustls-native-certs` 0.8 does
/// (`CertPaths::from_env`): `SSL_CERT_FILE` counts as set even when it is the
/// empty string, and `SSL_CERT_DIR` is split on the platform path separator
/// with empty entries dropped (so `SSL_CERT_DIR=""` is the same as unset).
pub(crate) fn ca_paths_from(
    file: Option<OsString>,
    dir: Option<OsString>,
) -> (Option<PathBuf>, Vec<PathBuf>) {
    let file = file.map(PathBuf::from);
    let dirs = match dir {
        Some(d) => std::env::split_paths(&d)
            .filter(|p| !p.as_os_str().is_empty())
            .collect(),
        None => Vec::new(),
    };
    (file, dirs)
}

/// The pure decision + message. `certs_loaded` is what the real loader found
/// over `file` and `dirs`; the two closures describe a path for the message.
///
/// `None` when neither variable is in effect (the loader then reads the system
/// store, which this pre-flight does not judge) or when any certificate loaded.
pub(crate) fn ca_env_problem(
    file: Option<&Path>,
    dirs: &[PathBuf],
    certs_loaded: usize,
    file_problem: impl Fn(&Path) -> PathProblem,
    dir_problem: impl Fn(&Path) -> PathProblem,
) -> Option<String> {
    if (file.is_none() && dirs.is_empty()) || certs_loaded > 0 {
        return None;
    }
    let mut out = String::from(
        "error: no CA certificates could be loaded, so yoyo cannot open any HTTPS \
         connection (including to the model provider).\n",
    );
    if let Some(f) = file {
        let shown = sanitize_for_display(&f.to_string_lossy());
        let problem = file_problem(f);
        if problem == PathProblem::Blank {
            out.push_str(&format!("  {ENV_CERT_FILE} is set to an empty value.\n"));
        } else {
            out.push_str(&format!(
                "  {ENV_CERT_FILE} is \"{shown}\", {}.\n",
                problem.describe()
            ));
        }
    }
    for d in dirs {
        let shown = sanitize_for_display(&d.to_string_lossy());
        let label = if dirs.len() == 1 {
            format!("{ENV_CERT_DIR} is")
        } else {
            format!("{ENV_CERT_DIR} entry")
        };
        out.push_str(&format!(
            "  {label} \"{shown}\", {}.\n",
            dir_problem(d).describe()
        ));
    }
    let remedy = match (file.is_some(), !dirs.is_empty()) {
        (true, false) => format!("Point {ENV_CERT_FILE} at a PEM CA bundle, or unset it."),
        (false, true) => {
            format!("Point {ENV_CERT_DIR} at a directory of PEM CA certificates, or unset it.")
        }
        _ => format!(
            "Point {ENV_CERT_FILE} at a PEM CA bundle or {ENV_CERT_DIR} at a directory \
             of PEM CA certificates, or unset both."
        ),
    };
    out.push_str(&remedy);
    Some(out)
}

/// Describe a configured `SSL_CERT_FILE` path from the real filesystem.
pub(crate) fn fs_file_problem(p: &Path) -> PathProblem {
    if p.as_os_str().is_empty() {
        return PathProblem::Blank;
    }
    match std::fs::metadata(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PathProblem::Missing,
        Err(e) => PathProblem::Unreadable(e.to_string()),
        Ok(m) if m.is_dir() => PathProblem::WrongKind("is a directory, not a file"),
        Ok(m) if m.len() == 0 => PathProblem::Empty,
        Ok(_) => match std::fs::File::open(p) {
            Err(e) => PathProblem::Unreadable(e.to_string()),
            Ok(_) => PathProblem::NoPem,
        },
    }
}

/// Describe one configured `SSL_CERT_DIR` entry from the real filesystem.
pub(crate) fn fs_dir_problem(p: &Path) -> PathProblem {
    match std::fs::metadata(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PathProblem::Missing,
        Err(e) => PathProblem::Unreadable(e.to_string()),
        Ok(m) if !m.is_dir() => PathProblem::WrongKind("is not a directory"),
        Ok(_) => match std::fs::read_dir(p) {
            Err(e) => PathProblem::Unreadable(e.to_string()),
            Ok(mut rd) => {
                if rd.next().is_none() {
                    PathProblem::Empty
                } else {
                    PathProblem::NoPem
                }
            }
        },
    }
}

/// How many certificates the verifier's own loader finds on these paths.
#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
pub(crate) fn loaded_cert_count(file: Option<&Path>, dirs: &[PathBuf]) -> usize {
    let mut n = match file {
        Some(f) => rustls_native_certs::load_certs_from_paths(Some(f), None)
            .certs
            .len(),
        None => 0,
    };
    for d in dirs {
        n += rustls_native_certs::load_certs_from_paths(None, Some(d))
            .certs
            .len();
    }
    n
}

/// Run the pre-flight against the process environment. `Some(message)` means
/// building a provider client would panic; print it and exit 1.
pub(crate) fn preflight_ca_env() -> Option<String> {
    #[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
    {
        let (file, dirs) = ca_paths_from(
            std::env::var_os(ENV_CERT_FILE),
            std::env::var_os(ENV_CERT_DIR),
        );
        if file.is_none() && dirs.is_empty() {
            return None;
        }
        let n = loaded_cert_count(file.as_deref(), &dirs);
        ca_env_problem(file.as_deref(), &dirs, n, fs_file_problem, fs_dir_problem)
    }
    #[cfg(not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(p: PathProblem) -> impl Fn(&Path) -> PathProblem {
        move |_| p.clone()
    }

    fn never(_: &Path) -> PathProblem {
        panic!("a path was described although the env must not be refused")
    }

    #[test]
    fn empty_file_renders_the_full_message() {
        // Probe (Day 249): SSL_CERT_FILE=<empty file> -> reqwest Client::new() panic.
        let f = PathBuf::from("/tmp/corp-ca.pem");
        let got = ca_env_problem(Some(&f), &[], 0, fixed(PathProblem::Empty), never);
        assert_eq!(
            got.as_deref(),
            Some(
                "error: no CA certificates could be loaded, so yoyo cannot open any HTTPS \
                 connection (including to the model provider).\n  \
                 SSL_CERT_FILE is \"/tmp/corp-ca.pem\", which is empty.\n\
                 Point SSL_CERT_FILE at a PEM CA bundle, or unset it."
            )
        );
    }

    #[test]
    fn refuses_each_measured_panic_shape_naming_variable_and_problem() {
        let f = PathBuf::from("/x/ca.pem");
        // (problem, expected fragment) -- each row is a shape probed to panic.
        let file_rows = [
            (
                PathProblem::Missing,
                "SSL_CERT_FILE is \"/x/ca.pem\", which does not exist.",
            ),
            (
                PathProblem::Empty,
                "SSL_CERT_FILE is \"/x/ca.pem\", which is empty.",
            ),
            (
                PathProblem::NoPem,
                "SSL_CERT_FILE is \"/x/ca.pem\", which contains no PEM certificates.",
            ),
            (
                PathProblem::WrongKind("is a directory, not a file"),
                "SSL_CERT_FILE is \"/x/ca.pem\", which is a directory, not a file.",
            ),
            (
                PathProblem::Blank,
                "SSL_CERT_FILE is set to an empty value.",
            ),
        ];
        for (problem, want) in file_rows {
            let msg = ca_env_problem(Some(&f), &[], 0, fixed(problem.clone()), never)
                .unwrap_or_else(|| panic!("{problem:?} must be refused"));
            assert!(msg.contains(want), "{problem:?}: {msg}");
            assert!(msg.ends_with("Point SSL_CERT_FILE at a PEM CA bundle, or unset it."));
        }
        let d = vec![PathBuf::from("/x/certs")];
        let dir_rows = [
            (
                PathProblem::Missing,
                "SSL_CERT_DIR is \"/x/certs\", which does not exist.",
            ),
            (
                PathProblem::Empty,
                "SSL_CERT_DIR is \"/x/certs\", which is empty.",
            ),
            (
                PathProblem::NoPem,
                "SSL_CERT_DIR is \"/x/certs\", which contains no PEM certificates.",
            ),
        ];
        for (problem, want) in dir_rows {
            let msg = ca_env_problem(None, &d, 0, never, fixed(problem.clone()))
                .unwrap_or_else(|| panic!("{problem:?} must be refused"));
            assert!(msg.contains(want), "{problem:?}: {msg}");
            assert!(msg.ends_with(
                "Point SSL_CERT_DIR at a directory of PEM CA certificates, or unset it."
            ));
        }
    }

    #[test]
    fn both_variables_bad_names_both_and_says_unset_both() {
        let f = PathBuf::from("/x/ca.pem");
        let d = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let msg = ca_env_problem(
            Some(&f),
            &d,
            0,
            fixed(PathProblem::Missing),
            fixed(PathProblem::Empty),
        )
        .unwrap();
        assert!(msg.contains("SSL_CERT_FILE is \"/x/ca.pem\", which does not exist."));
        assert!(msg.contains("SSL_CERT_DIR entry \"/a\", which is empty."));
        assert!(msg.contains("SSL_CERT_DIR entry \"/b\", which is empty."));
        assert!(msg.ends_with("or unset both."));
    }

    #[test]
    fn never_refuses_when_unset_or_when_any_certificate_loaded() {
        // Unset/unset: the loader reads the system store; not judged here.
        assert_eq!(ca_env_problem(None, &[], 0, never, never), None);
        // Probed to WORK (Day 249): valid bundle; bundle+garbage either order;
        // bad file + valid dir; valid file + missing dir; missing:valid dir list.
        // All of those load >0 certificates, and that is the only thing that
        // decides -- the path descriptions are never consulted.
        let f = PathBuf::from("/bad");
        let d = vec![PathBuf::from("/missing"), PathBuf::from("/etc/ssl/certs")];
        assert_eq!(ca_env_problem(Some(&f), &[], 1, never, never), None);
        assert_eq!(ca_env_problem(None, &d, 140, never, never), None);
        assert_eq!(ca_env_problem(Some(&f), &d, 3, never, never), None);
    }

    #[test]
    fn hostile_path_bytes_are_escaped() {
        let f = PathBuf::from("/x/\u{1b}[31mca.pem");
        let msg = ca_env_problem(Some(&f), &[], 0, fixed(PathProblem::Missing), never).unwrap();
        assert!(
            f.to_string_lossy().contains('\u{1b}'),
            "fixture must be hostile"
        );
        assert!(!msg.as_bytes().contains(&0x1b), "{msg:?}");
        assert!(msg.contains("\\x1b[31mca.pem"));
    }

    #[test]
    fn env_parsing_mirrors_rustls_native_certs() {
        // SSL_CERT_FILE="" counts as set (probed: it panics).
        let (f, d) = ca_paths_from(Some(OsString::from("")), None);
        assert_eq!(f, Some(PathBuf::new()));
        assert!(d.is_empty());
        // SSL_CERT_DIR="" is the same as unset (probed: it works).
        let (f, d) = ca_paths_from(None, Some(OsString::from("")));
        assert_eq!(f, None);
        assert!(d.is_empty());
        let (_, d) = ca_paths_from(None, Some(OsString::from("/a::/b")));
        assert_eq!(d, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn fs_problems_describe_real_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = tmp.path().join("empty.pem");
        std::fs::write(&empty, b"").unwrap();
        let text = tmp.path().join("text.pem");
        std::fs::write(&text, b"hello garbage\n").unwrap();
        let emptydir = tmp.path().join("emptydir");
        std::fs::create_dir(&emptydir).unwrap();
        let missing = tmp.path().join("nope");

        assert_eq!(fs_file_problem(Path::new("")), PathProblem::Blank);
        assert_eq!(fs_file_problem(&missing), PathProblem::Missing);
        assert_eq!(fs_file_problem(&empty), PathProblem::Empty);
        assert_eq!(fs_file_problem(&text), PathProblem::NoPem);
        assert_eq!(
            fs_file_problem(&emptydir),
            PathProblem::WrongKind("is a directory, not a file")
        );
        assert_eq!(fs_dir_problem(&missing), PathProblem::Missing);
        assert_eq!(fs_dir_problem(&emptydir), PathProblem::Empty);
        assert_eq!(fs_dir_problem(tmp.path()), PathProblem::NoPem);
        assert_eq!(
            fs_dir_problem(&text),
            PathProblem::WrongKind("is not a directory")
        );
    }

    #[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
    #[test]
    fn loader_count_is_zero_for_panic_shapes_and_positive_for_a_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = tmp.path().join("empty.pem");
        std::fs::write(&empty, b"").unwrap();
        let text = tmp.path().join("text.pem");
        std::fs::write(&text, b"hello garbage\n").unwrap();
        let missing = tmp.path().join("nope");
        assert_eq!(loaded_cert_count(Some(&empty), &[]), 0);
        assert_eq!(loaded_cert_count(Some(&text), &[]), 0);
        assert_eq!(loaded_cert_count(Some(&missing), &[]), 0);
        assert_eq!(loaded_cert_count(None, std::slice::from_ref(&missing)), 0);

        let bundle = Path::new("/etc/ssl/certs/ca-certificates.crt");
        if !bundle.is_file() {
            eprintln!(
                "skip positive half: no system bundle at {}",
                bundle.display()
            );
            return;
        }
        // Garbage appended after a real bundle still loads (probed to work).
        let mixed = tmp.path().join("mixed.pem");
        let mut bytes = std::fs::read(bundle).unwrap();
        bytes.extend_from_slice(b"\nhello garbage\n");
        std::fs::write(&mixed, bytes).unwrap();
        assert!(loaded_cert_count(Some(bundle), &[]) > 0);
        assert!(loaded_cert_count(Some(&mixed), &[]) > 0);
        assert!(loaded_cert_count(Some(&missing), &[bundle.parent().unwrap().into()]) > 0);
    }
}
