use serde_json::{json, Value};

#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    BadRequest(String),
    GitHubError(String),
    Internal(String),
    RateLimited(u64),
    Timeout(String),
    ValidationError(String, String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            Self::GitHubError(msg) => write!(f, "GitHub API Error: {}", msg),
            Self::Internal(msg) => write!(f, "Internal Error: {}", msg),
            Self::RateLimited(secs) => write!(f, "Rate Limited: retry after {}s", secs),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::ValidationError(field, reason) => write!(f, "Validation Error ({}): {}", field, reason),
        }
    }
}

impl std::error::Error for AppError {}

impl AppError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::GitHubError(_) => "GITHUB_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::RateLimited(_) => "RATE_LIMITED",
            Self::Timeout(_) => "TIMEOUT",
            Self::ValidationError(_, _) => "VALIDATION_ERROR",
        }
    }

    /// Decompose into (HTTP status, JSON body, error code) for use by ResponseBuilder.
    /// The pipeline and ResponseBuilder own the CORS and request-ID headers.
    pub fn to_error_parts(&self) -> (u16, Value) {
        let error_code = self.error_code();
        let (body, status): (Value, u16) = match self {
            Self::Unauthorized(msg) => {
                let json = if msg.contains("required") {
                    json!({
                        "error": "GitHub Personal Access Token required for non-whitelisted users",
                        "error_code": error_code,
                        "usage": "Add header: Authorization: Bearer YOUR_GITHUB_PAT",
                        "how_to_create": "GitHub Settings → Developer settings → Personal access tokens -> Generate new token (no special permissions needed, just for rate limiting)"
                    })
                } else {
                    json!({ "error": msg, "error_code": error_code })
                };
                (json, 401)
            }
            Self::BadRequest(msg) => {
                let json = if msg.contains("username") {
                    json!({
                        "error": "username parameter is required",
                        "error_code": error_code,
                        "usage": "Add ?username=your_github_username to the URL"
                    })
                } else {
                    json!({ "error": msg, "error_code": error_code })
                };
                (json, 400)
            }
            Self::GitHubError(msg) => {
                (json!({ "error": format!("GraphQL query error: {}", msg), "error_code": error_code }), 500)
            }
            Self::Internal(msg) => {
                (json!({ "error": msg, "error_code": error_code }), 500)
            }
            Self::RateLimited(retry_after) => {
                (json!({
                    "error": "GitHub API rate limit exceeded",
                    "error_code": error_code,
                    "retry_after": retry_after
                }), 429)
            }
            Self::Timeout(reason) => {
                (json!({ "error": format!("Request timed out: {}", reason), "error_code": error_code }), 504)
            }
            Self::ValidationError(field, reason) => {
                (json!({
                    "error": format!("Validation failed for field '{}': {}", field, reason),
                    "error_code": error_code
                }), 400)
            }
        };
        (status, body)
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
