mod command;
mod evidence;
mod executor;
mod linux;
mod mcp_planner;
mod planner;
#[cfg(test)]
mod platform_contract_tests;
mod service;
mod skill_planner;
mod skill_source;
mod source_probe;
mod source_registry;
pub(crate) mod time;

#[cfg(test)]
pub(crate) use command::executable_identity;
pub(crate) use command::{compile_mcp_stdio, CompiledManagerCommand, ExecutableIdentity};

pub use service::LifecycleService;
