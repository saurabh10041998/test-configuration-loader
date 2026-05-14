//! # Unified Configuration Loader
//!
//! A trait-based, plugin-extensible configuration system for Rust applications.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use unified_config_loader::prelude::*;
//! use unified_config_loader::sources::{DefaultSource, EnvSource, FileSource, FileFormat};
//! use unified_config_loader::validators::RangeValidator;
//! use std::collections::HashMap;
//!
//! #[derive(Debug)]
//! struct AppConfig {
//!     host: String,
//!     port: i64,
//! }
//!
//! impl FromConfigValue for AppConfig {
//!     fn from_config_value(value: &ConfigValue) -> Result<Self, ConfigError> {
//!         Ok(Self {
//!             host: value.get_path("server.host")
//!                 .and_then(|v| v.as_str())
//!                 .map(String::from)
//!                 .ok_or_else(|| ConfigError::MissingField("server.host".into()))?,
//!             port: value.get_path("server.port")
//!                 .and_then(|v| v.as_i64())
//!                 .ok_or_else(|| ConfigError::MissingField("server.port".into()))?,
//!         })
//!     }
//! }
//! ```

pub mod error;
pub mod loader;
pub mod sources;
pub mod traits;
pub mod validators;

/// Convenience re-exports for the most commonly used types.
pub mod prelude {
    pub use crate::error::ConfigError;
    pub use crate::loader::ConfigLoader;
    pub use crate::traits::{ConfigSource, ConfigValidator, ConfigValue, FromConfigValue};
}
