//! Hermetic fault-injection stub for the calyx-mcp sidecar transport tests (#1792).
//!
//! This is NOT the real addon. It speaks just enough newline-delimited JSON-RPC over
//! stdin/stdout to exercise the transport's failure and framing paths with no ML crates
//! and no network -- so the concurrency tests run under a plain
//! `cargo test -p litd-editor-mcp --features calyx-sidecar`.
//!
//! Mode is selected by argv[1]:
//!   echo          -- reply to every request with a valid result carrying the same id.
//!   mismatched-id -- reply with a deliberately wrong id (id + 1000) to trip the
//!                    1:1 correlation guard.
//!   sleep-forever -- read one request, then never reply (exercises the recv timeout).
//!   stderr-flood  -- flood >64 KiB to stderr, THEN reply, proving the stderr drain
//!                    thread prevents a pipe-backpressure deadlock.
//!   exit-mid      -- read one request, then exit without replying (EOF on child death).

use std::io::{BufRead, Write};
use std::process::exit;

use serde_json::{Value, json};

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "echo".to_string());
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break, // stdin closed: clean shutdown.
        };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);

        match mode.as_str() {
            "sleep-forever" => {
                // Read nothing more; park forever so the transport hits its recv timeout.
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                }
            }
            "exit-mid" => {
                // Die mid-request without replying: the reader thread sees EOF and the
                // transport must surface a typed "closed" error, not hang.
                exit(0);
            }
            "stderr-flood" => {
                // 70 KiB dwarfs a typical 64 KiB pipe buffer. If the transport did not
                // drain stderr on its own thread, this write (or the child's) would block
                // and deadlock; instead the response below must still arrive.
                let blob = vec![b'x'; 70 * 1024];
                let _ = stderr.write_all(&blob);
                let _ = stderr.write_all(b"\n");
                let _ = stderr.flush();
                reply(&mut stdout, &id, json!({"ok": true, "flooded": true}));
            }
            "mismatched-id" => {
                let wrong = match id.as_i64() {
                    Some(n) => json!(n + 1000),
                    None => json!(999_999),
                };
                reply(&mut stdout, &wrong, json!({"ok": true}));
            }
            // "echo" and any unknown mode reply correctly.
            _ => {
                let method = request
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let result = if method == "tools/list" {
                    json!({"tools": [
                        {"name": "calyx.stub.ping", "description": "stub", "inputSchema": {"type": "object"}}
                    ]})
                } else {
                    json!({"ok": true, "method": method})
                };
                reply(&mut stdout, &id, result);
            }
        }
    }
}

fn reply(stdout: &mut std::io::Stdout, id: &Value, result: Value) {
    let frame = json!({"jsonrpc": "2.0", "id": id, "result": result});
    let mut line = frame.to_string();
    line.push('\n');
    // stdout is block-buffered when piped, so an explicit flush is mandatory: without it
    // the parent would block on recv while the reply sits in this child's buffer.
    let _ = stdout.write_all(line.as_bytes());
    let _ = stdout.flush();
}
