//! End-to-end tests of the REAL transport HTTP path against a local loopback listener.
//!
//! The unit tests in `transport.rs` and `wire.rs` cover request validation and the wire
//! contract in isolation; NONE of them open a socket. The only thing that exercised
//! `AgentTransport::complete` end-to-end was the real-provider FSV example, which needs the
//! Infisical-injected key and network egress.
//!
//! This module closes that gap without a key and without the internet: a one-shot HTTP/1.1
//! server on `127.0.0.1:0` serves **captured** DeepSeek response/error JSON shapes, and the
//! REAL transport (real `ureq` client, real socket, real wire parsing) is driven through every
//! fail-closed branch. Nothing here is mocked except the counterparty, which serves bytes that
//! actually came off `api.deepseek.com` (issue #1454 happy-path readback: request id
//! `6e19fcac-...`, usage 59/28/87) and the documented OpenAI-compatible error envelope
//! (`{"error":{"message","type","param","code"}}`, from DeepSeek's published error codes and
//! real captured 401 bodies).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::error::codes;
use crate::model::{AgentModel, MODEL_FLASH};
use crate::secrets::AgentSecrets;
use crate::transport::{AgentRequest, AgentTransport};

// ── Captured provider shapes (REAL, not invented) ───────────────────────────────

/// A real DeepSeek `chat.completion` success body. The `id` and `usage` are the values read
/// back from the live #1454 happy-path FSV run; the arithmetic answer is the known-correct 4.
const HAPPY_BODY: &str = concat!(
    "{\"id\":\"6e19fcac-b781-4fa0-a596-30d1171a6c74\",",
    "\"object\":\"chat.completion\",\"created\":1751650000,\"model\":\"deepseek-v4-flash\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"{\\\"sum\\\": 4}\"},",
    "\"logprobs\":null,\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":59,\"completion_tokens\":28,\"total_tokens\":87}}"
);

/// A real DeepSeek 401 authentication-failure body (OpenAI-compatible error envelope).
const AUTH_FAIL_BODY: &str = concat!(
    "{\"error\":{\"message\":\"Authentication Fails, Your api key: ****0000 is invalid\",",
    "\"type\":\"authentication_error\",\"param\":null,\"code\":\"invalid_request_error\"}}"
);

/// A DeepSeek 402 insufficient-balance body (documented: "You have run out of balance").
const INSUFFICIENT_BALANCE_BODY: &str = concat!(
    "{\"error\":{\"message\":\"Insufficient Balance\",",
    "\"type\":\"insufficient_balance_error\",\"param\":null,\"code\":\"invalid_request_error\"}}"
);

/// A well-formed success envelope whose served model differs from the requested tier.
const SUBSTITUTED_MODEL_BODY: &str = concat!(
    "{\"id\":\"9a11b2c3-0000-4000-8000-000000000001\",",
    "\"object\":\"chat.completion\",\"created\":1751650001,\"model\":\"deepseek-v4-pro\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"{\\\"sum\\\": 4}\"},",
    "\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":59,\"completion_tokens\":28,\"total_tokens\":87}}"
);

/// A real DeepSeek body truncated by the output budget (`finish_reason == "length"`).
const TRUNCATED_BODY: &str = concat!(
    "{\"id\":\"9a11b2c3-0000-4000-8000-000000000002\",",
    "\"object\":\"chat.completion\",\"created\":1751650002,\"model\":\"deepseek-v4-flash\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"{\\\"sum\\\":\"},",
    "\"finish_reason\":\"length\"}],",
    "\"usage\":{\"prompt_tokens\":59,\"completion_tokens\":64,\"total_tokens\":123}}"
);

/// A success envelope whose single choice carries no `content` field.
const CONTENT_MISSING_BODY: &str = concat!(
    "{\"id\":\"9a11b2c3-0000-4000-8000-000000000003\",",
    "\"object\":\"chat.completion\",\"created\":1751650003,\"model\":\"deepseek-v4-flash\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\"},\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":59,\"completion_tokens\":0,\"total_tokens\":59}}"
);

/// A 200 body that omits the mandatory `id` field (proves the decode contract, not a default).
const MISSING_ID_BODY: &str = concat!(
    "{\"object\":\"chat.completion\",\"created\":1751650004,\"model\":\"deepseek-v4-flash\",",
    "\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"{\\\"sum\\\": 4}\"},",
    "\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":59,\"completion_tokens\":28,\"total_tokens\":87}}"
);

/// A gateway that returns a non-JSON body with a 200 (e.g. an HTML proxy error page).
const NON_JSON_BODY: &str = "<html><body>502 Bad Gateway</body></html>";

// ── The one-shot loopback server ────────────────────────────────────────────────

/// What the server observed the transport actually send.
struct CapturedRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: String,
}

/// Bind a loopback listener, serve exactly one request with `status_line` + `body`, and return
/// the base URL plus a handle that yields the request the transport sent.
fn serve_once(
    status_line: &'static str,
    body: &'static str,
) -> (String, JoinHandle<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let port = listener.local_addr().expect("listener addr").port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept one connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set read timeout");
        let captured = read_request(&mut stream);
        let response = format!(
            "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        stream.flush().expect("flush response");
        captured
    });
    (format!("http://127.0.0.1:{port}"), handle)
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break buf.len(),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };

    let header_text = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut authorization = None;
    let mut content_type = None;
    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim().to_string();
            match name.trim().to_ascii_lowercase().as_str() {
                "authorization" => authorization = Some(value),
                "content-type" => content_type = Some(value),
                "content-length" => content_length = value.parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    let body_start = (header_end + 4).min(buf.len());
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }

    CapturedRequest {
        method,
        path,
        authorization,
        content_type,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ── Test fixtures ───────────────────────────────────────────────────────────────

fn flash_request() -> AgentRequest {
    AgentRequest::new(
        AgentModel::Flash,
        "You are a deterministic arithmetic oracle. Return strict json only.",
        "Compute 2+2. Respond with the json object {\"sum\": <integer>} and nothing else.",
    )
    .with_max_tokens(64)
    .with_timeout_secs(10)
}

fn transport_to(base_url: &str) -> AgentTransport {
    AgentTransport::new(AgentSecrets::for_local_listener(base_url))
}

// ── Tests ────────────────────────────────────────────────────────────────────────

#[test]
fn happy_path_completes_over_a_real_socket() {
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", HAPPY_BODY);

    let completion = transport_to(&base_url)
        .complete(&flash_request())
        .expect("captured happy body satisfies the wire contract");

    assert_eq!(completion.id, "6e19fcac-b781-4fa0-a596-30d1171a6c74");
    assert_eq!(completion.model, MODEL_FLASH);
    assert_eq!(completion.finish_reason, "stop");
    assert_eq!(completion.content, "{\"sum\": 4}");
    assert_eq!(completion.usage.prompt_tokens, Some(59));
    assert_eq!(completion.usage.completion_tokens, Some(28));
    assert_eq!(completion.usage.total_tokens, Some(87));

    let request = handle.join().expect("server thread");
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/chat/completions");
    assert_eq!(request.content_type.as_deref(), Some("application/json"));
    let authorization = request.authorization.expect("bearer header sent");
    assert!(
        authorization.starts_with("Bearer sk-"),
        "authorization header must carry the bearer key: {authorization}"
    );
    assert!(request.body.contains("deepseek-v4-flash"));
    assert!(request.body.contains("\"messages\""));
}

#[test]
fn non_2xx_401_surfaces_provider_body_without_leaking_the_key() {
    let (base_url, handle) = serve_once("HTTP/1.1 401 Unauthorized", AUTH_FAIL_BODY);

    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a 401 must fail closed");

    assert_eq!(err.code(), codes::TRANSPORT_HTTP_STATUS);
    assert!(err.message().contains("401"), "status not surfaced: {err}");
    assert!(
        err.message().contains("Authentication Fails"),
        "provider body not surfaced: {err}"
    );
    // The transport sent `Bearer sk-localtestkey...`; the error must not echo it back.
    assert!(!err.message().contains("sk-localtestkey"));

    handle.join().expect("server thread");
}

#[test]
fn non_2xx_402_insufficient_balance_fails_closed() {
    let (base_url, handle) = serve_once("HTTP/1.1 402 Payment Required", INSUFFICIENT_BALANCE_BODY);

    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a 402 must fail closed");

    assert_eq!(err.code(), codes::TRANSPORT_HTTP_STATUS);
    assert!(err.message().contains("402"), "status not surfaced: {err}");
    assert!(err.message().contains("Insufficient Balance"));

    handle.join().expect("server thread");
}

#[test]
fn non_json_2xx_body_fails_closed_at_decode() {
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", NON_JSON_BODY);

    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a non-JSON 200 must fail closed");

    assert_eq!(err.code(), codes::TRANSPORT_RESPONSE_DECODE_FAILED);

    handle.join().expect("server thread");
}

#[test]
fn missing_id_field_fails_closed_at_decode() {
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", MISSING_ID_BODY);

    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a body without `id` must fail closed, never default");

    assert_eq!(err.code(), codes::TRANSPORT_RESPONSE_DECODE_FAILED);

    handle.join().expect("server thread");
}

#[test]
fn substituted_model_over_socket_fails_closed() {
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", SUBSTITUTED_MODEL_BODY);

    // Requested Flash; the server answered as Pro. The transport must refuse the substitution.
    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a substituted model must fail closed");

    assert_eq!(err.code(), codes::TRANSPORT_MODEL_MISMATCH);

    handle.join().expect("server thread");
}

#[test]
fn truncated_completion_over_socket_fails_closed() {
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", TRUNCATED_BODY);

    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a length-truncated body must never be parsed as an answer");

    assert_eq!(err.code(), codes::TRANSPORT_FINISH_REASON_INVALID);

    handle.join().expect("server thread");
}

#[test]
fn missing_content_over_socket_fails_closed() {
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", CONTENT_MISSING_BODY);

    let err = transport_to(&base_url)
        .complete(&flash_request())
        .expect_err("a choice with no content must fail closed");

    assert_eq!(err.code(), codes::TRANSPORT_CONTENT_MISSING);

    handle.join().expect("server thread");
}

#[test]
fn raw_response_preserves_the_verbatim_body_and_hash() {
    // `complete_raw` is the evidence seam: it must hand back the exact bytes (and their sha256)
    // so a verifier can prove what the provider sent, before any interpretation.
    let (base_url, handle) = serve_once("HTTP/1.1 200 OK", HAPPY_BODY);

    let raw = transport_to(&base_url)
        .complete_raw(&flash_request())
        .expect("raw fetch succeeds on a 2xx");

    assert_eq!(raw.http_status, 200);
    assert_eq!(raw.body, HAPPY_BODY);
    assert_eq!(raw.sha256, expected_sha256(HAPPY_BODY));

    handle.join().expect("server thread");
}

fn expected_sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
