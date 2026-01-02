use crate::output::DevlogOutput;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct ProjectStats {
    pub machine: String,
    pub project: String,
    pub session_count: usize,
    pub prompt_count: usize,
    pub tool_calls: usize,
    pub files_touched: usize,
    pub prompt_words: usize,
    pub response_words: usize,
    pub last_activity: String,
}

pub fn get_project_stats(storage_dir: &Path, days: u32) -> Result<Vec<ProjectStats>> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let mut stats: HashMap<(String, String), ProjectStats> = HashMap::new();

    if !storage_dir.exists() {
        anyhow::bail!("Storage directory does not exist: {}", storage_dir.display());
    }

    // Walk storage directory: storage_dir/machine/project/*.json
    for machine_entry in fs::read_dir(storage_dir)? {
        let machine_entry = machine_entry?;
        let machine_path = machine_entry.path();
        if !machine_path.is_dir() {
            continue;
        }
        let machine = machine_entry.file_name().to_string_lossy().to_string();

        for project_entry in fs::read_dir(&machine_path)? {
            let project_entry = project_entry?;
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let project = project_entry.file_name().to_string_lossy().to_string();

            for file_entry in fs::read_dir(&project_path)? {
                let file_entry = file_entry?;
                let file_path = file_entry.path();

                if file_path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(devlog) = read_devlog(&file_path) {
                        // Check if within date range
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&devlog.timestamp) {
                            if dt < cutoff {
                                continue;
                            }
                        }

                        let key = (machine.clone(), project.clone());
                        let entry = stats.entry(key).or_insert(ProjectStats {
                            machine: machine.clone(),
                            project: project.clone(),
                            session_count: 0,
                            prompt_count: 0,
                            tool_calls: 0,
                            files_touched: 0,
                            prompt_words: 0,
                            response_words: 0,
                            last_activity: String::new(),
                        });

                        entry.session_count += 1;
                        let session_stats = analyze_session(&devlog);
                        entry.prompt_count += session_stats.prompts;
                        entry.tool_calls += session_stats.tool_calls;
                        entry.files_touched += session_stats.files_touched;
                        entry.prompt_words += session_stats.prompt_words;
                        entry.response_words += session_stats.response_words;

                        if devlog.timestamp > entry.last_activity {
                            entry.last_activity = devlog.timestamp.clone();
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<ProjectStats> = stats.into_values().collect();

    // Sort by prompt count descending
    result.sort_by(|a, b| b.prompt_count.cmp(&a.prompt_count));

    Ok(result)
}

pub fn get_project_stats_grouped(storage_dir: &Path, days: u32) -> Result<Vec<ProjectStats>> {
    let by_machine = get_project_stats(storage_dir, days)?;

    // Aggregate by project name only
    let mut grouped: HashMap<String, ProjectStats> = HashMap::new();

    for stat in by_machine {
        let entry = grouped.entry(stat.project.clone()).or_insert(ProjectStats {
            machine: String::new(),
            project: stat.project.clone(),
            session_count: 0,
            prompt_count: 0,
            tool_calls: 0,
            files_touched: 0,
            prompt_words: 0,
            response_words: 0,
            last_activity: String::new(),
        });

        entry.session_count += stat.session_count;
        entry.prompt_count += stat.prompt_count;
        entry.tool_calls += stat.tool_calls;
        entry.files_touched += stat.files_touched;
        entry.prompt_words += stat.prompt_words;
        entry.response_words += stat.response_words;

        if stat.last_activity > entry.last_activity {
            entry.last_activity = stat.last_activity;
        }

        // Track machines
        if entry.machine.is_empty() {
            entry.machine = stat.machine;
        } else if !entry.machine.contains(&stat.machine) {
            entry.machine = format!("{}, {}", entry.machine, stat.machine);
        }
    }

    let mut result: Vec<ProjectStats> = grouped.into_values().collect();
    result.sort_by(|a, b| b.prompt_count.cmp(&a.prompt_count));
    Ok(result)
}

fn read_devlog(path: &Path) -> Result<DevlogOutput> {
    let content = fs::read_to_string(path)?;
    let devlog: DevlogOutput = serde_json::from_str(&content)?;
    Ok(devlog)
}

struct SessionStats {
    prompts: usize,
    tool_calls: usize,
    files_touched: usize,
    prompt_words: usize,
    response_words: usize,
}

fn analyze_session(devlog: &DevlogOutput) -> SessionStats {
    use crate::parser::ConversationEntry;
    use std::collections::HashSet;

    let mut stats = SessionStats {
        prompts: 0,
        tool_calls: 0,
        files_touched: 0,
        prompt_words: 0,
        response_words: 0,
    };

    let mut files: HashSet<String> = HashSet::new();

    for entry in &devlog.conversation {
        match entry {
            ConversationEntry::User { content, .. } => {
                stats.prompts += 1;
                stats.prompt_words += count_words(content);
            }
            ConversationEntry::Assistant { content, .. } => {
                stats.response_words += count_words(content);
            }
            ConversationEntry::ToolSummary { actions } => {
                stats.tool_calls += actions.len();
                // Extract file paths from tool actions
                for action in actions {
                    if let Some(file) = extract_file_from_action(action) {
                        files.insert(file);
                    }
                }
            }
        }
    }

    stats.files_touched = files.len();
    stats
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn extract_file_from_action(action: &str) -> Option<String> {
    // Actions look like: "edited src/main.rs", "read config.json", "created foo.txt"
    let prefixes = ["edited ", "read ", "created "];
    for prefix in prefixes {
        if action.starts_with(prefix) {
            return Some(action[prefix.len()..].to_string());
        }
    }
    None
}

pub fn print_stats(stats: &[ProjectStats], days: u32) {
    if stats.is_empty() {
        println!("No activity in the last {} days", days);
        return;
    }

    println!("Project activity (last {} days):\n", days);
    println!(
        "{:<15} {:<25} {:>8} {:>8}  {}",
        "Machine", "Project", "Sessions", "Prompts", "Last Activity"
    );
    println!("{}", "-".repeat(78));

    for stat in stats {
        let last = if stat.last_activity.is_empty() {
            "unknown".to_string()
        } else {
            chrono::DateTime::parse_from_rfc3339(&stat.last_activity)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|_| stat.last_activity.clone())
        };

        println!(
            "{:<15} {:<25} {:>8} {:>8}  {}",
            truncate(&stat.machine, 15),
            truncate(&stat.project, 25),
            stat.session_count,
            stat.prompt_count,
            last
        );
    }

    let total_sessions: usize = stats.iter().map(|s| s.session_count).sum();
    let total_prompts: usize = stats.iter().map(|s| s.prompt_count).sum();
    println!("{}", "-".repeat(78));
    println!(
        "Total: {} sessions, {} prompts across {} projects",
        total_sessions,
        total_prompts,
        stats.len()
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
