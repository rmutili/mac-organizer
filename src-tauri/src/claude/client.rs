//! Thin HTTPS client for the Messages API.

use std::time::Duration;

use serde_json::Value;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("Your Anthropic API key was rejected. Check it in Settings.")]
    Unauthorized,
    #[error("Your API key doesn't have access to this model.")]
    Forbidden,
    #[error("Rate limited by the Claude API. Wait a minute and try again.")]
    RateLimited,
    #[error("The Claude API is overloaded right now. Try again shortly.")]
    Overloaded,
    #[error("Claude API error ({status}): {message}")]
    Other { status: u16, message: String },
    #[error("Couldn't reach the Claude API: {0}")]
    Network(String),
}

/// Map a non-success HTTP status and error body to a user-facing error.
pub fn error_for_status(status: u16, body: &Value) -> ApiError {
    match status {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden,
        429 => ApiError::RateLimited,
        500..=599 => ApiError::Overloaded,
        _ => ApiError::Other {
            status,
            message: body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string(),
        },
    }
}

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Couldn't start the HTTP client: {e}"))
}

pub async fn send_message(
    http: &reqwest::Client,
    api_key: &str,
    body: &Value,
) -> Result<Value, ApiError> {
    let response = http
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(body)
        .send()
        .await
        .map_err(|e| ApiError::Network(e.without_url().to_string()))?;
    let status = response.status();
    let json: Value = response
        .json()
        .await
        .map_err(|e| ApiError::Network(e.without_url().to_string()))?;
    if status.is_success() {
        Ok(json)
    } else {
        Err(error_for_status(status.as_u16(), &json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_common_statuses_to_friendly_errors() {
        assert_eq!(error_for_status(401, &json!({})), ApiError::Unauthorized);
        assert_eq!(error_for_status(429, &json!({})), ApiError::RateLimited);
        assert_eq!(error_for_status(529, &json!({})), ApiError::Overloaded);
    }

    #[test]
    fn passes_through_api_error_message() {
        let body = json!({ "error": { "type": "invalid_request_error", "message": "bad model" } });
        assert_eq!(
            error_for_status(400, &body),
            ApiError::Other {
                status: 400,
                message: "bad model".into()
            }
        );
    }
}
