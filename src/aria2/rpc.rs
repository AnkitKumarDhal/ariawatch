use serde::{Deserialize, Serialize};

// Represents an aria2 JSON-RPC error
#[derive(Debug, Deserialize)]
pub struct Aria2RpcError {
    pub code: i64,
    pub message: String,
}

// Represents an outgoing JSON-RPC request
#[derive(Debug, Serialize)]
pub struct Aria2RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

// Reprsents a JSON-RPC response envelope
#[derive(Debug, Deserialize)]
pub struct Aria2RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<Aria2RpcError>,
}

impl Aria2RpcResponse {
    // Validates the RPC envelope and extracts it result or error
    pub fn into_result(self, expected_id: &str) -> Result<serde_json::Value, String> {
        if self.jsonrpc != "2.0" {
            return Err(format!("Unexpected JSON-RPC version: {}", self.jsonrpc));
        }

        if self.id != expected_id {
            return Err(format!(
                "Response ID mismatch: expected {expected_id} got {}",
                self.id
            ));
        }

        match (self.result, self.error) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) => {
                Err(format!("aria2 rpc error {}: {}", error.code, error.message))
            }
            (Some(_), Some(_)) => Err(String::from(
                "Invalid RPC response: both result and error fields populated",
            )),
            (None, None) => Err(String::from(
                "Invalid RPC response: neither result nor error fields populated",
            )),
        }
    }
}

pub fn parse_rpc_response(json: &str) -> Result<Aria2RpcResponse, String> {
    serde_json::from_str(json).map_err(|error| format!("Invalid JSON RPC response: {error}"))
}

pub fn build_rpc_request(
    method: &str,
    mut params: Vec<serde_json::Value>,
    rpc_secret: Option<&str>,
) -> Result<String, String> {
    if let Some(secret) = rpc_secret {
        params.insert(0, serde_json::Value::String(format!("token:{secret}")));
    }

    let request = Aria2RpcRequest {
        jsonrpc: String::from("2.0"),
        id: String::from("ariawatch-1"),
        method: method.to_owned(),
        params,
    };

    serde_json::to_string(&request)
        .map_err(|error| format!("Could not serialize rpc request: {error}"))
}

pub fn build_get_version_request(rpc_secret: Option<&str>) -> Result<String, String> {
    build_rpc_request("aria2.getVersion", Vec::new(), rpc_secret)
}

// re-usable rpc call method
pub fn call_rpc_method(
    endpoint: &str,
    method: &str,
    params: Vec<serde_json::Value>,
    rpc_secret: Option<&str>,
) -> Result<serde_json::Value, String> {
    let request = build_rpc_request(method, params, rpc_secret)?;
    let body = send_rpc_request(endpoint, &request)?;
    let response = parse_rpc_response(&body)?;
    response.into_result("ariawatch-1")
}

// formats and returns the http_error details in a more presentable way
fn format_http_error_detail(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(error) = value.get("error")
        && let (Some(code), Some(message)) = (
            error.get("code").and_then(serde_json::Value::as_i64),
            error.get("message").and_then(serde_json::Value::as_str),
        )
    {
        return format!(" (JSON-RPC error {code}: {message})");
    }

    let body = body.trim();

    if body.is_empty() {
        String::new()
    } else {
        let excerpt: String = body.chars().take(512).collect();
        format!("; response body: {excerpt}")
    }
}

pub fn send_rpc_request(endpoint: &str, request_json: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|error| format!("Could not create HTTP Client: {error}"))?;

    let response = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_json.to_owned())
        .send()
        .map_err(|error| format!("Could not reach aria2 rpc endpoint: {error}"))?;

    let status = response.status();

    let body = response
        .text()
        .map_err(|error| format!("Could not read aria2 response: {error}"))?;

    if !status.is_success() {
        let detail = format_http_error_detail(&body);
        return Err(format!("aria2 returned HTTP status {status}{detail}"));
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::format_http_error_detail;

    #[test]
    fn includes_json_rpc_error_details_in_http_diagnostics() {
        let body = r#"{
            "jsonrpc": "2.0",
            "id": "ariawatch-1",
            "error": {
                "code": 1,
                "message": "Invalid request"
            }
        }"#;

        let detail = format_http_error_detail(body);

        assert!(detail.contains("JSON-RPC error 1"));
        assert!(detail.contains("Invalid request"));
    }
}
