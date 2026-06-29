use crate::index;
use crate::output::DevlogOutput;
use crate::search::{self, SearchScope};
use crate::stats;
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ServerConfig {
    pub storage_dir: PathBuf,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from("/store/devolver"),
            port: 8090,
        }
    }
}

pub struct AppState {
    pub config: ServerConfig,
    pub db: Mutex<rusqlite::Connection>,
}

pub async fn run_server(config: ServerConfig) -> anyhow::Result<()> {
    // Ensure storage directory exists
    fs::create_dir_all(&config.storage_dir)?;

    let mut db = index::open(&config.storage_dir)?;
    let indexed = index::backfill(&mut db, &config.storage_dir)?;
    if indexed > 0 {
        eprintln!("Indexed {} devlog files into stats index", indexed);
    }

    let state = Arc::new(AppState {
        config: config.clone(),
        db: Mutex::new(db),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/stats", get(stats_page))
        .route("/search", get(search_page))
        .route("/ingest", post(ingest))
        // Long sessions exceed axum's 2MB default body limit
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    eprintln!("Devlog receiver listening on {}", addr);
    eprintln!("Storage directory: {}", config.storage_dir.display());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "devlog-receiver OK"
}

const NAV_STYLE: &str = r#"
.nav { display: flex; gap: 1rem; margin-bottom: 1.5rem; padding-bottom: 1rem; border-bottom: 1px solid #333; }
.nav a { color: #888; text-decoration: none; padding: 0.3rem 0.8rem; border-radius: 4px; }
.nav a:hover { background: #16213e; color: #00d9ff; }
.nav a.active { background: #00d9ff; color: #1a1a2e; }
"#;

fn nav_html(active: &str) -> String {
    format!(
        r#"<nav class="nav">
  <a href="search" {}>Search</a>
  <a href="stats" {}>Stats</a>
</nav>"#,
        if active == "search" { "class=\"active\"" } else { "" },
        if active == "stats" { "class=\"active\"" } else { "" },
    )
}

async fn index() -> Html<String> {
    Html(format!(r#"<!DOCTYPE html>
<html>
<head>
<title>Devlog</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; background: #1a1a2e; color: #eee; }}
h1 {{ color: #00d9ff; }}
a {{ color: #00d9ff; }}
{}
</style>
</head>
<body>
{}
<h1>Devlog</h1>
<p>Development conversation archive from Claude Code sessions.</p>
</body>
</html>"#, NAV_STYLE, nav_html("")))
}

#[derive(serde::Deserialize)]
struct StatsQuery {
    days: Option<u32>,
    project: Option<String>,
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: Option<String>,
    scope: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    days: Option<u32>,
}

fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => s.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

use serde::Deserialize;

async fn stats_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StatsQuery>,
) -> impl IntoResponse {
    let days = query.days.unwrap_or(7);
    let db = state.db.lock().expect("stats db lock poisoned");

    if let Some(project) = query.project.as_deref() {
        let sessions = stats::get_session_stats(&db, project, days);
        let weekly = stats::get_weekly_stats(&db, Some(project));
        return match (sessions, weekly) {
            (Ok(sessions), Ok(weekly)) => {
                let html = render_project_html(project, &sessions, &weekly, days);
                (StatusCode::OK, Html(html))
            }
            (Err(e), _) | (_, Err(e)) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("<h1>Error</h1><p>{}</p>", e)),
            ),
        };
    }

    let grouped = stats::get_project_stats_grouped(&db, days);
    let by_machine = stats::get_project_stats(&db, days);
    let weekly = stats::get_weekly_stats(&db, None);

    match (grouped, by_machine, weekly) {
        (Ok(grouped_stats), Ok(machine_stats), Ok(weekly_stats)) => {
            let html = render_stats_html(&grouped_stats, &machine_stats, &weekly_stats, days);
            (StatusCode::OK, Html(html))
        }
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Error</h1><p>{}</p>", e)),
        ),
    }
}

fn render_stats_html(
    grouped: &[stats::ProjectStats],
    by_machine: &[stats::ProjectStats],
    weekly: &[stats::WeeklyStats],
    days: u32,
) -> String {
    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<title>Devlog Stats</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; background: #1a1a2e; color: #eee; }}
h1 {{ color: #00d9ff; }}
table {{ border-collapse: collapse; width: 100%; max-width: 1400px; }}
th, td {{ padding: 0.5rem 1rem; text-align: left; border-bottom: 1px solid #333; white-space: nowrap; }}
th {{ background: #16213e; color: #00d9ff; }}
tr:hover {{ background: #16213e; }}
tr.parent {{ cursor: pointer; }}
tr.parent td:first-child::before {{ content: "▶ "; font-size: 0.8em; }}
tr.parent.expanded td:first-child::before {{ content: "▼ "; }}
tr.child {{ display: none; background: #0d1117; }}
tr.child.visible {{ display: table-row; }}
tr.child td:first-child {{ padding-left: 2rem; color: #888; }}
tr.totals td {{ font-weight: bold; background: #16213e; border-top: 2px solid #00d9ff; }}
.number {{ text-align: right; font-variant-numeric: tabular-nums; }}
a {{ color: #00d9ff; }}
.filter {{ margin-bottom: 1rem; display: flex; gap: 0.5rem; }}
.filter a {{ padding: 0.3rem 0.8rem; background: #16213e; text-decoration: none; border-radius: 4px; }}
.filter a:hover, .filter a.active {{ background: #00d9ff; color: #1a1a2e; }}
.total {{ margin-top: 1rem; color: #888; }}
{}
</style>
</head>
<body>
{}
<h1>Project Activity</h1>
<div class="filter">
  <a href="stats?days=1" {}>Today</a>
  <a href="stats?days=7" {}>7 days</a>
  <a href="stats?days=30" {}>30 days</a>
  <a href="stats?days=90" {}>90 days</a>
</div>
"#,
        NAV_STYLE,
        nav_html("stats"),
        if days == 1 { "class=\"active\"" } else { "" },
        if days == 7 { "class=\"active\"" } else { "" },
        if days == 30 { "class=\"active\"" } else { "" },
        if days == 90 { "class=\"active\"" } else { "" },
    );

    if grouped.is_empty() {
        html.push_str(&format!("<p>No activity in the last {} days</p>", days));
    } else {
        html.push_str(
            r#"<table>
<tr><th>Project</th><th class="number">Sessions</th><th class="number">Prompts</th><th class="number">Tools</th><th class="number">Files</th><th class="number">Words In</th><th class="number">Words Out</th><th class="number">Tokens In</th><th class="number">Tokens Out</th><th class="number">Cache R/W</th><th class="number">Cache R/Prompt</th><th class="number">Est. Cost</th><th>Last Activity</th></tr>
"#,
        );

        for (idx, stat) in grouped.iter().enumerate() {
            let last = chrono::DateTime::parse_from_rfc3339(&stat.last_activity)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|_| stat.last_activity.clone());

            // Parent row (grouped)
            html.push_str(&format!(
                "<tr class=\"parent\" data-idx=\"{}\"><td><a href=\"stats?project={}&days={}\">{}</a></td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td>{}</td></tr>\n",
                idx,
                urlencoding(&stat.project),
                days,
                html_escape(&stat.project),
                stat.session_count,
                stat.prompt_count,
                stat.tool_calls,
                stat.files_touched,
                format_number(stat.prompt_words),
                format_number(stat.response_words),
                format_tokens(stat.input_tokens),
                format_tokens(stat.output_tokens),
                format_cache_tokens(stat.cache_read_tokens, stat.cache_write_tokens),
                format_per_prompt(stat.cache_read_tokens, stat.prompt_count),
                format_cost(stat.cost_usd()),
                last
            ));

            // Child rows (by machine for this project)
            for machine_stat in by_machine.iter().filter(|s| s.project == stat.project) {
                let m_last = chrono::DateTime::parse_from_rfc3339(&machine_stat.last_activity)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|_| machine_stat.last_activity.clone());

                html.push_str(&format!(
                    "<tr class=\"child\" data-parent=\"{}\"><td>{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td>{}</td></tr>\n",
                    idx,
                    html_escape(&machine_stat.machine),
                    machine_stat.session_count,
                    machine_stat.prompt_count,
                    machine_stat.tool_calls,
                    machine_stat.files_touched,
                    format_number(machine_stat.prompt_words),
                    format_number(machine_stat.response_words),
                    format_tokens(machine_stat.input_tokens),
                    format_tokens(machine_stat.output_tokens),
                    format_cache_tokens(machine_stat.cache_read_tokens, machine_stat.cache_write_tokens),
                    format_per_prompt(machine_stat.cache_read_tokens, machine_stat.prompt_count),
                    format_cost(machine_stat.cost_usd()),
                    m_last
                ));
            }
        }

        let total_sessions: usize = grouped.iter().map(|s| s.session_count).sum();
        let total_prompts: usize = grouped.iter().map(|s| s.prompt_count).sum();
        let total_tools: usize = grouped.iter().map(|s| s.tool_calls).sum();
        let total_files: usize = grouped.iter().map(|s| s.files_touched).sum();
        let total_words_in: usize = grouped.iter().map(|s| s.prompt_words).sum();
        let total_words_out: usize = grouped.iter().map(|s| s.response_words).sum();
        let total_input_tokens: u64 = grouped.iter().map(|s| s.input_tokens).sum();
        let total_output_tokens: u64 = grouped.iter().map(|s| s.output_tokens).sum();
        let total_cache_read: u64 = grouped.iter().map(|s| s.cache_read_tokens).sum();
        let total_cache_write: u64 = grouped.iter().map(|s| s.cache_write_tokens).sum();
        let total_cost: f64 = grouped.iter().map(|s| s.cost_usd()).sum();

        html.push_str(&format!(
            "<tr class=\"totals\"><td>Total</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td></td></tr>\n",
            total_sessions,
            total_prompts,
            total_tools,
            total_files,
            format_number(total_words_in),
            format_number(total_words_out),
            format_tokens(total_input_tokens),
            format_tokens(total_output_tokens),
            format_cache_tokens(total_cache_read, total_cache_write),
            format_per_prompt(total_cache_read, total_prompts),
            format_cost(total_cost),
        ));
        html.push_str("</table>");

        html.push_str(&format!(
            "<p class=\"total\">{} prompts, {} tool calls, {}k words in, {}k words out | {} tokens in, {} tokens out, {} cache read, {} cache write | est. API cost {}</p>",
            total_prompts,
            total_tools,
            total_words_in / 1000,
            total_words_out / 1000,
            format_tokens(total_input_tokens),
            format_tokens(total_output_tokens),
            format_tokens(total_cache_read),
            format_tokens(total_cache_write),
            format_cost(total_cost),
        ));

        html.push_str(&render_model_breakdown(grouped));
        html.push_str(&render_weekly_table(weekly));
    }

    html.push_str(r#"
<script>
document.querySelectorAll('tr.parent').forEach(row => {
  row.onclick = () => {
    row.classList.toggle('expanded');
    const idx = row.dataset.idx;
    document.querySelectorAll(`tr.child[data-parent="${idx}"]`).forEach(child => {
      child.classList.toggle('visible');
    });
  };
});
</script>
</body></html>"#);
    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Minimal percent-encoding for a query-string value (project names are
/// directory names, but spaces/&/# would still break the URL).
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

fn format_per_prompt(tokens: u64, prompts: usize) -> String {
    if prompts == 0 {
        "-".to_string()
    } else {
        format_tokens(tokens / prompts as u64)
    }
}

/// Per-project drill-down: one row per session, plus the project's weekly trend.
fn render_project_html(
    project: &str,
    sessions: &[stats::SessionStats],
    weekly: &[stats::WeeklyStats],
    days: u32,
) -> String {
    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<title>Devlog Stats — {project}</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; background: #1a1a2e; color: #eee; }}
h1, h2 {{ color: #00d9ff; }}
table {{ border-collapse: collapse; width: 100%; max-width: 1400px; }}
th, td {{ padding: 0.5rem 1rem; text-align: left; border-bottom: 1px solid #333; white-space: nowrap; }}
th {{ background: #16213e; color: #00d9ff; }}
tr:hover {{ background: #16213e; }}
tr.totals td {{ font-weight: bold; background: #16213e; border-top: 2px solid #00d9ff; }}
.number {{ text-align: right; font-variant-numeric: tabular-nums; }}
a {{ color: #00d9ff; }}
.filter {{ margin-bottom: 1rem; display: flex; gap: 0.5rem; }}
.filter a {{ padding: 0.3rem 0.8rem; background: #16213e; text-decoration: none; border-radius: 4px; }}
.filter a:hover, .filter a.active {{ background: #00d9ff; color: #1a1a2e; }}
.session-id {{ color: #888; font-size: 0.85em; }}
{nav_style}
</style>
</head>
<body>
{nav}
<h1>Project: {project}</h1>
<p><a href="stats?days={days}">&larr; All projects</a></p>
<div class="filter">
  <a href="stats?project={proj_url}&days=1" {d1}>Today</a>
  <a href="stats?project={proj_url}&days=7" {d7}>7 days</a>
  <a href="stats?project={proj_url}&days=30" {d30}>30 days</a>
  <a href="stats?project={proj_url}&days=90" {d90}>90 days</a>
  <a href="stats?project={proj_url}&days=3650" {dall}>All time</a>
</div>
"#,
        project = html_escape(project),
        proj_url = urlencoding(project),
        nav_style = NAV_STYLE,
        nav = nav_html("stats"),
        days = days,
        d1 = if days == 1 { "class=\"active\"" } else { "" },
        d7 = if days == 7 { "class=\"active\"" } else { "" },
        d30 = if days == 30 { "class=\"active\"" } else { "" },
        d90 = if days == 90 { "class=\"active\"" } else { "" },
        dall = if days == 3650 { "class=\"active\"" } else { "" },
    );

    if sessions.is_empty() {
        html.push_str(&format!(
            "<p>No sessions in the last {} days</p>",
            days
        ));
    } else {
        html.push_str(
            r#"<table>
<tr><th>Last Activity</th><th>Machine</th><th>Session</th><th class="number">Prompts</th><th class="number">Tools</th><th class="number">Tokens In</th><th class="number">Tokens Out</th><th class="number">Cache R/W</th><th class="number">Cache R/Prompt</th><th class="number">Est. Cost</th></tr>
"#,
        );

        let mut total = stats::ModelTokens::default();
        let mut total_prompts = 0usize;
        let mut total_tools = 0usize;
        let mut total_cost = 0.0f64;

        for session in sessions {
            let last = chrono::DateTime::parse_from_rfc3339(&session.last_activity)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|_| session.last_activity.clone());
            let tokens = session.totals();
            let cost = session.cost_usd();
            let short_id: String = session.session_id.chars().take(8).collect();

            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"session-id\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td></tr>\n",
                last,
                html_escape(&session.machine),
                html_escape(&short_id),
                session.prompt_count,
                session.tool_calls,
                format_tokens(tokens.input_tokens),
                format_tokens(tokens.output_tokens),
                format_cache_tokens(tokens.cache_read_tokens, tokens.cache_write_tokens),
                format_per_prompt(tokens.cache_read_tokens, session.prompt_count),
                format_cost(cost),
            ));

            total.input_tokens += tokens.input_tokens;
            total.output_tokens += tokens.output_tokens;
            total.cache_read_tokens += tokens.cache_read_tokens;
            total.cache_write_tokens += tokens.cache_write_tokens;
            total_prompts += session.prompt_count;
            total_tools += session.tool_calls;
            total_cost += cost;
        }

        html.push_str(&format!(
            "<tr class=\"totals\"><td>Total</td><td></td><td>{} sessions</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td></tr>\n",
            sessions.len(),
            total_prompts,
            total_tools,
            format_tokens(total.input_tokens),
            format_tokens(total.output_tokens),
            format_cache_tokens(total.cache_read_tokens, total.cache_write_tokens),
            format_per_prompt(total.cache_read_tokens, total_prompts),
            format_cost(total_cost),
        ));
        html.push_str("</table>");
    }

    html.push_str(&render_weekly_table(weekly));
    html.push_str("</body></html>");
    html
}

/// Weekly trend across all history (not affected by the days filter).
fn render_weekly_table(weekly: &[stats::WeeklyStats]) -> String {
    if weekly.is_empty() {
        return String::new();
    }

    let mut html = String::from(
        r#"<h2>By Week (all history)</h2>
<table>
<tr><th>Week</th><th class="number">Tokens In</th><th class="number">Tokens Out</th><th class="number">Cache R/W</th><th class="number">Est. Cost</th></tr>
"#,
    );
    for week in weekly {
        let tokens = week.totals();
        html.push_str(&format!(
            "<tr><td>{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td></tr>\n",
            html_escape(&week.week),
            format_tokens(tokens.input_tokens),
            format_tokens(tokens.output_tokens),
            format_cache_tokens(tokens.cache_read_tokens, tokens.cache_write_tokens),
            format_cost(week.cost_usd()),
        ));
    }
    html.push_str("</table>");
    html
}

fn format_number(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn format_tokens(n: u64) -> String {
    if n == 0 {
        "-".to_string()
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn render_model_breakdown(grouped: &[stats::ProjectStats]) -> String {
    use std::collections::HashMap;

    let mut by_model: HashMap<String, stats::ModelTokens> = HashMap::new();
    for stat in grouped {
        for (model, tokens) in &stat.model_tokens {
            let entry = by_model.entry(model.clone()).or_default();
            entry.input_tokens += tokens.input_tokens;
            entry.output_tokens += tokens.output_tokens;
            entry.cache_read_tokens += tokens.cache_read_tokens;
            entry.cache_write_tokens += tokens.cache_write_tokens;
        }
    }

    if by_model.is_empty() {
        return String::new();
    }

    let mut rows: Vec<(String, stats::ModelTokens, f64)> = by_model
        .into_iter()
        .map(|(model, tokens)| {
            let cost = stats::estimate_cost(&model, &tokens);
            (model, tokens, cost)
        })
        .collect();
    rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut html = String::from(
        r#"<h2>By Model</h2>
<table>
<tr><th>Model</th><th class="number">Tokens In</th><th class="number">Tokens Out</th><th class="number">Cache R/W</th><th class="number">Est. Cost</th></tr>
"#,
    );
    for (model, tokens, cost) in rows {
        html.push_str(&format!(
            "<tr><td>{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td></tr>\n",
            html_escape(&model),
            format_tokens(tokens.input_tokens),
            format_tokens(tokens.output_tokens),
            format_cache_tokens(tokens.cache_read_tokens, tokens.cache_write_tokens),
            format_cost(cost),
        ));
    }
    html.push_str("</table>");
    html
}

fn format_cost(cost: f64) -> String {
    if cost == 0.0 {
        "-".to_string()
    } else if cost < 0.01 {
        "<$0.01".to_string()
    } else {
        format!("${:.2}", cost)
    }
}

fn format_cache_tokens(read: u64, write: u64) -> String {
    if read == 0 && write == 0 {
        "-".to_string()
    } else {
        format!("{}/{}", format_tokens(read), format_tokens(write))
    }
}

async fn search_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let scope = query
        .scope
        .as_deref()
        .map(SearchScope::from_str)
        .unwrap_or_default();

    let results = query.q.as_ref().map(|q| {
        if q.trim().is_empty() {
            Ok(Vec::new())
        } else {
            search::search_devlogs(&state.config.storage_dir, q, scope, query.days, 50)
        }
    });

    let html = render_search_html(
        query.q.as_deref().unwrap_or(""),
        query.scope.as_deref().unwrap_or("conversations"),
        query.days,
        results,
    );

    (StatusCode::OK, Html(html))
}

fn render_search_html(
    query: &str,
    scope: &str,
    days: Option<u32>,
    results: Option<anyhow::Result<Vec<search::SearchResult>>>,
) -> String {
    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
<title>Search Devlogs</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; background: #1a1a2e; color: #eee; }}
h1 {{ color: #00d9ff; }}
.search-form {{ margin-bottom: 1.5rem; }}
.search-form input[type="text"] {{
    width: 400px; padding: 0.5rem; font-size: 1rem;
    background: #16213e; border: 1px solid #333; color: #eee; border-radius: 4px;
}}
.search-form button {{
    padding: 0.5rem 1rem; font-size: 1rem; cursor: pointer;
    background: #00d9ff; color: #1a1a2e; border: none; border-radius: 4px;
}}
.search-form button:hover {{ background: #00b8d4; }}
.filters {{ margin-bottom: 1rem; display: flex; gap: 1.5rem; align-items: center; }}
.filters label {{ color: #888; cursor: pointer; }}
.filters input[type="radio"] {{ margin-right: 0.3rem; }}
.filters select {{
    background: #16213e; color: #eee; border: 1px solid #333;
    padding: 0.3rem; border-radius: 4px;
}}
.result {{
    margin-bottom: 1.5rem; padding: 1rem;
    background: #16213e; border-radius: 8px; border-left: 3px solid #00d9ff;
}}
.result-header {{
    display: flex; justify-content: space-between; align-items: center;
    margin-bottom: 0.5rem; font-size: 0.9rem; color: #888;
}}
.result-header .project {{ color: #00d9ff; font-weight: bold; }}
.result-header .type {{
    padding: 0.15rem 0.5rem; border-radius: 3px; font-size: 0.8rem;
}}
.type-user {{ background: #2d5a2d; color: #8f8; }}
.type-assistant {{ background: #5a2d5a; color: #f8f; }}
.type-tool {{ background: #5a5a2d; color: #ff8; }}
.snippet {{ line-height: 1.5; white-space: pre-wrap; word-break: break-word; }}
.snippet mark {{ background: #ff0; color: #000; padding: 0 2px; }}
a {{ color: #00d9ff; }}
.no-results {{ color: #888; font-style: italic; }}
{}
</style>
</head>
<body>
{}
<h1>Search</h1>
<form class="search-form" method="get">
  <input type="text" name="q" placeholder="Search conversations..." value="{}" autofocus>
  <button type="submit">Search</button>
  <div class="filters">
    <span>Scope:</span>
    <label><input type="radio" name="scope" value="prompts" {}> Prompts only</label>
    <label><input type="radio" name="scope" value="conversations" {}> Conversations</label>
    <label><input type="radio" name="scope" value="all" {}> Everything</label>
    <span style="margin-left: 1rem;">Days:</span>
    <select name="days">
      <option value="" {}>All time</option>
      <option value="1" {}>Today</option>
      <option value="7" {}>7 days</option>
      <option value="30" {}>30 days</option>
      <option value="90" {}>90 days</option>
    </select>
  </div>
</form>
"#,
        NAV_STYLE,
        nav_html("search"),
        html_escape(query),
        if scope == "prompts" { "checked" } else { "" },
        if scope == "conversations" { "checked" } else { "" },
        if scope == "all" { "checked" } else { "" },
        if days.is_none() { "selected" } else { "" },
        if days == Some(1) { "selected" } else { "" },
        if days == Some(7) { "selected" } else { "" },
        if days == Some(30) { "selected" } else { "" },
        if days == Some(90) { "selected" } else { "" },
    );

    match results {
        None => {
            // No search performed yet
        }
        Some(Ok(results)) if results.is_empty() => {
            html.push_str("<p class=\"no-results\">No results found.</p>");
        }
        Some(Ok(results)) => {
            html.push_str(&format!("<p>{} results</p>", results.len()));
            for result in results {
                let timestamp = chrono::DateTime::parse_from_rfc3339(&result.timestamp)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|_| result.timestamp.clone());

                let type_class = match result.entry_type.as_str() {
                    "user" => "type-user",
                    "assistant" => "type-assistant",
                    _ => "type-tool",
                };

                let highlighted_snippet = highlight_match(&result.snippet, &result.query);

                html.push_str(&format!(
                    r#"<div class="result">
  <div class="result-header">
    <span><span class="project">{}</span> · {} · {}</span>
    <span class="type {}">{}</span>
  </div>
  <div class="snippet">{}</div>
</div>
"#,
                    html_escape(&result.project),
                    html_escape(&result.machine),
                    timestamp,
                    type_class,
                    result.entry_type,
                    highlighted_snippet,
                ));
            }
        }
        Some(Err(e)) => {
            html.push_str(&format!("<p class=\"no-results\">Error: {}</p>", e));
        }
    }

    html.push_str("</body></html>");
    html
}

fn highlight_match(snippet: &str, query: &str) -> String {
    let escaped = html_escape(snippet);
    let query_lower = query.to_lowercase();
    let escaped_lower = escaped.to_lowercase();

    // Find match position in escaped string (char-safe)
    if let Some(byte_pos) = escaped_lower.find(&query_lower) {
        // Convert byte position to char position
        let char_pos = escaped_lower[..byte_pos].chars().count();
        let query_char_len = query_lower.chars().count();

        let before: String = escaped.chars().take(char_pos).collect();
        let matched: String = escaped.chars().skip(char_pos).take(query_char_len).collect();
        let after: String = escaped.chars().skip(char_pos + query_char_len).collect();
        format!("{}<mark>{}</mark>{}", before, matched, after)
    } else {
        escaped
    }
}

async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DevlogOutput>,
) -> impl IntoResponse {
    match store_devlog(&state.config.storage_dir, &payload) {
        Ok(path) => {
            eprintln!("Stored devlog: {}", path.display());
            if let Err(e) = index_stored_devlog(&state, &path, &payload) {
                // The raw file is stored; the index will pick it up via
                // backfill on the next restart, so don't fail the ingest.
                eprintln!("Failed to index devlog (will backfill on restart): {}", e);
            }
            (StatusCode::OK, format!("Stored: {}", path.display()))
        }
        Err(e) => {
            eprintln!("Failed to store devlog: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))
        }
    }
}

fn index_stored_devlog(
    state: &AppState,
    stored_path: &std::path::Path,
    payload: &DevlogOutput,
) -> anyhow::Result<()> {
    let machine = payload.machine_id.clone();
    let project = extract_project_name(&payload.project_dir);
    let db = state.db.lock().expect("stats db lock poisoned");
    index::upsert_devlog(&db, &machine, &project, payload)?;
    if let Ok(rel) = stored_path.strip_prefix(&state.config.storage_dir) {
        index::mark_file_indexed(&db, &rel.to_string_lossy().replace('\\', "/"))?;
    }
    Ok(())
}

fn store_devlog(storage_dir: &PathBuf, output: &DevlogOutput) -> anyhow::Result<PathBuf> {
    // Organize by machine_id/project
    let machine_dir = storage_dir.join(&output.machine_id);

    // Extract project name from project_dir (last component)
    // Handle both Windows and Unix paths
    let project_name = extract_project_name(&output.project_dir);

    let project_dir = machine_dir.join(&project_name);
    fs::create_dir_all(&project_dir)?;

    // Generate filename: YYYY-MM-DD-HHMMSS-<session_id_short>.json
    let filename = generate_filename(&output.session_id, &output.timestamp);
    let output_path = project_dir.join(&filename);

    // Serialize and write
    let json = serde_json::to_string_pretty(output)?;
    fs::write(&output_path, json)?;

    Ok(output_path)
}

/// Extract project name from a path, handling both Windows and Unix separators
fn extract_project_name(path: &str) -> String {
    // Split by both Windows and Unix separators, take the last non-empty component
    path.split(|c| c == '/' || c == '\\')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("unknown")
        .to_string()
}

fn generate_filename(session_id: &str, timestamp: &str) -> String {
    // Try to parse the timestamp for the date part
    let date_part = chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|dt| dt.format("%Y-%m-%d-%H%M%S").to_string())
        .unwrap_or_else(|_| chrono::Utc::now().format("%Y-%m-%d-%H%M%S").to_string());

    // Shorten session_id for filename
    let short_id: String = session_id.chars().take(8).collect();

    format!("{}-{}.json", date_part, short_id)
}
