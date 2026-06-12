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
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub model_tokens: HashMap<String, ModelTokens>,
    pub last_activity: String,
}

impl ProjectStats {
    pub fn cost_usd(&self) -> f64 {
        self.model_tokens
            .iter()
            .map(|(model, tokens)| estimate_cost(model, tokens))
            .sum()
    }
}

#[derive(Default, Clone)]
pub struct ModelTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// API list price in $ per million tokens: (input, output).
/// Cache reads bill at 0.1x input, cache writes at 1.25x input (5-minute TTL).
fn model_pricing(model: &str) -> (f64, f64) {
    if model.contains("fable") || model.contains("mythos") {
        (10.0, 50.0)
    } else if model.contains("opus-4-1") || model.contains("opus-4-0") || model.contains("opus-4-2025") {
        (15.0, 75.0)
    } else if model.contains("sonnet") {
        (3.0, 15.0)
    } else if model.contains("haiku") {
        (1.0, 5.0)
    } else {
        // opus, plus unknown models (sessions ingested before model
        // tracking) — Opus was the default model, so price them as Opus
        (5.0, 25.0)
    }
}

pub fn estimate_cost(model: &str, tokens: &ModelTokens) -> f64 {
    let (input_rate, output_rate) = model_pricing(model);
    (tokens.input_tokens as f64 * input_rate
        + tokens.output_tokens as f64 * output_rate
        + tokens.cache_read_tokens as f64 * input_rate * 0.1
        + tokens.cache_write_tokens as f64 * input_rate * 1.25)
        / 1_000_000.0
}

fn merge_model_tokens(dest: &mut HashMap<String, ModelTokens>, src: &HashMap<String, ModelTokens>) {
    for (model, tokens) in src {
        let entry = dest.entry(model.clone()).or_default();
        entry.input_tokens += tokens.input_tokens;
        entry.output_tokens += tokens.output_tokens;
        entry.cache_read_tokens += tokens.cache_read_tokens;
        entry.cache_write_tokens += tokens.cache_write_tokens;
    }
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
                            input_tokens: 0,
                            output_tokens: 0,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                            model_tokens: HashMap::new(),
                            last_activity: String::new(),
                        });

                        entry.session_count += 1;
                        let session_stats = analyze_session(&devlog);
                        entry.prompt_count += session_stats.prompts;
                        entry.tool_calls += session_stats.tool_calls;
                        entry.files_touched += session_stats.files_touched;
                        entry.prompt_words += session_stats.prompt_words;
                        entry.response_words += session_stats.response_words;
                        entry.input_tokens += session_stats.input_tokens;
                        entry.output_tokens += session_stats.output_tokens;
                        entry.cache_read_tokens += session_stats.cache_read_tokens;
                        entry.cache_write_tokens += session_stats.cache_write_tokens;
                        merge_model_tokens(&mut entry.model_tokens, &session_stats.model_tokens);

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
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            model_tokens: HashMap::new(),
            last_activity: String::new(),
        });

        entry.session_count += stat.session_count;
        entry.prompt_count += stat.prompt_count;
        entry.tool_calls += stat.tool_calls;
        entry.files_touched += stat.files_touched;
        entry.prompt_words += stat.prompt_words;
        entry.response_words += stat.response_words;
        entry.input_tokens += stat.input_tokens;
        entry.output_tokens += stat.output_tokens;
        entry.cache_read_tokens += stat.cache_read_tokens;
        entry.cache_write_tokens += stat.cache_write_tokens;
        merge_model_tokens(&mut entry.model_tokens, &stat.model_tokens);

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
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    model_tokens: HashMap<String, ModelTokens>,
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
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        model_tokens: HashMap::new(),
    };

    let mut files: HashSet<String> = HashSet::new();

    for entry in &devlog.conversation {
        match entry {
            ConversationEntry::User { content, .. } => {
                stats.prompts += 1;
                stats.prompt_words += count_words(content);
            }
            ConversationEntry::Assistant { content, usage, model, .. } => {
                stats.response_words += count_words(content);
                if let Some(ref usage) = usage {
                    stats.input_tokens += usage.input_tokens.unwrap_or(0);
                    stats.output_tokens += usage.output_tokens.unwrap_or(0);
                    stats.cache_read_tokens += usage.cache_read_input_tokens.unwrap_or(0);
                    stats.cache_write_tokens += usage.cache_creation_input_tokens.unwrap_or(0);

                    let model_name = model.as_deref().unwrap_or("unknown").to_string();
                    let per_model = stats.model_tokens.entry(model_name).or_default();
                    per_model.input_tokens += usage.input_tokens.unwrap_or(0);
                    per_model.output_tokens += usage.output_tokens.unwrap_or(0);
                    per_model.cache_read_tokens += usage.cache_read_input_tokens.unwrap_or(0);
                    per_model.cache_write_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_uses_per_model_rates() {
        let tokens = ModelTokens {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
            cache_write_tokens: 1_000_000,
        };
        // Fable 5: $10 in + $50 out + $1 cache read (0.1x) + $12.50 cache write (1.25x)
        assert_eq!(estimate_cost("claude-fable-5", &tokens), 73.5);
        // Opus 4.x: $5 in + $25 out + $0.50 + $6.25
        assert_eq!(estimate_cost("claude-opus-4-8", &tokens), 36.75);
        // Unknown models (pre-model-tracking sessions) price as Opus
        assert_eq!(estimate_cost("unknown", &tokens), 36.75);
    }
}
