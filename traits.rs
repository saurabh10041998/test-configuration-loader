use crate::error::ConfigError;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ConfigValue: the universal intermediate representation
// ---------------------------------------------------------------------------

/// A dynamically-typed configuration value used as the interchange format
/// between sources. Every [`ConfigSource`] produces a tree of `ConfigValue`s,
/// and the merge layer combines them before the final strongly-typed
/// deserialization step.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<ConfigValue>),
    Table(HashMap<String, ConfigValue>),
    Null,
}

impl ConfigValue {
    /// Deep-merge `other` into `self`. Values in `other` take precedence.
    /// Tables are merged recursively; all other types are replaced outright.
    pub fn merge(&mut self, other: ConfigValue) {
        match (self, other) {
            (ConfigValue::Table(base), ConfigValue::Table(overlay)) => {
                for (key, val) in overlay {
                    base.entry(key.clone())
                        .and_modify(|existing| existing.merge(val.clone()))
                        .or_insert(val);
                }
            }
            (this, other) => {
                *this = other;
            }
        }
    }

    /// Convenience: build an empty table.
    pub fn empty_table() -> Self {
        ConfigValue::Table(HashMap::new())
    }

    /// Try to interpret the value as a `String`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to interpret the value as an `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ConfigValue::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// Try to interpret the value as a `bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Lookup a dotted path inside nested tables (e.g. `"server.host"`).
    pub fn get_path(&self, path: &str) -> Option<&ConfigValue> {
        let mut current = self;
        for segment in path.split('.') {
            match current {
                ConfigValue::Table(map) => {
                    current = map.get(segment)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }
}

// ---------------------------------------------------------------------------
// ConfigSource trait (plugin point)
// ---------------------------------------------------------------------------

/// A pluggable configuration source.
///
/// Implementors provide a `name` (for diagnostics) and a `load` method that
/// returns a [`ConfigValue::Table`] tree. Sources are registered with the
/// [`ConfigLoader`](crate::loader::ConfigLoader) in the desired precedence
/// order.
///
/// # Example
///
/// ```ignore
/// struct MySource;
/// impl ConfigSource for MySource {
///     fn name(&self) -> &str { "my-source" }
///     fn load(&self) -> Result<ConfigValue, ConfigError> {
///         let mut map = HashMap::new();
///         map.insert("key".into(), ConfigValue::String("value".into()));
///         Ok(ConfigValue::Table(map))
///     }
/// }
/// ```
pub trait ConfigSource {
    /// Human-readable name for error messages.
    fn name(&self) -> &str;

    /// Load configuration and return a table of values.
    fn load(&self) -> Result<ConfigValue, ConfigError>;
}

// ---------------------------------------------------------------------------
// ConfigValidator trait (hook point)
// ---------------------------------------------------------------------------

/// A validation hook applied *after* all sources have been merged but *before*
/// the final typed config is returned.
///
/// Users can implement this to enforce cross-field constraints, range checks,
/// regex patterns, etc.
pub trait ConfigValidator {
    /// Validate the merged configuration tree.
    ///
    /// Return `Ok(())` if valid, or a [`ConfigError`] describing the problem.
    fn validate(&self, config: &ConfigValue) -> Result<(), ConfigError>;
}

// ---------------------------------------------------------------------------
// Deserialize bridge: ConfigValue → strongly-typed struct
// ---------------------------------------------------------------------------

/// Trait for converting a [`ConfigValue`] tree into a strongly-typed
/// configuration struct. Implement this for your application config type.
///
/// A blanket implementation is **not** provided on purpose so that users
/// control how missing/extra fields are handled.
pub trait FromConfigValue: Sized {
    fn from_config_value(value: &ConfigValue) -> Result<Self, ConfigError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_scalars() {
        let mut base = ConfigValue::String("old".into());
        base.merge(ConfigValue::String("new".into()));
        assert_eq!(base, ConfigValue::String("new".into()));
    }

    #[test]
    fn test_merge_tables_recursive() {
        let mut base = ConfigValue::Table(HashMap::from([
            ("a".into(), ConfigValue::String("1".into())),
            (
                "nested".into(),
                ConfigValue::Table(HashMap::from([
                    ("x".into(), ConfigValue::Integer(10)),
                    ("y".into(), ConfigValue::Integer(20)),
                ])),
            ),
        ]));

        let overlay = ConfigValue::Table(HashMap::from([
            ("b".into(), ConfigValue::String("2".into())),
            (
                "nested".into(),
                ConfigValue::Table(HashMap::from([("y".into(), ConfigValue::Integer(99))])),
            ),
        ]));

        base.merge(overlay);

        // "a" preserved, "b" added
        assert_eq!(base.get_path("a"), Some(&ConfigValue::String("1".into())));
        assert_eq!(base.get_path("b"), Some(&ConfigValue::String("2".into())));
        // nested.x preserved, nested.y overridden
        assert_eq!(base.get_path("nested.x"), Some(&ConfigValue::Integer(10)));
        assert_eq!(base.get_path("nested.y"), Some(&ConfigValue::Integer(99)));
    }

    #[test]
    fn test_get_path_missing() {
        let val = ConfigValue::empty_table();
        assert_eq!(val.get_path("nonexistent"), None);
    }
}
