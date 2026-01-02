mod db;
mod models;

use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use log::info;
use std::sync::Mutex;

struct AppState {
    db: Mutex<duckdb::Connection>,
}

async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

async fn ingest_session(
    data: web::Json<models::DevlogSession>,
    app_state: web::Data<AppState>,
) -> impl Responder {
    let session = data.into_inner();

    info!(
        "Received session {} from machine {} (project: {})",
        session.session_id, session.machine_id, session.project_dir
    );

    let db = app_state.db.lock().unwrap();

    match db::insert_session(&db, &session) {
        Ok(_) => {
            info!("Session {} stored successfully", session.session_id);
            HttpResponse::Ok().json(serde_json::json!({
                "status": "success",
                "session_id": session.session_id
            }))
        }
        Err(e) => {
            eprintln!("Failed to store session: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "status": "error",
                "error": format!("{}", e)
            }))
        }
    }
}

#[actix_web::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("Initializing DuckDB database...");
    let db_path = std::env::var("DEVLOG_DB_PATH").unwrap_or_else(|_| "devlog.duckdb".to_string());
    let conn = db::init_database(&db_path)?;

    info!("Database initialized at: {}", db_path);

    let app_state = web::Data::new(AppState {
        db: Mutex::new(conn),
    });

    let bind_addr = std::env::var("DEVLOG_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    info!("Starting HTTP server on {}", bind_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/health", web::get().to(health_check))
            .route("/ingest", web::post().to(ingest_session))
    })
    .bind(&bind_addr)?
    .run()
    .await?;

    Ok(())
}
