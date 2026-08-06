use crate::config::Config;
use crate::index::SessionInfo;
use anyhow::{bail, Context, Result};
use std::io::Write;

/// Sessions older than this can't be resumed: Claude Code prunes its native
/// transcripts after ~30 days (cleanupPeriodDays), and resume replays those,
/// not our devlog copies.
const DAYS: u32 = 30;

/// Menu rows shown per host before collapsing to "… and N more".
const PER_HOST: usize = 15;

pub fn resume() -> Result<()> {
    let sessions = fetch_sessions()?;
    if sessions.is_empty() {
        println!("No resumable sessions found in the last {} days.", DAYS);
        return Ok(());
    }

    let local_host = crate::output::get_machine_id();
    let groups = group_by_host(sessions, &local_host);

    let mut flat: Vec<&SessionInfo> = Vec::new();
    for (host, sessions) in &groups {
        let tag = if host.eq_ignore_ascii_case(&local_host) {
            " (this host)"
        } else {
            ""
        };
        println!("\n{}{}", host, tag);
        for session in sessions.iter().take(PER_HOST) {
            flat.push(session);
            println!(
                "  [{:>2}] {:>3}  {:<20} {}",
                flat.len(),
                format_age(&session.last_activity),
                session.project,
                session.title,
            );
        }
        if sessions.len() > PER_HOST {
            println!("       … and {} more", sessions.len() - PER_HOST);
        }
    }

    let Some(choice) = prompt_choice(flat.len())? else {
        return Ok(());
    };
    launch(flat[choice - 1], &local_host)
}

fn fetch_sessions() -> Result<Vec<SessionInfo>> {
    let config = Config::load()?;
    let endpoint = match config.push {
        Some(pc) => pc.endpoint,
        None => bail!("No push endpoint configured in ~/.devlog/config.toml"),
    };
    let base = endpoint.trim_end_matches('/').trim_end_matches("/ingest");
    let url = format!("{}/api/sessions?days={}", base, DAYS);

    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?
        .get(&url)
        .send()
        .with_context(|| format!("Failed to fetch sessions from {}", url))?;
    if !response.status().is_success() {
        bail!(
            "Session list request failed with status {} (is the server running an older devlog?)",
            response.status()
        );
    }
    response.json().context("Failed to parse session list")
}

/// Group newest-first sessions by machine: local host first, remaining hosts
/// ordered by their most recent activity. Input order (newest first) is
/// preserved within each group.
fn group_by_host(sessions: Vec<SessionInfo>, local_host: &str) -> Vec<(String, Vec<SessionInfo>)> {
    let mut groups: Vec<(String, Vec<SessionInfo>)> = Vec::new();
    for session in sessions {
        match groups.iter_mut().find(|(host, _)| *host == session.machine) {
            Some((_, list)) => list.push(session),
            None => groups.push((session.machine.clone(), vec![session])),
        }
    }
    // Hosts already appear in newest-activity order (first session seen per
    // host is its newest); just float the local host to the top.
    if let Some(pos) = groups
        .iter()
        .position(|(host, _)| host.eq_ignore_ascii_case(local_host))
    {
        let local = groups.remove(pos);
        groups.insert(0, local);
    }
    groups
}

fn prompt_choice(max: usize) -> Result<Option<usize>> {
    print!("\nSelect session (1-{}, blank to cancel): ", max);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    match line.parse::<usize>() {
        Ok(n) if (1..=max).contains(&n) => Ok(Some(n)),
        _ => bail!("Invalid selection: {}", line),
    }
}

fn launch(session: &SessionInfo, local_host: &str) -> Result<()> {
    let status = if session.machine.eq_ignore_ascii_case(local_host) {
        if !std::path::Path::new(&session.project_dir).is_dir() {
            bail!("Project directory no longer exists: {}", session.project_dir);
        }
        eprintln!(
            "Resuming {} in {}",
            session.session_id, session.project_dir
        );
        std::process::Command::new("claude")
            .args(["--resume", &session.session_id])
            .current_dir(&session.project_dir)
            .status()
            .context("Failed to launch claude (is it on PATH?)")?
    } else {
        let remote_cmd = remote_command(&session.project_dir, &session.session_id);
        eprintln!("Resuming on {}: {}", session.machine, remote_cmd);
        std::process::Command::new("ssh")
            .args(["-t", &session.machine, &remote_cmd])
            .status()
            .context("Failed to launch ssh")?
    };
    if !status.success() {
        bail!("claude exited with {}", status);
    }
    Ok(())
}

/// Build the remote shell command. Windows hosts (drive-letter project dirs)
/// get cmd.exe syntax; everything else gets POSIX sh.
fn remote_command(project_dir: &str, session_id: &str) -> String {
    if is_windows_path(project_dir) {
        format!("cd /d \"{}\" && claude --resume {}", project_dir, session_id)
    } else {
        format!(
            "cd '{}' && claude --resume {}",
            project_dir.replace('\'', r"'\''"),
            session_id
        )
    }
}

fn is_windows_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn format_age(last_activity: &str) -> String {
    let Ok(ts) = chrono::DateTime::parse_from_rfc3339(last_activity) else {
        return "?".to_string();
    };
    let minutes = (chrono::Utc::now() - ts.with_timezone(&chrono::Utc)).num_minutes();
    match minutes {
        m if m < 60 => format!("{}m", m.max(0)),
        m if m < 60 * 24 => format!("{}h", m / 60),
        m => format!("{}d", m / (60 * 24)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(machine: &str, id: &str) -> SessionInfo {
        SessionInfo {
            machine: machine.to_string(),
            project: "proj".to_string(),
            project_dir: "/p/proj".to_string(),
            session_id: id.to_string(),
            last_activity: "2026-08-06T10:00:00.000Z".to_string(),
            title: String::new(),
        }
    }

    #[test]
    fn local_host_group_floats_to_top() {
        let sessions = vec![session("beast", "a"), session("roob", "b"), session("beast", "c")];
        let groups = group_by_host(sessions, "ROOB");
        assert_eq!(groups[0].0, "roob");
        assert_eq!(groups[1].0, "beast");
        assert_eq!(groups[1].1.len(), 2);
        // newest-first input order preserved within a group
        assert_eq!(groups[1].1[0].session_id, "a");
    }

    #[test]
    fn remote_command_per_platform() {
        assert_eq!(
            remote_command(r"C:\Git\Retro", "abc"),
            r#"cd /d "C:\Git\Retro" && claude --resume abc"#
        );
        assert_eq!(
            remote_command("/home/matt/it's", "abc"),
            r#"cd '/home/matt/it'\''s' && claude --resume abc"#
        );
    }
}
