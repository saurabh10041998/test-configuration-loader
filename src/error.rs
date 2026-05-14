use std::fmt;

/// All errors that can occur during configuration loading, merging, or validation.
#[derive(Debug)]
pub enum ConfigError {
    /// A required configuration field is missing after all sources have been merged.
    MissingField(String),
    /// A configuration value failed validation (field name, reason).
    ValidationFailed(String, String),
    /// An error occurred while reading or parsing a configuration source.
    SourceError(String, String),
    /// A file I/O error occurred.
    IoError(String, std::io::Error),
    /// A parse/deserialization error occurred.
    ParseError(String, String),
    /// Multiple errors collected during validation.
    Multiple(Vec<ConfigError>),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingField(field) => {
                write!(f, "missing required configuration field: '{field}'")
            }
            ConfigError::ValidationFailed(field, reason) => {
                write!(f, "validation failed for '{field}': {reason}")
            }
            ConfigError::SourceError(source, reason) => {
                write!(f, "error loading source '{source}': {reason}")
            }
            ConfigError::IoError(path, err) => {
                write!(f, "I/O error for '{path}': {err}")
            }
            ConfigError::ParseError(source, reason) => {
                write!(f, "parse error in '{source}': {reason}")
            }
            ConfigError::Multiple(errors) => {
                writeln!(f, "multiple configuration errors:")?;
                for (i, err) in errors.iter().enumerate() {
                    writeln!(f, "  {}: {err}", i + 1)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::IoError(_, err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ConfigError::MissingField("port".to_string());
        assert!(err.to_string().contains("port"));

        let err = ConfigError::ValidationFailed("port".to_string(), "must be > 0".to_string());
        assert!(err.to_string().contains("port"));
        assert!(err.to_string().contains("must be > 0"));
    }

    #[test]
    fn test_multiple_errors_display() {
        let err = ConfigError::Multiple(vec![
            ConfigError::MissingField("host".to_string()),
            ConfigError::ValidationFailed("port".to_string(), "out of range".to_string()),
        ]);
        let s = err.to_string();
        assert!(s.contains("host"));
        assert!(s.contains("port"));
    }
}
