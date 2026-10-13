use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Sandbox,
    Production,
}

impl Profile {
    pub fn as_cli_str(self) -> &'static str {
        match self {
            Profile::Sandbox => "sandbox",
            Profile::Production => "production",
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid FLUTE2_PROFILE value `{0}` (expected `sandbox` or `production`)")]
    InvalidProfile(String),
    #[error("invalid FLUTE2_MCP_TIMEOUT_SECS value `{0}` (expected positive integer)")]
    InvalidTimeout(String),
    #[error("could not locate `flute2` on PATH and FLUTE2_BIN is unset")]
    BinaryNotFound,
    #[error("FLUTE2_BIN=`{0}` does not exist or is not executable")]
    BinaryUnusable(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub profile: Profile,
    pub binary: PathBuf,
    pub merchant_id: Option<String>,
    pub timeout: Duration,
    pub debug: bool,
    pub allow_prod_writes: bool,
}

/// Only the exact values `1`, `true`, `yes` and `on` enable a flag; no
/// trimming or case folding, so `FLUTE2_MCP_ALLOW_PROD_WRITES=false` (or
/// `TRUE`, or `" true "`) does not unlock production writes.
fn truthy(value: Option<String>) -> bool {
    matches!(value.as_deref(), Some("1" | "true" | "yes" | "on"))
}

/// Whether `path` is a file we can execute, checked at startup so a
/// non-executable binary fails before the first tool call rather than during it.
#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

impl Config {
    /// Build a Config from a closure returning environment values, so tests
    /// inject an environment instead of mutating the process's.
    pub fn from_env<F>(getenv: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let profile = match getenv("FLUTE2_PROFILE").as_deref() {
            None | Some("") | Some("sandbox") => Profile::Sandbox,
            Some("production") | Some("prod") => Profile::Production,
            Some(other) => return Err(ConfigError::InvalidProfile(other.to_string())),
        };

        let timeout = match getenv("FLUTE2_MCP_TIMEOUT_SECS") {
            None => Duration::from_secs(30),
            Some(s) => {
                let n: u64 = s
                    .parse()
                    .map_err(|_| ConfigError::InvalidTimeout(s.clone()))?;
                if n == 0 {
                    return Err(ConfigError::InvalidTimeout(s));
                }
                Duration::from_secs(n)
            }
        };

        let binary = match getenv("FLUTE2_BIN") {
            Some(p) if !p.is_empty() => {
                let path = PathBuf::from(&p);
                if !is_executable_file(&path) {
                    return Err(ConfigError::BinaryUnusable(p));
                }
                path
            }
            _ => which::which("flute2").map_err(|_| ConfigError::BinaryNotFound)?,
        };

        let merchant_id = getenv("FLUTE2_MERCHANT_ID").filter(|s| !s.is_empty());
        let debug = truthy(getenv("FLUTE2_MCP_DEBUG"));
        let allow_prod_writes = truthy(getenv("FLUTE2_MCP_ALLOW_PROD_WRITES"));

        Ok(Self {
            profile,
            binary,
            merchant_id,
            timeout,
            debug,
            allow_prod_writes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn make_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn fake_binary(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("flute2");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[test]
    fn defaults_to_sandbox_30s_no_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let pairs = [("FLUTE2_BIN", bin.to_str().unwrap())];
        let cfg = Config::from_env(make_env(&pairs)).unwrap();
        assert_eq!(cfg.profile, Profile::Sandbox);
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert!(!cfg.debug);
        assert!(!cfg.allow_prod_writes);
        assert_eq!(cfg.merchant_id, None);
    }

    #[test]
    fn accepts_production_and_prod_alias() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["production", "prod"] {
            let pairs = [
                ("FLUTE2_BIN", bin.to_str().unwrap()),
                ("FLUTE2_PROFILE", value),
            ];
            assert_eq!(
                Config::from_env(make_env(&pairs)).unwrap().profile,
                Profile::Production
            );
        }
    }

    #[test]
    fn reads_merchant_id_timeout_and_debug() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let pairs = [
            ("FLUTE2_BIN", bin.to_str().unwrap()),
            ("FLUTE2_MERCHANT_ID", "m-123"),
            ("FLUTE2_MCP_TIMEOUT_SECS", "90"),
            ("FLUTE2_MCP_DEBUG", "on"),
        ];
        let cfg = Config::from_env(make_env(&pairs)).unwrap();
        assert_eq!(cfg.merchant_id.as_deref(), Some("m-123"));
        assert_eq!(cfg.timeout, Duration::from_secs(90));
        assert!(cfg.debug);
    }

    #[test]
    fn false_like_values_do_not_enable_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["false", "0", "no", "off", "", "TRUE", "On", "YES", " true "] {
            let pairs = [
                ("FLUTE2_BIN", bin.to_str().unwrap()),
                ("FLUTE2_MCP_ALLOW_PROD_WRITES", value),
            ];
            assert!(
                !Config::from_env(make_env(&pairs))
                    .unwrap()
                    .allow_prod_writes,
                "value {value:?} must NOT enable production writes"
            );
        }
    }

    #[test]
    fn truthy_values_enable_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["1", "true", "yes", "on"] {
            let pairs = [
                ("FLUTE2_BIN", bin.to_str().unwrap()),
                ("FLUTE2_MCP_ALLOW_PROD_WRITES", value),
            ];
            assert!(
                Config::from_env(make_env(&pairs))
                    .unwrap()
                    .allow_prod_writes,
                "value {value:?} must enable production writes"
            );
        }
    }

    #[test]
    fn rejects_unknown_profile_and_zero_timeout() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let profile_pairs = [
            ("FLUTE2_BIN", bin.to_str().unwrap()),
            ("FLUTE2_PROFILE", "staging"),
        ];
        assert!(matches!(
            Config::from_env(make_env(&profile_pairs)),
            Err(ConfigError::InvalidProfile(s)) if s == "staging"
        ));
        let timeout_pairs = [
            ("FLUTE2_BIN", bin.to_str().unwrap()),
            ("FLUTE2_MCP_TIMEOUT_SECS", "0"),
        ];
        assert!(matches!(
            Config::from_env(make_env(&timeout_pairs)),
            Err(ConfigError::InvalidTimeout(s)) if s == "0"
        ));
    }

    #[test]
    fn non_executable_or_missing_binary_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("flute2");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
        let present = [("FLUTE2_BIN", path.to_str().unwrap())];
        assert!(matches!(
            Config::from_env(make_env(&present)),
            Err(ConfigError::BinaryUnusable(_))
        ));
        let absent = [("FLUTE2_BIN", "/nope/does/not/exist")];
        assert!(matches!(
            Config::from_env(make_env(&absent)),
            Err(ConfigError::BinaryUnusable(_))
        ));
    }
}
