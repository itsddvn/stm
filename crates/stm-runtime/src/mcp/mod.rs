mod backup_crypto;
pub mod health;
pub mod lifecycle;

#[cfg(test)]
mod lifecycle_tests;

pub use lifecycle::McpConfigMaterializer;
