use serde_json::json;
use worker::*;

#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    BadRequest(String),
    GitHubError(String),
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            Self::GitHubError(msg) => write!(f, "GitHub API Error: {}", msg),
            Self::Internal(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl AppError {
    pub fn to_response(&self) -> Result<Response> {
        match self {
            Self::Unauthorized(msg) => {
                let json_body = if msg.contains("required") {
                    json!({
                        "error": "GitHub Personal Access Token required for non-whitelisted users",
                        "usage": "Add header: Authorization: Bearer YOUR_GITHUB_PAT",
                        "how_to_create": "GitHub Settings → Developer settings → Personal access tokens → Generate new token (no special permissions needed, just for rate limiting)"
                    })
                } else {
                    json!({
                        "error": msg
                    })
                };
                Ok(Response::from_json(&json_body)?.with_status(401))
            }
            Self::BadRequest(msg) => {
                let json_body = if msg.contains("username") {
                    json!({
                        "error": "username parameter is required",
                        "usage": "Add ?username=your_github_username to the URL"
                    })
                } else {
                    json!({
                        "error": msg
                    })
                };
                Ok(Response::from_json(&json_body)?.with_status(400))
            }
            Self::GitHubError(msg) => {
                Ok(Response::from_json(&json!({ "error": format!("GraphQL query error: {}", msg) }))?.with_status(500))
            }
            Self::Internal(msg) => {
                Ok(Response::from_json(&json!({ "error": msg }))?.with_status(500))
            }
        }
    }
}

impl From<worker::Error> for AppError {
    fn from(err: worker::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<AppError> for worker::Error {
    fn from(err: AppError) -> Self {
        worker::Error::from(err.to_string())
    }
}
