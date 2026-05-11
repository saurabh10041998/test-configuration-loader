//! Ready-made validators that cover common cases.
//!
//! Users can also implement [`ConfigValidator`] directly for custom logic.

use crate::error::ConfigError;
use crate::traits::{ConfigValidator, ConfigValue};

/// Validates that a set of dotted paths are present and non-null.
pub struct RequiredFieldsValidator {
    fields: Vec<String>,
}

impl RequiredFieldsValidator {
    pub fn new(fields: Vec<impl Into<String>>) -> Self {
        Self {
            fields: fields.into_iter().map(Into::into).collect(),
        }
    }
}

impl ConfigValidator for RequiredFieldsValidator {
    fn validate(&self, config: &ConfigValue) -> Result<(), ConfigError> {
        let mut errors = Vec::new();
        for field in &self.fields {
            match config.get_path(field) {
                None | Some(ConfigValue::Null) => {
                    errors.push(ConfigError::MissingField(field.clone()));
                }
                _ => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors.into_iter().next().expect("checked len"))
        } else {
            Err(ConfigError::Multiple(errors))
        }
    }
}

/// Validates that a numeric field falls within an inclusive range.
pub struct RangeValidator {
    field: String,
    min: i64,
    max: i64,
}

impl RangeValidator {
    pub fn new(field: impl Into<String>, min: i64, max: i64) -> Self {
        Self {
            field: field.into(),
            min,
            max,
        }
    }
}

impl ConfigValidator for RangeValidator {
    fn validate(&self, config: &ConfigValue) -> Result<(), ConfigError> {
        if let Some(val) = config.get_path(&self.field) {
            if let Some(n) = val.as_i64() {
                if n < self.min || n > self.max {
                    return Err(ConfigError::ValidationFailed(
                        self.field.clone(),
                        format!("value {n} is outside allowed range [{}, {}]", self.min, self.max),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A validator built from a closure — for quick one-off checks.
pub struct FnValidator<F>
where
    F: Fn(&ConfigValue) -> Result<(), ConfigError>,
{
    func: F,
}

impl<F> FnValidator<F>
where
    F: Fn(&ConfigValue) -> Result<(), ConfigError>,
{
    pub fn new(func: F) -> Self {
        Self { func }
    }
}

impl<F> ConfigValidator for FnValidator<F>
where
    F: Fn(&ConfigValue) -> Result<(), ConfigError>,
{
    fn validate(&self, config: &ConfigValue) -> Result<(), ConfigError> {
        (self.func)(config)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_config() -> ConfigValue {
        ConfigValue::Table(HashMap::from([
            ("host".into(), ConfigValue::String("localhost".into())),
            ("port".into(), ConfigValue::Integer(8080)),
            (
                "db".into(),
                ConfigValue::Table(HashMap::from([(
                    "url".into(),
                    ConfigValue::String("postgres://localhost/test".into()),
                )])),
            ),
        ]))
    }

    #[test]
    fn test_required_fields_pass() {
        let v = RequiredFieldsValidator::new(vec!["host", "port", "db.url"]);
        assert!(v.validate(&sample_config()).is_ok());
    }

    #[test]
    fn test_required_fields_fail() {
        let v = RequiredFieldsValidator::new(vec!["host", "missing_field"]);
        assert!(v.validate(&sample_config()).is_err());
    }

    #[test]
    fn test_range_validator_pass() {
        let v = RangeValidator::new("port", 1, 65535);
        assert!(v.validate(&sample_config()).is_ok());
    }

    #[test]
    fn test_range_validator_fail() {
        let v = RangeValidator::new("port", 9000, 9999);
        assert!(v.validate(&sample_config()).is_err());
    }

    #[test]
    fn test_fn_validator() {
        let v = FnValidator::new(|cfg| {
            let host = cfg
                .get_path("host")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if host.is_empty() {
                Err(ConfigError::ValidationFailed(
                    "host".into(),
                    "must not be empty".into(),
                ))
            } else {
                Ok(())
            }
        });
        assert!(v.validate(&sample_config()).is_ok());
    }
}
