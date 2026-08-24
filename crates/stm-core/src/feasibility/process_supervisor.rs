use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AllowedCommand {
    pub alias: String,
    pub executable: PathBuf,
    pub args: Vec<ArgRule>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArgRule {
    Exact(String),
    Pattern(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequest {
    pub command_alias: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub output_limit_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionOutcome {
    pub status: ExecutionStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Completed,
    TimedOut,
    Cancelled,
    OutputLimitExceeded,
}

#[derive(Debug, Clone, Default)]
pub struct CancelSignal {
    cancelled: Arc<AtomicBool>,
}

impl CancelSignal {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct AllowlistedProcessSupervisor {
    commands: BTreeMap<String, AllowedCommand>,
}

impl AllowlistedProcessSupervisor {
    pub fn new(commands: impl IntoIterator<Item = AllowedCommand>) -> Self {
        let commands = commands
            .into_iter()
            .map(|command| (command.alias.clone(), command))
            .collect();
        Self { commands }
    }

    pub fn execute(
        &self,
        request: &ExecutionRequest,
        cancel: &CancelSignal,
    ) -> Result<ExecutionOutcome, CoreError> {
        self.execute_with_spawn_callback(request, cancel, |_| Ok(()))
    }

    pub fn execute_with_spawn_callback<F>(
        &self,
        request: &ExecutionRequest,
        cancel: &CancelSignal,
        on_spawn: F,
    ) -> Result<ExecutionOutcome, CoreError>
    where
        F: FnOnce(u32) -> Result<(), CoreError>,
    {
        let command = self
            .commands
            .get(&request.command_alias)
            .ok_or_else(|| CoreError::CommandDenied(request.command_alias.clone()))?;
        self.validate_args(command, &request.args)?;

        let started = Instant::now();
        let mut process = Command::new(&command.executable);
        process
            .args(&request.args)
            .env_clear()
            .envs(minimum_environment())
            .envs(&command.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(parent) = command.executable.parent() {
            process.current_dir(parent);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.process_group(0);
        }
        let mut child = process
            .spawn()
            .map_err(|error| CoreError::ProcessSpawn(error.to_string()))?;
        if let Err(error) = on_spawn(child.id()) {
            terminate_process_tree(&mut child);
            let _ = child.wait();
            return Err(error);
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::ProcessExecution("stdout unavailable".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::ProcessExecution("stderr unavailable".to_string()))?;

        let stdout_collector =
            Arc::new(Mutex::new(StreamCollector::new(request.output_limit_bytes)));
        let stderr_collector =
            Arc::new(Mutex::new(StreamCollector::new(request.output_limit_bytes)));
        let limit_hit = Arc::new(AtomicBool::new(false));

        let stdout_handle = spawn_reader(
            stdout,
            Arc::clone(&stdout_collector),
            Arc::clone(&limit_hit),
        );
        let stderr_handle = spawn_reader(
            stderr,
            Arc::clone(&stderr_collector),
            Arc::clone(&limit_hit),
        );

        let deadline = started + Duration::from_millis(request.timeout_ms);
        let (status, exit_code) = loop {
            if cancel.is_cancelled() {
                terminate_process_tree(&mut child);
                let exit = child.wait().ok().and_then(|status| status.code());
                break (ExecutionStatus::Cancelled, exit);
            }

            if limit_hit.load(Ordering::SeqCst) {
                terminate_process_tree(&mut child);
                let exit = child.wait().ok().and_then(|status| status.code());
                break (ExecutionStatus::OutputLimitExceeded, exit);
            }

            if Instant::now() >= deadline {
                terminate_process_tree(&mut child);
                let exit = child.wait().ok().and_then(|status| status.code());
                break (ExecutionStatus::TimedOut, exit);
            }

            if let Some(exit) = child.try_wait()? {
                break (ExecutionStatus::Completed, exit.code());
            }

            thread::sleep(Duration::from_millis(10));
        };

        stdout_handle
            .join()
            .map_err(|_| CoreError::ProcessExecution("stdout reader panicked".to_string()))?;
        stderr_handle
            .join()
            .map_err(|_| CoreError::ProcessExecution("stderr reader panicked".to_string()))?;

        let stdout = stdout_collector
            .lock()
            .map_err(|_| CoreError::ProcessExecution("stdout collector poisoned".to_string()))?
            .finish();
        let stderr = stderr_collector
            .lock()
            .map_err(|_| CoreError::ProcessExecution("stderr collector poisoned".to_string()))?
            .finish();

        let truncated = stdout.truncated || stderr.truncated;

        Ok(ExecutionOutcome {
            status,
            exit_code,
            stdout: stdout.content,
            stderr: stderr.content,
            truncated,
            elapsed_ms: started.elapsed().as_millis(),
        })
    }

    fn validate_args(
        &self,
        command: &AllowedCommand,
        actual_args: &[String],
    ) -> Result<(), CoreError> {
        if command.args.len() != actual_args.len() {
            return Err(CoreError::ArgumentDenied(format!(
                "expected {} args for {}, got {}",
                command.args.len(),
                command.alias,
                actual_args.len()
            )));
        }

        for (rule, actual) in command.args.iter().zip(actual_args) {
            match rule {
                ArgRule::Exact(expected) if expected != actual => {
                    return Err(CoreError::ArgumentDenied(actual.clone()));
                }
                ArgRule::Pattern(pattern) => {
                    let regex = Regex::new(pattern)
                        .map_err(|error| CoreError::ArgumentDenied(error.to_string()))?;
                    if !regex.is_match(actual) {
                        return Err(CoreError::ArgumentDenied(actual.clone()));
                    }
                }
                ArgRule::Exact(_) => {}
            }
        }

        Ok(())
    }
}
fn minimum_environment() -> BTreeMap<String, String> {
    const ALLOWED_KEYS: &[&str] = &[
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "SystemRoot",
        "TMP",
        "TMPDIR",
        "TEMP",
        "USERPROFILE",
    ];
    let mut environment: BTreeMap<String, String> = ALLOWED_KEYS
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_string(), value))
        })
        .collect();
    #[cfg(unix)]
    environment.insert(
        "PATH".to_string(),
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    );
    #[cfg(windows)]
    {
        let system_root = environment
            .get("SystemRoot")
            .cloned()
            .unwrap_or_else(|| r"C:\Windows".to_string());
        let mut paths = vec![
            format!(r"{system_root}\System32"),
            system_root.clone(),
            format!(r"{system_root}\System32\WindowsPowerShell\v1.0"),
        ];
        if let Some(local_app_data) = environment.get("LOCALAPPDATA") {
            paths.push(format!(r"{local_app_data}\Microsoft\WindowsApps"));
        }
        environment.insert("PATH".to_string(), paths.join(";"));
    }
    environment
}

#[cfg(unix)]
fn terminate_process_tree(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", group.as_str()])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut std::process::Child) {
    let _ = Command::new("taskkill.exe")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    collector: Arc<Mutex<StreamCollector>>,
    limit_hit: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let mut collector = match collector.lock() {
                        Ok(collector) => collector,
                        Err(_) => return,
                    };
                    if collector.push(&buffer[..read]) {
                        limit_hit.store(true, Ordering::SeqCst);
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    })
}

#[derive(Debug)]
struct StreamCollector {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl StreamCollector {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            truncated: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> bool {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        if chunk.len() > remaining {
            self.bytes.extend_from_slice(&chunk[..remaining]);
            self.truncated = true;
            return true;
        }

        self.bytes.extend_from_slice(chunk);
        false
    }

    fn finish(&self) -> FinishedStream {
        FinishedStream {
            content: String::from_utf8_lossy(&self.bytes).into_owned(),
            truncated: self.truncated,
        }
    }
}

#[derive(Debug)]
struct FinishedStream {
    content: String,
    truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn process_supervisor_supports_timeout_and_output_limits() {
        let supervisor = AllowlistedProcessSupervisor::new([
            AllowedCommand {
                alias: "sleep".to_string(),
                executable: PathBuf::from("/bin/sleep"),
                args: vec![ArgRule::Exact("2".to_string())],
                environment: BTreeMap::new(),
            },
            AllowedCommand {
                alias: "yes".to_string(),
                executable: PathBuf::from("/usr/bin/yes"),
                args: vec![ArgRule::Exact("phase-two".to_string())],
                environment: BTreeMap::new(),
            },
        ]);

        let timeout = supervisor
            .execute(
                &ExecutionRequest {
                    command_alias: "sleep".to_string(),
                    args: vec!["2".to_string()],
                    timeout_ms: 50,
                    output_limit_bytes: 256,
                },
                &CancelSignal::default(),
            )
            .expect("timeout probe should run");
        assert_eq!(timeout.status, ExecutionStatus::TimedOut);

        let output_limit = supervisor
            .execute(
                &ExecutionRequest {
                    command_alias: "yes".to_string(),
                    args: vec!["phase-two".to_string()],
                    timeout_ms: 1_000,
                    output_limit_bytes: 128,
                },
                &CancelSignal::default(),
            )
            .expect("output-limit probe should run");
        assert_eq!(output_limit.status, ExecutionStatus::OutputLimitExceeded);
        assert!(output_limit.truncated);
    }
}
