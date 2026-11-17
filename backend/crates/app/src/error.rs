use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentification requise")]
    Unauthorized,
    #[error("accès refusé")]
    Forbidden,
    #[error(
        "l'authentification à deux facteurs est obligatoire dans cette société : activez-la dans « Sécurité » pour continuer"
    )]
    MfaEnrollmentRequired,
    #[error("ressource introuvable")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("trop de tentatives, réessayez plus tard")]
    TooManyAttempts,
    #[error("erreur interne")]
    Internal(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            // 23505 unique_violation, 23503 foreign_key_violation, 23514 check_violation
            match db.code().as_deref() {
                Some("23505") => return AppError::Conflict("cette valeur existe déjà".into()),
                Some("23503") => return AppError::Validation("référence invalide".into()),
                Some("23514") => {
                    return AppError::Validation("valeur refusée par les contraintes".into());
                }
                Some("P0001") => return AppError::Conflict(db.message().to_string()),
                _ => {}
            }
        }
        AppError::Internal(anyhow::Error::new(e))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let correlation_id = Uuid::new_v4();
        let (status, code) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::MfaEnrollmentRequired => (StatusCode::FORBIDDEN, "mfa_enrollment_required"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::TooManyAttempts => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            AppError::Internal(e) => {
                tracing::error!(%correlation_id, error = ?e, "erreur interne");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        let message = match &self {
            AppError::Internal(_) => "Une erreur interne est survenue".to_string(),
            other => other.to_string(),
        };
        (
            status,
            Json(json!({ "code": code, "message": message, "correlation_id": correlation_id })),
        )
            .into_response()
    }
}

/// Aide de validation : retourne `Err(Validation)` si la condition est fausse.
pub fn ensure(cond: bool, msg: &str) -> AppResult<()> {
    if cond {
        Ok(())
    } else {
        Err(AppError::Validation(msg.to_string()))
    }
}
