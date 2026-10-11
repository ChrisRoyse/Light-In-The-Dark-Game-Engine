use std::{
    io::{Read, Write},
    net::TcpStream,
    process::Command,
    str,
    time::Duration,
};

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    ai::LocalNpcDialogueInput,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateResponse {
    pub model: String,
    pub response: String,
    pub done: bool,
    pub done_reason: Option<String>,
    pub total_duration: Option<u64>,
    pub load_duration: Option<u64>,
    pub prompt_eval_count: Option<u64>,
    pub eval_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelMeta {
    pub present: bool,
    pub digest: String,
    pub size: u64,
    pub family: String,
    pub parameter_size: String,
    pub quantization_level: String,
}

pub fn content(
    kind: EngineContentKind,
    id: &str,
    display: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue970/{id}"),
            author: "issue970-fsv".to_string(),
            source_hash: sha256_hex(format!("issue970:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue970_grounding_anchor".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

pub fn ollama_generate(
    model: &str,
    prompt: &str,
    format: Option<&str>,
    timeout_seconds: u64,
) -> Result<OllamaGenerateResponse, Box<dyn std::error::Error>> {
    let mut body = json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "keep_alive": -1,
        "options": { "temperature": 0, "top_k": 1, "num_predict": 120, "num_ctx": 2048 }
    });
    if let Some(format) = format {
        body["format"] = json!(format);
    }
    let bytes = ollama_request("POST", "/api/generate", Some(&body), timeout_seconds)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn ollama_get_json(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&ollama_request(
        "GET", path, None, 30,
    )?)?)
}

pub fn model_meta(tags: &Value, model: &str) -> Result<ModelMeta, Box<dyn std::error::Error>> {
    let row = tags["models"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["model"] == model || row["name"] == model)
        })
        .ok_or("Ollama model not present after pull")?;
    Ok(ModelMeta {
        present: true,
        digest: row["digest"].as_str().unwrap_or_default().to_string(),
        size: row["size"].as_u64().unwrap_or_default(),
        family: row["details"]["family"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        parameter_size: row["details"]["parameter_size"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        quantization_level: row["details"]["quantization_level"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn running_model_present(ps: &Value, model: &str) -> bool {
    ps["models"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .any(|row| row["model"] == model || row["name"] == model)
        })
        .unwrap_or(false)
}

pub fn input_hash(input: &LocalNpcDialogueInput) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&serde_json::to_vec(input)?))
}

pub fn nanos_to_ms(value: u64) -> u64 {
    value.div_ceil(1_000_000)
}

pub fn nvidia_smi_readback() -> Result<Value, Box<dyn std::error::Error>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output()?;
    let stdout = str::from_utf8(&output.stdout)?.trim().to_string();
    let stderr = str::from_utf8(&output.stderr)?.trim().to_string();
    if !output.status.success() {
        return Err(format!("nvidia-smi failed: {stderr}").into());
    }
    let line = stdout
        .lines()
        .next()
        .ok_or("nvidia-smi returned no GPU rows")?;
    let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
    Ok(json!({
        "command": "nvidia-smi --query-gpu=name,memory.total,memory.used,memory.free --format=csv,noheader,nounits",
        "stdout": stdout,
        "gpu_name": parts.first().copied().unwrap_or_default(),
        "memory_total_mib": parts.get(1).copied().unwrap_or_default(),
        "memory_used_mib": parts.get(2).copied().unwrap_or_default(),
        "memory_free_mib": parts.get(3).copied().unwrap_or_default(),
        "matches_issue_hardware": parts.first().copied().unwrap_or_default().contains("5090")
    }))
}

fn ollama_request(
    method: &str,
    path: &str,
    body: Option<&Value>,
    timeout_seconds: u64,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let body_bytes = body
        .map(serde_json::to_vec)
        .transpose()?
        .unwrap_or_default();
    let mut stream = TcpStream::connect("127.0.0.1:11434")?;
    stream.set_read_timeout(Some(Duration::from_secs(timeout_seconds)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(&body_bytes)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    parse_http_response(&response)
}

fn parse_http_response(response: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("HTTP response missing header terminator")?;
    let head = str::from_utf8(&response[..split])?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or("HTTP response missing status")?
        .parse::<u16>()?;
    let body = response[split + 4..].to_vec();
    if !(200..300).contains(&status) {
        return Err(format!("Ollama HTTP {status}: {}", String::from_utf8_lossy(&body)).into());
    }
    if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return decode_chunked(&body);
    }
    Ok(body)
}

fn decode_chunked(body: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    loop {
        let line_end = body[cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("chunk size missing CRLF")?
            + cursor;
        let line = str::from_utf8(&body[cursor..line_end])?;
        let size = usize::from_str_radix(line.split(';').next().unwrap_or(line).trim(), 16)?;
        cursor = line_end + 2;
        if size == 0 {
            break;
        }
        out.extend_from_slice(&body[cursor..cursor + size]);
        cursor += size + 2;
    }
    Ok(out)
}
