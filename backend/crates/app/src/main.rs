use anyhow::{Context, Result};
use mysuite_app::{auth::password, build_router, config::Config, seed, state::AppState};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .json()
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .context("connexion PostgreSQL impossible")?;

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("migrate") => {
            sqlx::migrate!("../../migrations").run(&pool).await?;
            println!("migrations appliquées");
        }
        Some("create-admin") => {
            // Administrateur plateforme. Mot de passe lu depuis l'environnement (jamais en argument).
            let email = args
                .get(2)
                .context("usage : mysuite create-admin <email> <nom complet>")?
                .to_lowercase();
            let name = args
                .get(3)
                .context("usage : mysuite create-admin <email> <nom complet>")?;
            let pwd = std::env::var("MYSUITE_ADMIN_PASSWORD")
                .context("variable MYSUITE_ADMIN_PASSWORD requise")?;
            password::validate_strength(&pwd).map_err(anyhow::Error::msg)?;
            sqlx::migrate!("../../migrations").run(&pool).await?;
            sqlx::query("INSERT INTO users (email, password_hash, full_name, is_platform_admin) VALUES ($1,$2,$3,TRUE)
                         ON CONFLICT (email) DO UPDATE SET is_platform_admin = TRUE")
                .bind(&email).bind(password::hash(&pwd)?).bind(name).execute(&pool).await?;
            println!("administrateur plateforme prêt : {email}");
        }
        Some("seed-demo") => {
            sqlx::migrate!("../../migrations").run(&pool).await?;
            seed::demo(&pool).await?;
            println!("données de démonstration FICTIVES créées (société « demo »)");
        }
        _ => {
            if config.run_migrations {
                sqlx::migrate!("../../migrations").run(&pool).await?;
            }
            let addr = config.bind_addr.clone();
            let app = build_router(AppState::new(pool, config));
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            tracing::info!(%addr, "MySuite API démarrée");
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown())
                .await?;
        }
    }
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("arrêt demandé");
}
