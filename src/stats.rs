use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

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
        cost_of(&self.model_tokens)
    }
}

/// One session row for the per-project drill-down page.
pub struct SessionStats {
    pub machine: String,
    pub session_id: String,
    pub last_activity: String,
    pub prompt_count: usize,
    pub tool_calls: usize,
    pub model_tokens: HashMap<String, ModelTokens>,
}

impl SessionStats {
    pub fn cost_usd(&self) -> f64 {
        cost_of(&self.model_tokens)
    }
    pub fn totals(&self) -> ModelTokens {
        sum_tokens(&self.model_tokens)
    }
}

/// Token totals for one ISO week, for the trend table.
pub struct WeeklyStats {
    pub week: String,
    pub model_tokens: HashMap<String, ModelTokens>,
}

impl WeeklyStats {
    pub fn cost_usd(&self) -> f64 {
        cost_of(&self.model_tokens)
    }
    pub fn totals(&self) -> ModelTokens {
        sum_tokens(&self.model_tokens)
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

pub fn cost_of(model_tokens: &HashMap<String, ModelTokens>) -> f64 {
    model_tokens
        .iter()
        .map(|(model, tokens)| estimate_cost(model, tokens))
        .sum()
}

pub fn sum_tokens(model_tokens: &HashMap<String, ModelTokens>) -> ModelTokens {
    let mut total = ModelTokens::default();
    for tokens in model_tokens.values() {
        total.input_tokens += tokens.input_tokens;
        total.output_tokens += tokens.output_tokens;
        total.cache_read_tokens += tokens.cache_read_tokens;
        total.cache_write_tokens += tokens.cache_write_tokens;
    }
    total
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

fn cutoff_ts(days: u32) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days as i64))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

pub fn get_project_stats(conn: &Connection, days: u32) -> Result<Vec<ProjectStats>> {
    let cutoff = cutoff_ts(days);
    let mut stats: HashMap<(String, String), ProjectStats> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT machine, project, COUNT(*), SUM(prompts), SUM(tool_calls), SUM(files_touched),
                SUM(prompt_words), SUM(response_words), MAX(last_activity)
         FROM sessions WHERE last_activity >= ?1
         GROUP BY machine, project",
    )?;
    let rows = stmt.query_map([&cutoff], |row| {
        Ok(ProjectStats {
            machine: row.get(0)?,
            project: row.get(1)?,
            session_count: row.get::<_, i64>(2)? as usize,
            prompt_count: row.get::<_, i64>(3)? as usize,
            tool_calls: row.get::<_, i64>(4)? as usize,
            files_touched: row.get::<_, i64>(5)? as usize,
            prompt_words: row.get::<_, i64>(6)? as usize,
            response_words: row.get::<_, i64>(7)? as usize,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            model_tokens: HashMap::new(),
            last_activity: row.get(8)?,
        })
    })?;
    for row in rows {
        let stat = row?;
        stats.insert((stat.machine.clone(), stat.project.clone()), stat);
    }

    let mut stmt = conn.prepare(
        "SELECT s.machine, s.project, u.model,
                SUM(u.input_tokens), SUM(u.output_tokens), SUM(u.cache_read_tokens), SUM(u.cache_write_tokens)
         FROM sessions s JOIN message_usage u ON u.session_rowid = s.id
         WHERE s.last_activity >= ?1
         GROUP BY s.machine, s.project, u.model",
    )?;
    let rows = stmt.query_map([&cutoff], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            ModelTokens {
                input_tokens: row.get::<_, i64>(3)? as u64,
                output_tokens: row.get::<_, i64>(4)? as u64,
                cache_read_tokens: row.get::<_, i64>(5)? as u64,
                cache_write_tokens: row.get::<_, i64>(6)? as u64,
            },
        ))
    })?;
    for row in rows {
        let (machine, project, model, tokens) = row?;
        if let Some(stat) = stats.get_mut(&(machine, project)) {
            stat.input_tokens += tokens.input_tokens;
            stat.output_tokens += tokens.output_tokens;
            stat.cache_read_tokens += tokens.cache_read_tokens;
            stat.cache_write_tokens += tokens.cache_write_tokens;
            stat.model_tokens.insert(model, tokens);
        }
    }

    let mut result: Vec<ProjectStats> = stats.into_values().collect();
    result.sort_by(|a, b| b.prompt_count.cmp(&a.prompt_count));
    Ok(result)
}

