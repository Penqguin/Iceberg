use serde_json::{json, Value};

/// Why an upstream/authorization failure happened. Structured payloads let the
/// response mapping be total — no substring sniffing of message prose.
#[derive(Debug)]
pub enum Unauthorized {
    /// Non-whitelisted user made a request without a Personal Access Token.
    TokenRequired,
    /// A token was presented but empty.
    Invalid(String),
    /// GitHub itself rejected the request (upstream 401/403).
    GitHubApi(String),
}

#[derive(Debug)]
pub enum AppError {
    Unauthorized(Unauthorized),
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
            Self::Unauthorized(reason) => write!(f, "Unauthorized: {}", reason_message(reason)),
            Self::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            Self::GitHubError(msg) => write!(f, "GitHub API Error: {}", msg),
            Self::Internal(msg) => write!(f, "Internal Error: {}", msg),
            Self::RateLimited(secs) => write!(f, "Rate Limited: retry after {}s", secs),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::ValidationError(field, reason) => write!(f, "Validation Error ({}): {}", field, reason),
        }
    }
}

fn reason_message(reason: &Unauthorized) -> String {
    match reason {
        Unauthorized::TokenRequired => "GitHub Personal Access Token required for non-whitelisted users".to_string(),
        Unauthorized::Invalid(msg) => msg.clone(),
        Unauthorized::GitHubApi(msg) => msg.clone(),
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

    /// Extra lifecycle headers an error may carry, beyond the ones
    /// ResponseBuilder sets globally (CORS, X-Request-ID, X-Error-Code).
    pub fn extra_headers(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::RateLimited(retry_after) => vec![("X-Retry-After", retry_after.to_string())],
            _ => Vec::new(),
        }
    }

    /// Decompose into (HTTP status, JSON body) for use by ResponseBuilder.
    /// Total mapping over the variant structure — no message inspection.
    pub fn to_error_parts(&self) -> (u16, Value) {
        let error_code = self.error_code();
        match self {
            Self::Unauthorized(Unauthorized::TokenRequired) => (401, json!({
                "error": "GitHub Personal Access Token required for non-whitelisted users",
                "error_code": error_code,
                "usage": "Add header: Authorization: Bearer YOUR_GITHUB_PAT",
                "how_to_create": "GitHub Settings → Developer settings → Personal access tokens -> Generate new token (no special permissions needed, just for rate limiting)"
            })),
            Self::Unauthorized(reason) => (401, json!({
                "error": reason_message(reason),
                "error_code": error_code
            })),
            Self::BadRequest(msg) => (400, json!({
                "error": msg,
                "error_code": error_code
            })),
            Self::GitHubError(msg) => (500, json!({
                "error": format!("GraphQL query error: {}", msg),
                "error_code": error_code
            })),
            Self::Internal(msg) => (500, json!({
                "error": msg,
                "error_code": error_code
            })),
            Self::RateLimited(retry_after) => (429, json!({
                "error": "GitHub API rate limit exceeded",
                "error_code": error_code,
                "retry_after": retry_after
            })),
            Self::Timeout(reason) => (504, json!({
                "error": format!("Request timed out: {}", reason),
                "error_code": error_code
            })),
            Self::ValidationError(field, reason) => (400, json!({
                "error": format!("Validation failed for field '{}': {}", field, reason),
                "error_code": error_code
            })),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn body(err: &AppError) -> Value {
        let (_, body) = err.to_error_parts();
        body
    }

    fn status(err: &AppError) -> u16 {
        let (status, _) = err.to_error_parts();
        status
    }

    #[test]
    fn token_required_maps_to_help_body() {
        let err = AppError::Unauthorized(Unauthorized::TokenRequired);
        assert_eq!(status(&err), 401);
        let b = body(&err);
        assert_eq!(b["error_code"], "UNAUTHORIZED");
        assert!(b["usage"].as_str().unwrap().contains("Bearer"));
        assert!(b["how_to_create"].as_str().is_some());
    }

    #[test]
    fn github_rejection_maps_to_plain_401() {
        let err = AppError::Unauthorized(Unauthorized::GitHubApi("GitHub API unauthorized: Bad credentials".into()));
        assert_eq!(status(&err), 401);
        assert_eq!(body(&err)["error"], "GitHub API unauthorized: Bad credentials");
        assert!(body(&err).get("usage").is_none());
    }

    #[test]
    fn bad_request_maps_to_400() {
        let err = AppError::BadRequest("GitHub API bad request: oops".into());
        assert_eq!(status(&err), 400);
        assert_eq!(body(&err)["error"], "GitHub API bad request: oops");
    }

    #[test]
    fn github_error_maps_to_500_with_prefix() {
        let err = AppError::GitHubError("Not Found".into());
        assert_eq!(status(&err), 500);
        assert_eq!(body(&err)["error"], "GraphQL query error: Not Found");
    }

    #[test]
    fn internal_maps_to_500() {
        let err = AppError::Internal("boom".into());
        assert_eq!(status(&err), 500);
        assert_eq!(body(&err)["error"], "boom");
    }

    #[test]
    fn rate_limited_maps_to_429_with_retry() {
        let err = AppError::RateLimited(42);
        assert_eq!(status(&err), 429);
        assert_eq!(body(&err)["retry_after"], 42);
        assert_eq!(err.extra_headers(), vec![("X-Retry-After", "42".to_string())]);
    }

    #[test]
    fn timeout_maps_to_504() {
        let err = AppError::Timeout("GitHub API request timed out after 15 seconds".into());
        assert_eq!(status(&err), 504);
        assert!(body(&err)["error"].as_str().unwrap().contains("timed out"));
    }

    #[test]
    fn validation_error_maps_to_400_with_field() {
        let err = AppError::ValidationError("limit".into(), "must be between 1 and 20".into());
        assert_eq!(status(&err), 400);
        assert!(body(&err)["error"].as_str().unwrap().contains("limit"));
    }

    #[test]
    fn error_codes_are_stable() {
        let cases: Vec<(AppError, &str)> = vec![
            (AppError::Unauthorized(Unauthorized::TokenRequired), "UNAUTHORIZED"),
            (AppError::BadRequest("x".into()), "BAD_REQUEST"),
            (AppError::GitHubError("x".into()), "GITHUB_ERROR"),
            (AppError::Internal("x".into()), "INTERNAL_ERROR"),
            (AppError::RateLimited(1), "RATE_LIMITED"),
            (AppError::Timeout("x".into()), "TIMEOUT"),
            (AppError::ValidationError("a".into(), "b".into()), "VALIDATION_ERROR"),
        ];
        for (err, code) in cases {
            assert_eq!(err.error_code(), code);
        }
    }
}
