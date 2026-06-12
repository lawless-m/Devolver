use std::time::Duration;

const GITHUB_COMMITS_API: &str =
    "https://api.github.com/repos/lawless-m/Devolver/commits/master";

/// Compare the commit this binary was built from against GitHub master HEAD.
/// Returns the warning message if they differ. Network or parse failures are
/// silently ignored — the check must never break ingest/push/serve.
pub fn outdated_warning() -> Option<String> {
    let local = env!("GIT_COMMIT");
    if local.is_empty() {
        return None; // built outside a git checkout
    }
    let remote = fetch_github_master_sha()?;
    if remote == local {
        return None;
    }
    Some(format!(
        "WARNING: devlog binary is built from commit {} but GitHub master is at {}. \
         Run 'git pull' and reinstall.",
        &local[..7.min(local.len())],
        &remote[..7.min(remote.len())]
    ))
}

pub fn warn_if_outdated() {
    if let Some(warning) = outdated_warning() {
        eprintln!("{}", warning);
    }
}

fn fetch_github_master_sha() -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    let body = client
        .get(GITHUB_COMMITS_API)
        .header("User-Agent", "devlog-version-check")
        .send()
        .ok()?
        .text()
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    json.get("sha")?.as_str().map(|s| s.to_string())
}
