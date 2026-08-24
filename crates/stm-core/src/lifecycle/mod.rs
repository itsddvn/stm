mod command;
mod evidence;
mod executor;
mod mcp_planner;
mod planner;
mod service;
mod skill_planner;
mod source_probe;
mod source_registry;
pub(crate) mod time;

pub use command::{
    command_environment, lifecycle_privilege, manager_command_vector, npm_source_args,
    validate_package_id, validate_target_version, CompiledManagerCommand, ExecutableIdentity,
};
pub use evidence::{ManagerEvidencePort, ManagerStateEvidence};
pub use executor::{LifecycleExecutionPort, ManagedExecutionResult};
pub use service::{LifecycleService, LifecycleServiceDependencies};
pub use source_probe::{analyze_source_with_probe, SourceProbe, SourceProbeEvidence};
