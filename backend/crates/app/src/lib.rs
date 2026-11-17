//! MySuite — API RH multi-société (monolithe modulaire).

pub mod analytics;
pub mod attendance;
pub mod audit;
pub mod auth;
pub mod config;
pub mod contract_docs;
pub mod contracts;
pub mod documents;
pub mod employees;
pub mod error;
pub mod leave;
pub mod mail;
pub mod organization;
pub mod payroll;
pub mod payroll_templates;
pub mod pdf;
pub mod reports;
pub mod seed;
pub mod state;
pub mod tenants;
pub mod timeclock;
pub mod wagegrid;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderName, HeaderValue, Method, header},
    middleware,
    routing::get,
};
use serde_json::{Value, json};
use state::AppState;
use tower_http::{
    cors::CorsLayer, limit::RequestBodyLimitLayer, set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn ready(State(s): State<AppState>) -> Result<Json<Value>, error::AppError> {
    sqlx::query("SELECT 1").execute(&s.db).await?;
    Ok(Json(json!({"status": "ready"})))
}

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .merge(auth::http::routes())
        .merge(auth::mfa::routes())
        .merge(auth::reset::routes())
        .merge(tenants::routes())
        .merge(organization::routes())
        .merge(employees::routes())
        .merge(contracts::routes())
        .merge(contract_docs::routes())
        .merge(documents::routes())
        .merge(leave::routes())
        .merge(payroll::routes())
        .merge(payroll_templates::routes())
        .merge(wagegrid::routes())
        .merge(reports::routes())
        .merge(analytics::routes())
        .merge(attendance::routes())
        .merge(timeclock::routes())
        .layer(middleware::from_fn(auth::csrf_guard));

    let mut app = Router::new()
        .route("/health", get(live))
        .route("/health/ready", get(ready))
        .nest("/api/v1", api)
        .nest("/api/v1/device", timeclock::device_routes())
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(TraceLayer::new_for_http());

    if let Some(origin) = state
        .config
        .cors_origin
        .as_deref()
        .and_then(|o| HeaderValue::from_str(o).ok())
    {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_credentials(true)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([
                    header::CONTENT_TYPE,
                    HeaderName::from_static("x-requested-with"),
                ]),
        );
    }
    app.with_state(state)
}
