use std::time::Duration;

const GITHUB_CARGO_TOML: &str =
    "https://raw.githubusercontent.com/lawless-m/Devolver/master/Cargo.toml";

/// Compare this binary's version against Cargo.toml on GitHub master and warn
/// on stderr if they differ. Network or parse failures are silently ignored —
/// the check must never break ingest/push/serve.
pub fn warn_if_outdated() {
    let local = env!("CARGO_PKG_VERSION");
    if let Some(remote) = fetch_github_version() {
        if remote != local {
            eprintln!(
                "WARNING: devlog version mismatch: this binary is {} but GitHub master has {}. \
                 Run 'git pull && make install' to resync.",
                local, remote
            );
        }
    }
}

fn fetch_github_version() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    let body = client.get(GITHUB_CARGO_TOML).send().ok()?.text().ok()?;
    parse_version(&body)
}

fn parse_version(cargo_toml: &str) -> Option<String> {
    cargo_toml
        .lines()
        .find_map(|line| line.trim().strip_prefix("version"))
        .and_then(|rest| rest.split('"').nth(1))
        .map(|v| v.to_string())
}