pub fn get_project_stats_grouped(conn: &Connection, days: u32) -> Result<Vec<ProjectStats>> {
    let by_machine = get_project_stats(conn, days)?;

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

pub fn get_session_stats(conn: &Connection, project: &str, days: u32) -> Result<Vec<SessionStats>> {
    let cutoff = cutoff_ts(days);

    let mut stmt = conn.prepare(
        "SELECT id, machine, session_id, last_activity, prompts, tool_calls
         FROM sessions WHERE project = ?1 AND last_activity >= ?2
         ORDER BY last_activity DESC",
    )?;
    let mut order: Vec<i64> = Vec::new();
    let mut sessions: HashMap<i64, SessionStats> = HashMap::new();
    let rows = stmt.query_map(params![project, cutoff], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            SessionStats {
                machine: row.get(1)?,
                session_id: row.get(2)?,
                last_activity: row.get(3)?,
                prompt_count: row.get::<_, i64>(4)? as usize,
                tool_calls: row.get::<_, i64>(5)? as usize,
                model_tokens: HashMap::new(),
            },
        ))
    })?;
    for row in rows {
        let (id, session) = row?;
        order.push(id);
        sessions.insert(id, session);
    }

    let mut stmt = conn.prepare(
        "SELECT u.session_rowid, u.model,
                SUM(u.input_tokens), SUM(u.output_tokens), SUM(u.cache_read_tokens), SUM(u.cache_write_tokens)
         FROM message_usage u JOIN sessions s ON s.id = u.session_rowid
         WHERE s.project = ?1 AND s.last_activity >= ?2
         GROUP BY u.session_rowid, u.model",
    )?;
    let rows = stmt.query_map(params![project, cutoff], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            ModelTokens {
                input_tokens: row.get::<_, i64>(2)? as u64,
                output_tokens: row.get::<_, i64>(3)? as u64,
                cache_read_tokens: row.get::<_, i64>(4)? as u64,
                cache_write_tokens: row.get::<_, i64>(5)? as u64,
            },
        ))
    })?;
    for row in rows {
        let (id, model, tokens) = row?;
        if let Some(session) = sessions.get_mut(&id) {
            session.model_tokens.insert(model, tokens);
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|id| sessions.remove(&id))
        .collect())
}

/// Token totals per ISO week across all history (newest first), optionally
/// filtered to one project. Buckets by per-message timestamps, so it works
/// retroactively on everything ever ingested.
pub fn get_weekly_stats(conn: &Connection, project: Option<&str>) -> Result<Vec<WeeklyStats>> {
    let sql = format!(
        "SELECT substr(u.ts, 1, 10), u.model,
                SUM(u.input_tokens), SUM(u.output_tokens), SUM(u.cache_read_tokens), SUM(u.cache_write_tokens)
         FROM message_usage u JOIN sessions s ON s.id = u.session_rowid
         {}
         GROUP BY 1, 2",
        if project.is_some() { "WHERE s.project = ?1" } else { "" }
    );
    let mut stmt = conn.prepare(&sql)?;

    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<(String, String, ModelTokens)> {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            ModelTokens {
                input_tokens: row.get::<_, i64>(2)? as u64,
                output_tokens: row.get::<_, i64>(3)? as u64,
                cache_read_tokens: row.get::<_, i64>(4)? as u64,
                cache_write_tokens: row.get::<_, i64>(5)? as u64,
            },
        ))
    };
    let rows: Vec<(String, String, ModelTokens)> = match project {
        Some(p) => stmt.query_map([p], map_row)?.collect::<rusqlite::Result<_>>()?,
        None => stmt.query_map([], map_row)?.collect::<rusqlite::Result<_>>()?,
    };

    let mut weeks: HashMap<String, HashMap<String, ModelTokens>> = HashMap::new();
    for (day, model, tokens) in rows {
        let week = match chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d") {
            Ok(date) => {
                use chrono::Datelike;
                let iso = date.iso_week();
                format!("{}-W{:02}", iso.year(), iso.week())
            }
            Err(_) => "unknown".to_string(),
        };
        let entry = weeks.entry(week).or_default().entry(model).or_default();
        entry.input_tokens += tokens.input_tokens;
        entry.output_tokens += tokens.output_tokens;
        entry.cache_read_tokens += tokens.cache_read_tokens;
        entry.cache_write_tokens += tokens.cache_write_tokens;
    }

    let mut result: Vec<WeeklyStats> = weeks
        .into_iter()
        .map(|(week, model_tokens)| WeeklyStats { week, model_tokens })
        .collect();
    result.sort_by(|a, b| b.week.cmp(&a.week));
    Ok(result)
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
