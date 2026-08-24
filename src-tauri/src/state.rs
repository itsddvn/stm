use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use tauri::{AppHandle, Emitter};
use tools_manager_core::{
    application::{
        dto::{AppViewModelDto, RefreshStatusDto, SurfaceStateDto},
        events::{AppEvent, AppEventType},
        service::PhaseThreeApplicationService,
    },
    domain::inventory::{Freshness, LoadState},
};

pub const REFRESH_EVENT_NAME: &str = "phase-three-scan";

const TOTAL_REFRESH_STEPS: usize = 7;
const REFRESH_STEP_DELAY_MS: u64 = 30;

pub struct AppState {
    service: Arc<PhaseThreeApplicationService>,
    refresh: Arc<Mutex<RefreshRuntime>>,
}

#[derive(Clone, Default)]
struct RefreshRuntime {
    generation: u64,
    in_progress: bool,
    cancel_requested: bool,
    operation_id: Option<String>,
    current_step: Option<String>,
    steps_completed: usize,
    last_snapshot_at: String,
    warnings: Vec<String>,
    last_snapshot: Option<AppViewModelDto>,
    display_snapshot: Option<AppViewModelDto>,
    result: Option<String>,
    error_message: Option<String>,
}

impl AppState {
    #[cfg(test)]
    pub fn new(manifest_dir: &str) -> Self {
        let project_root = PathBuf::from(manifest_dir)
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(manifest_dir));
        Self::with_service(Arc::new(PhaseThreeApplicationService::new(project_root)))
    }

    pub fn new_runtime(manifest_dir: &str) -> Self {
        let project_root = PathBuf::from(manifest_dir)
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(manifest_dir));
        let service = Arc::new(
            PhaseThreeApplicationService::new_runtime(project_root)
                .expect("desktop runtime requires an available user home directory"),
        );
        Self::with_service(service)
    }

    fn with_service(service: Arc<PhaseThreeApplicationService>) -> Self {
        let mut refresh = RefreshRuntime::default();
        if let Ok(snapshot) = service.current_snapshot() {
            refresh.last_snapshot = Some(snapshot.clone());
            refresh.display_snapshot = Some(snapshot);
        }
        if let Ok(status) = service.refresh_status() {
            refresh.last_snapshot_at = status.last_snapshot_at;
            refresh.warnings = status.warnings;
        }
        Self {
            service,
            refresh: Arc::new(Mutex::new(refresh)),
        }
    }

    pub fn service(&self) -> &PhaseThreeApplicationService {
        self.service.as_ref()
    }

    pub fn start_refresh(&self, app: AppHandle) -> AppViewModelDto {
        self.start_refresh_with_emitter(Arc::new(move |event| {
            let _ = app.emit(REFRESH_EVENT_NAME, event);
        }))
    }

    fn start_refresh_with_emitter(
        &self,
        emit_event: Arc<dyn Fn(&AppEvent) + Send + Sync>,
    ) -> AppViewModelDto {
        let mut runtime = self.refresh.lock().expect("refresh state");
        if runtime.in_progress {
            return current_display_snapshot(&runtime);
        }

        runtime.generation += 1;
        runtime.in_progress = true;
        runtime.cancel_requested = false;
        runtime.operation_id = Some(format!("inventory-refresh-{}", runtime.generation));
        runtime.current_step = Some("Preparing scan".to_string());
        runtime.steps_completed = 0;
        runtime.result = None;
        runtime.error_message = None;
        runtime.display_snapshot = Some(overlay_surface(
            runtime.last_snapshot.clone(),
            LoadState::Loading,
            Some("inventory.loading"),
            None,
        ));

        let generation = runtime.generation;
        let service = Arc::clone(&self.service);
        let refresh = Arc::clone(&self.refresh);
        let emit_event = Arc::clone(&emit_event);
        drop(runtime);

        thread::spawn(move || {
            let result = service.refresh_snapshot_with_progress(
                |event| {
                    {
                        let mut runtime = refresh.lock().expect("refresh state");
                        if runtime.generation != generation || !runtime.in_progress {
                            return;
                        }
                        runtime.current_step = Some(progress_message(&event));
                        if should_advance_progress(&event.event_type) {
                            runtime.steps_completed = runtime
                                .steps_completed
                                .saturating_add(1)
                                .min(TOTAL_REFRESH_STEPS);
                        }
                    }
                    emit_event(&event);
                    thread::sleep(Duration::from_millis(REFRESH_STEP_DELAY_MS));
                },
                || {
                    let runtime = refresh.lock().expect("refresh state");
                    runtime.generation != generation || runtime.cancel_requested
                },
            );

            match result {
                Ok(snapshot) => {
                    let status = service.refresh_status().ok();
                    let mut runtime = refresh.lock().expect("refresh state");
                    if runtime.generation != generation {
                        return;
                    }

                    runtime.in_progress = false;
                    runtime.cancel_requested = false;
                    runtime.operation_id = None;
                    runtime.current_step = None;
                    runtime.steps_completed = TOTAL_REFRESH_STEPS;
                    runtime.result = Some("success".to_string());
                    runtime.error_message = None;
                    runtime.last_snapshot = Some(snapshot.clone());
                    runtime.display_snapshot = Some(snapshot);
                    if let Some(status) = status {
                        runtime.last_snapshot_at = status.last_snapshot_at;
                        runtime.warnings = status.warnings;
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    let cancelled = message.contains("refresh cancelled");
                    let status = service.refresh_status().ok();
                    let mut runtime = refresh.lock().expect("refresh state");
                    if runtime.generation != generation {
                        return;
                    }

                    runtime.in_progress = false;
                    runtime.cancel_requested = false;
                    runtime.operation_id = None;
                    runtime.current_step = None;
                    runtime.error_message = Some(message.clone());
                    runtime.result =
                        Some(if cancelled { "cancelled" } else { "failed" }.to_string());
                    if let Some(status) = status {
                        runtime.last_snapshot_at = status.last_snapshot_at;
                        runtime.warnings = status.warnings;
                    }
                    runtime.display_snapshot = Some(overlay_surface(
                        runtime.last_snapshot.clone(),
                        LoadState::Ready,
                        Some(if cancelled {
                            "operation.cancelled"
                        } else {
                            "operation.failed"
                        }),
                        None,
                    ));
                }
            }
        });

        let runtime = self.refresh.lock().expect("refresh state");
        current_display_snapshot(&runtime)
    }

    pub fn refresh_status(&self) -> RefreshStatusDto {
        let runtime = self.refresh.lock().expect("refresh state");
        let snapshot = current_display_snapshot(&runtime);
        RefreshStatusDto {
            surface: snapshot.surface.clone(),
            last_snapshot_at: runtime.last_snapshot_at.clone(),
            warning_count: runtime.warnings.len(),
            warnings: runtime.warnings.clone(),
            in_progress: runtime.in_progress,
            can_cancel: runtime.in_progress,
            operation_id: runtime.operation_id.clone(),
            current_step: runtime.current_step.clone(),
            steps_completed: runtime.steps_completed,
            total_steps: TOTAL_REFRESH_STEPS,
            snapshot: Some(snapshot),
            result: runtime.result.clone(),
            error_message: runtime.error_message.clone(),
        }
    }

    pub fn cancel_operation(&self, operation_id: &str) -> bool {
        let mut runtime = self.refresh.lock().expect("refresh state");
        if runtime.in_progress && runtime.operation_id.as_deref() == Some(operation_id) {
            runtime.cancel_requested = true;
            runtime.current_step = Some("Cancelling scan".to_string());
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
impl AppState {
    fn start_refresh_for_test(&self) -> AppViewModelDto {
        self.start_refresh_with_emitter(Arc::new(|_| {}))
    }
}

fn current_display_snapshot(runtime: &RefreshRuntime) -> AppViewModelDto {
    runtime
        .display_snapshot
        .clone()
        .or_else(|| runtime.last_snapshot.clone())
        .unwrap_or_else(|| {
            overlay_surface(
                None,
                LoadState::Loading,
                Some("inventory.loading"),
                Some(Freshness::Unknown),
            )
        })
}

fn overlay_surface(
    snapshot: Option<AppViewModelDto>,
    load_state: LoadState,
    reason_code: Option<&str>,
    freshness: Option<Freshness>,
) -> AppViewModelDto {
    let mut next = snapshot.unwrap_or_else(|| AppViewModelDto {
        surface: SurfaceStateDto {
            load_state: LoadState::Loading,
            reason_code: Some("inventory.loading".to_string()),
            freshness: Freshness::Unknown,
        },
        tools: Vec::new(),
        skills: Vec::new(),
        mcp_servers: Vec::new(),
        updates: Vec::new(),
        operations: Vec::new(),
    });
    next.surface = SurfaceStateDto {
        load_state,
        reason_code: reason_code.map(str::to_string),
        freshness: freshness.unwrap_or_else(|| next.surface.freshness.clone()),
    };
    next
}

fn should_advance_progress(event_type: &AppEventType) -> bool {
    matches!(
        event_type,
        AppEventType::RefreshStarted
            | AppEventType::CatalogValidated
            | AppEventType::InventoryScanned
            | AppEventType::SkillsScanned
            | AppEventType::McpDiscovered
            | AppEventType::SnapshotCommitted
            | AppEventType::DiagnosticsReady
    )
}

fn progress_message(event: &AppEvent) -> String {
    match event.event_type {
        AppEventType::RefreshStarted => "Preparing scan".to_string(),
        AppEventType::CatalogValidated => "Catalog verified".to_string(),
        AppEventType::InventoryScanned => "Tool inventory refreshed".to_string(),
        AppEventType::SkillsScanned => "Skill inventory refreshed".to_string(),
        AppEventType::McpDiscovered => "MCP inventory refreshed".to_string(),
        AppEventType::SnapshotRecovered => "Recovered last good snapshot".to_string(),
        AppEventType::SnapshotCommitted => "Snapshot saved".to_string(),
        AppEventType::DiagnosticsReady => "Diagnostics prepared".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn refresh_status_reaches_success_with_snapshot() {
        let state = AppState::new(env!("CARGO_MANIFEST_DIR"));

        let initial = state.start_refresh_for_test();
        assert_eq!(initial.surface.load_state, LoadState::Loading);

        let status = wait_for_refresh(&state);
        assert_eq!(status.result.as_deref(), Some("success"));
        assert!(!status.in_progress);
        assert!(status
            .snapshot
            .as_ref()
            .is_some_and(|view| !view.tools.is_empty()));
    }

    #[test]
    fn refresh_cancel_preserves_last_good_snapshot() {
        let state = AppState::new(env!("CARGO_MANIFEST_DIR"));

        let _ = state.start_refresh_for_test();
        let operation_id = loop {
            let status = state.refresh_status();
            if let Some(operation_id) = status.operation_id {
                break operation_id;
            }
            thread::sleep(Duration::from_millis(10));
        };

        assert!(state.cancel_operation(&operation_id));
        let status = wait_for_refresh(&state);
        assert_eq!(status.result.as_deref(), Some("cancelled"));
        assert_eq!(
            status
                .snapshot
                .and_then(|view| view.surface.reason_code)
                .as_deref(),
            Some("operation.cancelled")
        );
    }

    fn wait_for_refresh(state: &AppState) -> RefreshStatusDto {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = state.refresh_status();
            if !status.in_progress {
                return status;
            }
            assert!(Instant::now() < deadline, "refresh did not settle in time");
            thread::sleep(Duration::from_millis(20));
        }
    }
}
