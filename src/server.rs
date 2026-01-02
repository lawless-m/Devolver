use crate::output::DevlogOutput;
use crate::stats;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

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

pub async fn run_server(config: ServerConfig) -> anyhow::Result<()> {
    // Ensure storage directory exists
    fs::create_dir_all(&config.storage_dir)?;

    let state = Arc::new(config.clone());

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/stats", get(stats_page))
        .route("/ingest", post(ingest))
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

async fn index() -> Html<&'static str> {
    Html(r#"<!DOCTYPE html>
<html>
<head><title>Devlog Receiver</title></head>
<body>
<h1>Devlog Receiver</h1>
<ul>
<li><a href="stats">Project Stats</a></li>
<li><a href="health">Health Check</a></li>
</ul>
</body>
</html>"#)
}

#[derive(serde::Deserialize)]
struct StatsQuery {
    days: Option<u32>,
}

async fn stats_page(
    State(config): State<Arc<ServerConfig>>,
    Query(query): Query<StatsQuery>,
) -> impl IntoResponse {
    let days = query.days.unwrap_or(7);

    let grouped = stats::get_project_stats_grouped(&config.storage_dir, days);
    let by_machine = stats::get_project_stats(&config.storage_dir, days);

    match (grouped, by_machine) {
        (Ok(grouped_stats), Ok(machine_stats)) => {
            let html = render_stats_html(&grouped_stats, &machine_stats, days);
            (StatusCode::OK, Html(html))
        }
        (Err(e), _) | (_, Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Error</h1><p>{}</p>", e)),
        ),
    }
}

fn render_stats_html(
    grouped: &[stats::ProjectStats],
    by_machine: &[stats::ProjectStats],
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
table {{ border-collapse: collapse; width: 100%; max-width: 1000px; }}
th, td {{ padding: 0.5rem 1rem; text-align: left; border-bottom: 1px solid #333; }}
th {{ background: #16213e; color: #00d9ff; }}
tr:hover {{ background: #16213e; }}
tr.parent {{ cursor: pointer; }}
tr.parent td:first-child::before {{ content: "▶ "; font-size: 0.8em; }}
tr.parent.expanded td:first-child::before {{ content: "▼ "; }}
tr.child {{ display: none; background: #0d1117; }}
tr.child.visible {{ display: table-row; }}
tr.child td:first-child {{ padding-left: 2rem; color: #888; }}
.number {{ text-align: right; font-variant-numeric: tabular-nums; }}
a {{ color: #00d9ff; }}
.filter {{ margin-bottom: 1rem; display: flex; gap: 0.5rem; }}
.filter a {{ padding: 0.3rem 0.8rem; background: #16213e; text-decoration: none; border-radius: 4px; }}
.filter a:hover, .filter a.active {{ background: #00d9ff; color: #1a1a2e; }}
.total {{ margin-top: 1rem; color: #888; }}
</style>
</head>
<body>
<h1>Project Activity</h1>
<div class="filter">
  <a href="stats?days=1" {}>Today</a>
  <a href="stats?days=7" {}>7 days</a>
  <a href="stats?days=30" {}>30 days</a>
  <a href="stats?days=90" {}>90 days</a>
</div>
"#,
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
<tr><th>Project</th><th class="number">Prompts</th><th class="number">Tools</th><th class="number">Files</th><th class="number">Words In</th><th class="number">Words Out</th><th>Last Activity</th></tr>
"#,
        );

        for (idx, stat) in grouped.iter().enumerate() {
            let last = chrono::DateTime::parse_from_rfc3339(&stat.last_activity)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|_| stat.last_activity.clone());

            // Parent row (grouped)
            html.push_str(&format!(
                "<tr class=\"parent\" data-idx=\"{}\"><td>{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td>{}</td></tr>\n",
                idx,
                html_escape(&stat.project),
                stat.prompt_count,
                stat.tool_calls,
                stat.files_touched,
                format_number(stat.prompt_words),
                format_number(stat.response_words),
                last
            ));

            // Child rows (by machine for this project)
            for machine_stat in by_machine.iter().filter(|s| s.project == stat.project) {
                let m_last = chrono::DateTime::parse_from_rfc3339(&machine_stat.last_activity)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|_| machine_stat.last_activity.clone());

                html.push_str(&format!(
                    "<tr class=\"child\" data-parent=\"{}\"><td>{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td>{}</td></tr>\n",
                    idx,
                    html_escape(&machine_stat.machine),
                    machine_stat.prompt_count,
                    machine_stat.tool_calls,
                    machine_stat.files_touched,
                    format_number(machine_stat.prompt_words),
                    format_number(machine_stat.response_words),
                    m_last
                ));
            }
        }

        html.push_str("</table>");

        let total_prompts: usize = grouped.iter().map(|s| s.prompt_count).sum();
        let total_tools: usize = grouped.iter().map(|s| s.tool_calls).sum();
        let total_words_in: usize = grouped.iter().map(|s| s.prompt_words).sum();
        let total_words_out: usize = grouped.iter().map(|s| s.response_words).sum();
        html.push_str(&format!(
            "<p class=\"total\">{} prompts, {} tool calls, {}k words in, {}k words out</p>",
            total_prompts,
            total_tools,
            total_words_in / 1000,
            total_words_out / 1000,
        ));
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

fn format_number(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

async fn ingest(
    State(config): State<Arc<ServerConfig>>,
    Json(payload): Json<DevlogOutput>,
) -> impl IntoResponse {
    match store_devlog(&config.storage_dir, &payload) {
        Ok(path) => {
            eprintln!("Stored devlog: {}", path.display());
            (StatusCode::OK, format!("Stored: {}", path.display()))
        }
        Err(e) => {
            eprintln!("Failed to store devlog: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))
        }
    }
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
