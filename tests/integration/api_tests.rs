//! Integration tests: real HTTP requests against a mockito server, exercising
//! the `execute_graphql` URL seam the same way `RealHttpFetch` does in
//! production (POST + Bearer token + JSON body), over an actual socket.
//!
//! A minimal HTTP/1.1 client is implemented on `std::net::TcpStream` instead
//! of adding an HTTP-client dev-dependency: `RealHttpFetch` itself uses
//! `worker::Fetch`, which only runs inside the Workers runtime, so it cannot
//! be driven from the native test harness.

use iceberg::error::AppError;
use iceberg::github::types::ReposResponseData;
use iceberg::github::{execute_graphql, HttpFetch, HttpResponse};
use mockito::{Matcher, Server};

/// A `RealHttpFetch`-equivalent adapter: same request shape, but speaking
/// plain HTTP/1.1 over TCP so it runs on the native test host against mockito.
struct RealSocketFetch;

impl HttpFetch for RealSocketFetch {
    async fn post_json(
        &self,
        url: &str,
        token: &str,
        body: &serde_json::Value,
        timeout_secs: u64,
    ) -> Result<HttpResponse, AppError> {
        // Only the mockito URL shape is supported: http://127.0.0.1:<port>/path
        let rest = url.strip_prefix("http://").ok_or_else(|| {
            AppError::Internal(format!("test fetcher only supports http:// URLs, got {}", url))
        })?;
        let (host_port, path) = rest.split_once('/').unwrap_or((rest, ""));
        let path = format!("/{}", path);
        let (host, port) = host_port.split_once(':').ok_or_else(|| {
            AppError::Internal(format!("no port in test URL {}", url))
        })?;
        let port: u16 = port.parse().map_err(|e| AppError::Internal(format!("bad port: {}", e)))?;

        let payload = serde_json::to_vec(body)
            .map_err(|e| AppError::Internal(format!("serialize body: {}", e)))?;

        let timeout = std::time::Duration::from_secs(timeout_secs);
        let addr = std::net::SocketAddr::new(
            host.parse()
                .map_err(|e| AppError::Internal(format!("bad host: {}", e)))?,
            port,
        );
        let mut stream = std::net::TcpStream::connect_timeout(&addr, timeout)
            .map_err(|e| AppError::GitHubError(format!("connect: {}", e)))?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| AppError::Internal(format!("set read timeout: {}", e)))?;

        // Mirror RealHttpFetch's headers exactly; `Connection: close` keeps the
        // response read simple (server closes after the body).
        let request = format!(
            "POST {} HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: iceberg-rust-api/1.0\r\n\
             Authorization: Bearer {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            path, host_port, token, payload.len()
        );
        use std::io::{Read, Write};
        stream
            .write_all(request.as_bytes())
            .and_then(|_| stream.write_all(&payload))
            .map_err(|e| AppError::GitHubError(format!("write request: {}", e)))?;

        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| AppError::GitHubError(format!("read response: {}", e)))?;

        parse_http_response(&raw)
    }
}

/// Parse a full HTTP/1.1 response (head + body read to EOF) into the crate's
/// `HttpResponse`. Handles `Content-Length` and chunked transfer bodies.
fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, AppError> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| AppError::Internal(format!("malformed HTTP response head:\n{}", text)))?;

    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| AppError::Internal("empty HTTP response".to_string()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| AppError::Internal(format!("bad status line: {}", status_line)))?;

    let mut headers = std::collections::HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }

    let body = if headers
        .get("transfer-encoding")
        .map(|v| v.contains("chunked"))
        .unwrap_or(false)
    {
        decode_chunked(body)
    } else {
        // With `Connection: close` the remainder after the head is the body,
        // regardless of whether Content-Length was present.
        body.to_string()
    };

    Ok(HttpResponse {
        status,
        headers,
        body: body.into_bytes(),
    })
}

