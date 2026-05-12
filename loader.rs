use crate::error::ConfigError;
use crate::traits::{ConfigSource, ConfigValidator, ConfigValue, FromConfigValue};

/// The core configuration loader.
///
/// Sources are added in **ascending precedence order** — the last source added
/// wins on conflicts. The caller controls precedence entirely; there is no
/// built-in ordering assumption.
///
/// # Lifecycle
///
/// 1. `add_source` / `add_validator` – register plugins
/// 2. `load_raw` – loads & merges all sources into a [`ConfigValue`] tree
/// 3. `load::<T>` – additionally validates and deserializes into `T`
///
/// # Example
///
/// ```ignore
/// let config: AppConfig = ConfigLoader::new()
///     .add_source(Box::new(defaults))
///     .add_source(Box::new(file_source))
///     .add_source(Box::new(env_source))
///     .add_validator(Box::new(port_range_check))
///     .load()?;
/// ```
pub struct ConfigLoader {
    /// Sources in ascending precedence order.
    sources: Vec<Box<dyn ConfigSource>>,
    /// Validators run after merge, before deserialization.
    validators: Vec<Box<dyn ConfigValidator>>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            validators: Vec::new(),
        }
    }

    /// Register a configuration source. Sources are merged in the order they
    /// are added — **later sources override earlier ones**.
    pub fn add_source(mut self, source: Box<dyn ConfigSource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Register a validation hook.
    pub fn add_validator(mut self, validator: Box<dyn ConfigValidator>) -> Self {
        self.validators.push(validator);
        self
    }

    /// Load and merge all sources, returning the raw [`ConfigValue`] tree.
    pub fn load_raw(&self) -> Result<ConfigValue, ConfigError> {
        let mut merged = ConfigValue::empty_table();

        for source in &self.sources {
            let layer = source
                .load()
                .map_err(|e| ConfigError::SourceError(source.name().to_string(), e.to_string()))?;
            merged.merge(layer);
        }

        Ok(merged)
    }

    /// Load, merge, validate, and deserialize into the target type `T`.
    pub fn load<T: FromConfigValue>(&self) -> Result<T, ConfigError> {
        let merged = self.load_raw()?;

        // Run all validators; collect errors.
        let errors: Vec<ConfigError> = self
            .validators
            .iter()
            .filter_map(|v| v.validate(&merged).err())
            .collect();

        if !errors.is_empty() {
            return Err(ConfigError::Multiple(errors));
        }

        T::from_config_value(&merged)
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::DefaultSource;
    use std::collections::HashMap;

    // A minimal typed config for test purposes.
    #[derive(Debug, PartialEq)]
    struct TestConfig {
        host: String,
        port: i64,
        debug: bool,
    }

    impl FromConfigValue for TestConfig {
        fn from_config_value(value: &ConfigValue) -> Result<Self, ConfigError> {
            let host = value
                .get_path("host")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| ConfigError::MissingField("host".into()))?;
            let port = value
                .get_path("port")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ConfigError::MissingField("port".into()))?;
            let debug = value
                .get_path("debug")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok(Self { host, port, debug })
        }
    }

    // ── Precedence: later source wins ───────────────────────────────────

    #[test]
    fn test_precedence_last_wins() {
        let defaults = DefaultSource::from_map(HashMap::from([
            ("host", ConfigValue::String("localhost".into())),
            ("port", ConfigValue::Integer(3000)),
        ]));
        let overrides =
            DefaultSource::from_map(HashMap::from([("port", ConfigValue::Integer(9090))]));

        let config: TestConfig = ConfigLoader::new()
            .add_source(Box::new(defaults))
            .add_source(Box::new(overrides))
            .load()
            .expect("load should succeed");

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 9090); // overridden
    }

    // ── Validation failure ──────────────────────────────────────────────

    struct PortRangeValidator;
    impl ConfigValidator for PortRangeValidator {
        fn validate(&self, config: &ConfigValue) -> Result<(), ConfigError> {
            if let Some(port) = config.get_path("port").and_then(|v| v.as_i64()) {
                if !(1..=65535).contains(&port) {
                    return Err(ConfigError::ValidationFailed(
                        "port".into(),
                        format!("port {port} is outside 1–65535"),
                    ));
                }
            }
            Ok(())
        }
    }

    #[test]
    fn test_validation_blocks_load() {
        let src = DefaultSource::from_map(HashMap::from([
            ("host", ConfigValue::String("localhost".into())),
            ("port", ConfigValue::Integer(99999)),
        ]));

        let result: Result<TestConfig, _> = ConfigLoader::new()
            .add_source(Box::new(src))
            .add_validator(Box::new(PortRangeValidator))
            .load();

        assert!(result.is_err());
    }

    #[test]
    fn test_validation_passes() {
        let src = DefaultSource::from_map(HashMap::from([
            ("host", ConfigValue::String("0.0.0.0".into())),
            ("port", ConfigValue::Integer(8080)),
        ]));

        let config: TestConfig = ConfigLoader::new()
            .add_source(Box::new(src))
            .add_validator(Box::new(PortRangeValidator))
            .load()
            .expect("valid config");

        assert_eq!(config.port, 8080);
    }

    // ── Missing required field ──────────────────────────────────────────

    #[test]
    fn test_missing_required_field() {
        let src = DefaultSource::from_map(HashMap::from([("debug", ConfigValue::Bool(true))]));

        let result: Result<TestConfig, _> = ConfigLoader::new().add_source(Box::new(src)).load();

        assert!(result.is_err());
    }

    // ── Multiple validators, multiple errors ────────────────────────────

    struct AlwaysFailValidator(&'static str);
    impl ConfigValidator for AlwaysFailValidator {
        fn validate(&self, _: &ConfigValue) -> Result<(), ConfigError> {
            Err(ConfigError::ValidationFailed(
                self.0.into(),
                "always fails".into(),
            ))
        }
    }

    #[test]
    fn test_multiple_validation_errors_collected() {
        let src = DefaultSource::from_map(HashMap::from([
            ("host", ConfigValue::String("h".into())),
            ("port", ConfigValue::Integer(80)),
        ]));

        let result: Result<TestConfig, _> = ConfigLoader::new()
            .add_source(Box::new(src))
            .add_validator(Box::new(AlwaysFailValidator("a")))
            .add_validator(Box::new(AlwaysFailValidator("b")))
            .load();

        match result {
            Err(ConfigError::Multiple(errs)) => assert_eq!(errs.len(), 2),
            other => panic!("expected Multiple, got {other:?}"),
        }
    }

    // ── Configurable precedence ─────────────────────────────────────────

    #[test]
    fn test_reversed_precedence() {
        // Here we add "overrides" first so defaults actually win.
        let overrides =
            DefaultSource::from_map(HashMap::from([("port", ConfigValue::Integer(9090))]));
        let defaults = DefaultSource::from_map(HashMap::from([
            ("host", ConfigValue::String("localhost".into())),
            ("port", ConfigValue::Integer(3000)),
        ]));

        let config: TestConfig = ConfigLoader::new()
            .add_source(Box::new(overrides)) // lower precedence
            .add_source(Box::new(defaults)) // higher precedence (added last)
            .load()
            .expect("load");

        // defaults win because they were added last
        assert_eq!(config.port, 3000);
    }
}
