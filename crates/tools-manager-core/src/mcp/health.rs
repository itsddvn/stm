use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    process::{ChildStdout, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use serde_json::json;

use crate::{
    domain::mcp::{
        AuthReferenceKind, AuthReferenceState, McpHealthState, McpServerRecord, McpTransport,
    },
    lifecycle::{compile_mcp_stdio, CompiledManagerCommand, ExecutableIdentity},
};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_HEALTH_RESPONSE_BYTES: usize = 256 * 1024;

pub(crate) fn check_protocol_health(
    server: &McpServerRecord,
    executable_identities: &[ExecutableIdentity],
) -> McpHealthState {
    if server.auth_state == AuthReferenceState::ReferenceMissing {
        return McpHealthState::Degraded;
    }
    if server.transport == McpTransport::Stdio {
        return check_stdio_health(server, executable_identities);
    }
    let Ok(url) = url::Url::parse(&server.command_or_url) else {
        return McpHealthState::Unreachable;
    };
    if url
        .host_str()
        .is_some_and(|host| host.ends_with(".example.com") || host == "example.com")
    {
        return McpHealthState::Unknown;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(HEALTH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return McpHealthState::Unknown;
    };
    let mut request = client
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json");
    if !server.auth_references.is_empty() {
        let Some(token) = single_environment_credential(server) else {
            return McpHealthState::Degraded;
        };
        request = request.bearer_auth(token);
    }
    let response = match request.body(initialize_request().to_string()).send() {
        Ok(response) => response,
        Err(_) => return McpHealthState::Unreachable,
    };
    if matches!(response.status().as_u16(), 401 | 403) {
        return McpHealthState::Degraded;
    }
    if !response.status().is_success() {
        return McpHealthState::Unreachable;
    }
    if response
        .content_length()
        .is_some_and(|length| length as usize > MAX_HEALTH_RESPONSE_BYTES)
    {
        return McpHealthState::Degraded;
    }
    let mut bytes = Vec::new();
    if response
        .take((MAX_HEALTH_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > MAX_HEALTH_RESPONSE_BYTES
    {
        return McpHealthState::Degraded;
    }
    classify_initialize_response(&bytes)
}

fn check_stdio_health(
    server: &McpServerRecord,
    expected_identities: &[ExecutableIdentity],
) -> McpHealthState {
    let Ok(Some(compiled)) = compile_mcp_stdio(&server.command_or_url, &server.args) else {
        return McpHealthState::Unreachable;
    };
    if expected_identities.is_empty() || compiled.identities != expected_identities {
        return McpHealthState::Degraded;
    }
    check_compiled_stdio_health(server, &compiled)
}

fn check_compiled_stdio_health(
    server: &McpServerRecord,
    compiled: &CompiledManagerCommand,
) -> McpHealthState {
    let mut command = Command::new(&compiled.executable);
    command
        .args(&compiled.argv)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for key in [
        "HOME",
        "PATH",
        "SystemRoot",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USERPROFILE",
    ] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    for reference in &server.auth_references {
        let AuthReferenceKind::EnvVar = reference.kind else {
            return McpHealthState::Degraded;
        };
        let Some(value) = env::var_os(&reference.reference).filter(|value| !value.is_empty())
        else {
            return McpHealthState::Degraded;
        };
        command.env(&reference.reference, value);
    }
    let Ok(mut child) = command.spawn() else {
        return McpHealthState::Unreachable;
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return McpHealthState::Unreachable;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return McpHealthState::Unreachable;
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(read_bounded_stdio_message(stdout));
    });
    let payload = initialize_request().to_string();
    if stdin.write_all(payload.as_bytes()).is_err()
        || stdin.write_all(b"\n").is_err()
        || stdin.flush().is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return McpHealthState::Unreachable;
    }
    drop(stdin);
    let result = receiver.recv_timeout(HEALTH_TIMEOUT);
    let _ = child.kill();
    let _ = child.wait();
    match result {
        Ok(Some(bytes)) => classify_initialize_response(&bytes),
        Ok(None) => McpHealthState::Degraded,
        Err(_) => McpHealthState::Unreachable,
    }
}

fn read_bounded_stdio_message(stdout: ChildStdout) -> Option<Vec<u8>> {
    let mut reader = BufReader::new(stdout);
    let mut message = Vec::new();
    loop {
        let available = reader.fill_buf().ok()?;
        if available.is_empty() {
            return (!message.is_empty()).then_some(message);
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if message.len() + take > MAX_HEALTH_RESPONSE_BYTES {
            return None;
        }
        message.extend_from_slice(&available[..take]);
        reader.consume(take);
        if message.last() == Some(&b'\n') {
            return Some(message);
        }
    }
}

fn single_environment_credential(server: &McpServerRecord) -> Option<String> {
    if server.auth_references.len() != 1
        || server.auth_references[0].kind != AuthReferenceKind::EnvVar
    {
        return None;
    }
    env::var(&server.auth_references[0].reference)
        .ok()
        .filter(|value| !value.is_empty())
}

fn initialize_request() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": "stm-health",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "stm-health", "version": env!("CARGO_PKG_VERSION")}
        }
    })
}

fn classify_initialize_response(bytes: &[u8]) -> McpHealthState {
    let payload = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.trim().strip_prefix("data:").map(str::trim))
                .or_else(|| Some(text.trim()))
        })
        .unwrap_or_default();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return McpHealthState::Degraded;
    };
    if value.get("jsonrpc").and_then(serde_json::Value::as_str) != Some("2.0")
        || value.get("id").and_then(serde_json::Value::as_str) != Some("stm-health")
    {
        return McpHealthState::Degraded;
    }
    if value
        .get("result")
        .and_then(|result| result.get("capabilities"))
        .and_then(serde_json::Value::as_object)
        .is_some()
    {
        McpHealthState::Healthy
    } else {
        McpHealthState::Degraded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialization_capabilities_are_healthy_without_invoking_a_tool() {
        assert_eq!(
            classify_initialize_response(
                br#"{"jsonrpc":"2.0","id":"stm-health","result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}}"#,
            ),
            McpHealthState::Healthy
        );
        assert_eq!(
            classify_initialize_response(br#"{"jsonrpc":"2.0","id":"stm-health","result":{}}"#,),
            McpHealthState::Degraded
        );
    }

    #[cfg(unix)]
    #[test]
    fn approved_stdio_health_runs_only_initialize_against_the_bound_executable() {
        use std::{fs, os::unix::fs::PermissionsExt};

        let temp = tempfile::TempDir::new().expect("tempdir");
        let executable = temp.path().join("mcp-health-fixture.sh");
        fs::write(
            &executable,
            b"#!/bin/sh\nIFS= read -r request\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":\"stm-health\",\"result\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{\"tools\":{}}}}'\n",
        )
        .expect("fixture executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("fixture permissions");
        let identity = crate::lifecycle::executable_identity(executable).expect("fixture identity");
        let compiled = CompiledManagerCommand {
            executable: identity.canonical_path.clone(),
            argv: Vec::new(),
            identities: vec![identity],
        };
        let server = crate::mcp::policy::approved_server(
            "filesystem",
            &["/tmp".into()],
            &[crate::domain::mcp::McpClientName::Codex],
        )
        .expect("approved mapping")
        .expect("filesystem mapping");

        assert_eq!(
            check_compiled_stdio_health(&server, &compiled),
            McpHealthState::Healthy
        );
    }
}