/// Decode a chunked transfer-encoded body.
fn decode_chunked(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let Some((size_line, after)) = rest.split_once("\r\n") else { break };
        let size = match usize::from_str_radix(size_line.trim(), 16) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        // Each chunk is `size` bytes followed by the CRLF terminator.
        let end = after.char_indices().nth(size).map(|(i, _)| i).unwrap_or(after.len());
        let end = end.min(after.len());
        out.push_str(&after[..end]);
        rest = &after[end..];
        if rest.starts_with("\r\n") {
            rest = &rest[2..];
        }
    }
    out
}

const QUERY: &str = r#"
query($username: String!, $firstRepos: Int!) {
  user(login: $username) {
    repositories(first: $firstRepos) { nodes { nameWithOwner } }
  }
}
"#;

fn success_body() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "user": {
                "repositories": {
                    "nodes": [{ "nameWithOwner": "octocat/iceberg" }]
                }
            }
        },
        "extensions": { "cost": { "actualCost": 42 } }
    })
}

/// Happy path: pointing `execute_graphql` at the mock server sends a
/// well-formed GraphQL request (method, headers, `{query, variables}` body)
/// and the response parses into `ReposResponseData`.
#[tokio::test]
async fn posts_well_formed_graphql_request_and_parses_response() {
    let mut server = Server::new_async().await;

    let variables = serde_json::json!({ "username": "octocat", "firstRepos": 10 });
    let expected_payload = serde_json::json!({ "query": QUERY, "variables": variables });

    let mock = server
        .mock("POST", "/graphql")
        .match_header("User-Agent", "iceberg-rust-api/1.0")
        .match_header("Authorization", "Bearer ghp_test_token")
        .match_header("Content-Type", "application/json")
        .match_body(Matcher::JsonString(
            serde_json::to_string(&expected_payload).unwrap(),
        ))
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(serde_json::to_string(&success_body()).unwrap())
        .expect(1)
        .create_async()
        .await;

    let url = format!("{}/graphql", server.url());
    let (data, cost) = execute_graphql::<_, ReposResponseData>(
        &RealSocketFetch,
        &url,
        "ghp_test_token",
        QUERY,
        variables,
        Some(5),
    )
    .await
    .expect("execute_graphql against mock server");

    // Mock matched every request assertion, exactly once.
    mock.assert();

    // The response really parsed into the domain type.
    let user = data.user.expect("user should deserialize");
    let nodes = user.repositories.nodes.expect("repo nodes should deserialize");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name_with_owner, "octocat/iceberg");

    // Slim extensions (GraphQL cost) parse alongside the data.
    assert_eq!(cost, 42);
}

/// Partial failure: a GraphQL `errors` array alongside partial `data` must
/// surface as `AppError::GitHubError` (first error message), matching
/// `execute_graphql` behaviour — the partial data is never trusted.
#[tokio::test]
async fn graphql_errors_with_partial_data_is_github_error() {
    let mut server = Server::new_async().await;

    let body = concat!(
        r#"{"errors": [{"message": "rate limit hit on field repositories", "type": "FORBIDDEN"}],"#,
        r#" "data": {"user": null}}"#
    );

    let mock = server
        .mock("POST", "/graphql")
        .match_header("Content-Type", "application/json")
        .match_body(Matcher::JsonString(
            serde_json::to_string(&serde_json::json!({
                "query": QUERY,
                "variables": { "username": "octocat", "firstRepos": 10 },
            }))
            .unwrap(),
        ))
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(body)
        .expect(1)
        .create_async()
        .await;

    let url = format!("{}/graphql", server.url());
    let result = execute_graphql::<_, ReposResponseData>(
        &RealSocketFetch,
        &url,
        "ghp_test_token",
        QUERY,
        serde_json::json!({ "username": "octocat", "firstRepos": 10 }),
        Some(5),
    )
    .await;

    // The request really went to the server (well-formed enough to match).
    mock.assert();

    match result {
        Err(AppError::GitHubError(msg)) => {
            assert!(
                msg.contains("rate limit hit on field repositories"),
                "expected the GraphQL error message, got: {}",
                msg
            );
        }
        other => panic!("expected AppError::GitHubError for errors+partial data, got: {:?}", other),
    }
}
