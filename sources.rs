use crate::error::ConfigError;
use crate::traits::{ConfigSource, ConfigValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ===========================================================================
// 1. DefaultSource – hardcoded defaults supplied via closure
// ===========================================================================

/// A source that returns a fixed [`ConfigValue`] tree built at construction
/// time. Typically used as the lowest-precedence layer.
pub struct DefaultSource {
    values: ConfigValue,
}

impl DefaultSource {
    pub fn new(values: ConfigValue) -> Self {
        Self { values }
    }

    /// Convenience: build from a `HashMap<&str, ConfigValue>`.
    pub fn from_map(map: HashMap<&str, ConfigValue>) -> Self {
        let table = map
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        Self {
            values: ConfigValue::Table(table),
        }
    }
}

impl ConfigSource for DefaultSource {
    fn name(&self) -> &str {
        "defaults"
    }

    fn load(&self) -> Result<ConfigValue, ConfigError> {
        Ok(self.values.clone())
    }
}

// ===========================================================================
// 2. EnvSource – reads environment variables with a configurable prefix
// ===========================================================================

/// Maps environment variables into a [`ConfigValue`] table.
///
/// Variables are expected to follow the pattern `{PREFIX}_KEY` where
/// double-underscore (`__`) is treated as a nesting separator.
///
/// Example: `APP_SERVER__PORT=8080` → `{ "server": { "port": "8080" } }`
///
/// The reader function is injectable so tests can substitute a fake
/// environment.
pub struct EnvSource<F>
where
    F: Fn() -> Vec<(String, String)>,
{
    prefix: String,
    reader: F,
}

impl<F> EnvSource<F>
where
    F: Fn() -> Vec<(String, String)>,
{
    pub fn new(prefix: impl Into<String>, reader: F) -> Self {
        Self {
            prefix: prefix.into(),
            reader,
        }
    }
}

/// Default constructor that reads from the real environment.
impl EnvSource<fn() -> Vec<(String, String)>> {
    pub fn from_env(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            reader: || std::env::vars().collect(),
        }
    }
}

impl<F> ConfigSource for EnvSource<F>
where
    F: Fn() -> Vec<(String, String)>,
{
    fn name(&self) -> &str {
        "environment"
    }

    fn load(&self) -> Result<ConfigValue, ConfigError> {
        let vars = (self.reader)();
        let prefix_upper = format!("{}_", self.prefix.to_uppercase());

        let mut root: HashMap<String, ConfigValue> = HashMap::new();

        for (key, value) in vars {
            let upper_key = key.to_uppercase();
            if let Some(stripped) = upper_key.strip_prefix(&prefix_upper) {
                let segments: Vec<&str> = stripped.split("__").collect();
                insert_nested(&mut root, &segments, value);
            }
        }

        Ok(ConfigValue::Table(root))
    }
}

/// Recursively insert a value into nested hash maps based on segments.
fn insert_nested(map: &mut HashMap<String, ConfigValue>, segments: &[&str], value: String) {
    if segments.is_empty() {
        return;
    }
    let key = segments[0].to_lowercase();
    if segments.len() == 1 {
        // Leaf – try to parse as int / bool, fall back to string
        let parsed = if let Ok(n) = value.parse::<i64>() {
            ConfigValue::Integer(n)
        } else if let Ok(b) = value.parse::<bool>() {
            ConfigValue::Bool(b)
        } else if let Ok(f) = value.parse::<f64>() {
            ConfigValue::Float(f)
        } else {
            ConfigValue::String(value)
        };
        map.insert(key, parsed);
    } else {
        let entry = map
            .entry(key)
            .or_insert_with(|| ConfigValue::Table(HashMap::new()));
        if let ConfigValue::Table(inner) = entry {
            insert_nested(inner, &segments[1..], value);
        }
    }
}

// ===========================================================================
// 3. FileSource – TOML / YAML / JSON file reader
// ===========================================================================

/// Supported configuration file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Toml,
    Yaml,
    Json,
}

impl FileFormat {
    /// Detect format from file extension, returning `None` for unknown
    /// extensions.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Some(FileFormat::Toml),
            Some("yaml" | "yml") => Some(FileFormat::Yaml),
            Some("json") => Some(FileFormat::Json),
            _ => None,
        }
    }
}

/// Reads configuration from a file on disk.
///
/// The `reader` closure is injectable so tests can supply in-memory content
/// instead of touching the file system.
pub struct FileSource<R>
where
    R: Fn(&Path) -> Result<String, std::io::Error>,
{
    path: PathBuf,
    format: FileFormat,
    required: bool,
    reader: R,
}

impl FileSource<fn(&Path) -> Result<String, std::io::Error>> {
    /// Create a file source that reads from the real filesystem.
    /// If `required` is false, a missing file silently returns an empty table.
    pub fn new(path: impl Into<PathBuf>, format: FileFormat, required: bool) -> Self {
        Self {
            path: path.into(),
            format,
            required,
            reader: |p| std::fs::read_to_string(p),
        }
    }

    /// Auto-detect format from the file extension.
    pub fn auto(path: impl Into<PathBuf>, required: bool) -> Result<Self, ConfigError> {
        let path = path.into();
        let format = FileFormat::from_path(&path).ok_or_else(|| {
            ConfigError::ParseError(
                path.display().to_string(),
                "unable to detect file format from extension".into(),
            )
        })?;
        Ok(Self {
            path,
            format,
            required,
            reader: |p| std::fs::read_to_string(p),
        })
    }
}

impl<R> FileSource<R>
where
    R: Fn(&Path) -> Result<String, std::io::Error>,
{
    /// Constructor with a custom reader (useful for testing).
    pub fn with_reader(
        path: impl Into<PathBuf>,
        format: FileFormat,
        required: bool,
        reader: R,
    ) -> Self {
        Self {
            path: path.into(),
            format,
            required,
            reader,
        }
    }
}

impl<R> ConfigSource for FileSource<R>
where
    R: Fn(&Path) -> Result<String, std::io::Error>,
{
    fn name(&self) -> &str {
        "file"
    }

    fn load(&self) -> Result<ConfigValue, ConfigError> {
        let content = match (self.reader)(&self.path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !self.required => {
                return Ok(ConfigValue::empty_table());
            }
            Err(e) => {
                return Err(ConfigError::IoError(self.path.display().to_string(), e));
            }
        };

        let display_path = self.path.display().to_string();
        parse_content(&content, self.format, &display_path)
    }
}

// ---------------------------------------------------------------------------
// Parsers: string → ConfigValue
// ---------------------------------------------------------------------------

fn parse_content(
    content: &str,
    format: FileFormat,
    source_name: &str,
) -> Result<ConfigValue, ConfigError> {
    match format {
        FileFormat::Toml => {
            let table: toml::Value = toml::from_str(content).map_err(|e| {
                ConfigError::ParseError(source_name.to_string(), e.to_string())
            })?;
            Ok(toml_to_config_value(&table))
        }
        FileFormat::Yaml => {
            let yaml: serde_yaml::Value =
                serde_yaml::from_str(content).map_err(|e| {
                    ConfigError::ParseError(source_name.to_string(), e.to_string())
                })?;
            Ok(yaml_to_config_value(&yaml))
        }
        FileFormat::Json => {
            let json: serde_json::Value =
                serde_json::from_str(content).map_err(|e| {
                    ConfigError::ParseError(source_name.to_string(), e.to_string())
                })?;
            Ok(json_to_config_value(&json))
        }
    }
}

fn toml_to_config_value(val: &toml::Value) -> ConfigValue {
    match val {
        toml::Value::String(s) => ConfigValue::String(s.clone()),
        toml::Value::Integer(n) => ConfigValue::Integer(*n),
        toml::Value::Float(f) => ConfigValue::Float(*f),
        toml::Value::Boolean(b) => ConfigValue::Bool(*b),
        toml::Value::Array(arr) => {
            ConfigValue::Array(arr.iter().map(toml_to_config_value).collect())
        }
        toml::Value::Table(tbl) => {
            let map = tbl
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_config_value(v)))
                .collect();
            ConfigValue::Table(map)
        }
        toml::Value::Datetime(dt) => ConfigValue::String(dt.to_string()),
    }
}

fn yaml_to_config_value(val: &serde_yaml::Value) -> ConfigValue {
    match val {
        serde_yaml::Value::String(s) => ConfigValue::String(s.clone()),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ConfigValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                ConfigValue::Float(f)
            } else {
                ConfigValue::String(n.to_string())
            }
        }
        serde_yaml::Value::Bool(b) => ConfigValue::Bool(*b),
        serde_yaml::Value::Sequence(arr) => {
            ConfigValue::Array(arr.iter().map(yaml_to_config_value).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let m = map
                .iter()
                .filter_map(|(k, v)| {
                    k.as_str()
                        .map(|ks| (ks.to_string(), yaml_to_config_value(v)))
                })
                .collect();
            ConfigValue::Table(m)
        }
        serde_yaml::Value::Null => ConfigValue::Null,
        serde_yaml::Value::Tagged(tagged) => yaml_to_config_value(&tagged.value),
    }
}

fn json_to_config_value(val: &serde_json::Value) -> ConfigValue {
    match val {
        serde_json::Value::String(s) => ConfigValue::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ConfigValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                ConfigValue::Float(f)
            } else {
                ConfigValue::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => ConfigValue::Bool(*b),
        serde_json::Value::Array(arr) => {
            ConfigValue::Array(arr.iter().map(json_to_config_value).collect())
        }
        serde_json::Value::Object(map) => {
            let m = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_config_value(v)))
                .collect();
            ConfigValue::Table(m)
        }
        serde_json::Value::Null => ConfigValue::Null,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── DefaultSource ───────────────────────────────────────────────────

    #[test]
    fn test_default_source() {
        let src = DefaultSource::from_map(HashMap::from([
            ("host", ConfigValue::String("localhost".into())),
            ("port", ConfigValue::Integer(3000)),
        ]));
        let val = src.load().expect("load defaults");
        assert_eq!(
            val.get_path("host"),
            Some(&ConfigValue::String("localhost".into()))
        );
        assert_eq!(val.get_path("port"), Some(&ConfigValue::Integer(3000)));
    }

    // ── EnvSource ───────────────────────────────────────────────────────

    #[test]
    fn test_env_source_basic() {
        let fake_env = || {
            vec![
                ("APP_SERVER__HOST".into(), "0.0.0.0".into()),
                ("APP_SERVER__PORT".into(), "9090".into()),
                ("APP_DEBUG".into(), "true".into()),
                ("UNRELATED_VAR".into(), "ignored".into()),
            ]
        };
        let src = EnvSource::new("APP", fake_env);
        let val = src.load().expect("load env");

        assert_eq!(
            val.get_path("server.host"),
            Some(&ConfigValue::String("0.0.0.0".into()))
        );
        assert_eq!(
            val.get_path("server.port"),
            Some(&ConfigValue::Integer(9090))
        );
        assert_eq!(val.get_path("debug"), Some(&ConfigValue::Bool(true)));
        // unrelated var should not appear
        assert_eq!(val.get_path("unrelated_var"), None);
    }

    // ── FileSource (TOML) ───────────────────────────────────────────────

    #[test]
    fn test_file_source_toml() {
        let toml_content = r#"
[server]
host = "127.0.0.1"
port = 8080

[database]
url = "postgres://localhost/mydb"
"#;

        let src = FileSource::with_reader(
            "config.toml",
            FileFormat::Toml,
            true,
            |_| Ok(toml_content.to_string()),
        );
        let val = src.load().expect("parse toml");
        assert_eq!(
            val.get_path("server.host"),
            Some(&ConfigValue::String("127.0.0.1".into()))
        );
        assert_eq!(
            val.get_path("server.port"),
            Some(&ConfigValue::Integer(8080))
        );
    }

    // ── FileSource (JSON) ───────────────────────────────────────────────

    #[test]
    fn test_file_source_json() {
        let json_content = r#"{"server": {"host": "0.0.0.0", "port": 3000}}"#;
        let src = FileSource::with_reader(
            "config.json",
            FileFormat::Json,
            true,
            |_| Ok(json_content.to_string()),
        );
        let val = src.load().expect("parse json");
        assert_eq!(
            val.get_path("server.port"),
            Some(&ConfigValue::Integer(3000))
        );
    }

    // ── FileSource (YAML) ───────────────────────────────────────────────

    #[test]
    fn test_file_source_yaml() {
        let yaml_content = "server:\n  host: 10.0.0.1\n  port: 5000\n";
        let src = FileSource::with_reader(
            "config.yaml",
            FileFormat::Yaml,
            true,
            |_| Ok(yaml_content.to_string()),
        );
        let val = src.load().expect("parse yaml");
        assert_eq!(
            val.get_path("server.host"),
            Some(&ConfigValue::String("10.0.0.1".into()))
        );
        assert_eq!(
            val.get_path("server.port"),
            Some(&ConfigValue::Integer(5000))
        );
    }

    // ── FileSource optional missing file ────────────────────────────────

    #[test]
    fn test_file_source_optional_missing() {
        let src = FileSource::with_reader("absent.toml", FileFormat::Toml, false, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            ))
        });
        let val = src.load().expect("optional missing should succeed");
        assert_eq!(val, ConfigValue::empty_table());
    }

    // ── FileSource required missing file → error ────────────────────────

    #[test]
    fn test_file_source_required_missing() {
        let src = FileSource::with_reader("absent.toml", FileFormat::Toml, true, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            ))
        });
        assert!(src.load().is_err());
    }

    // ── FileFormat auto-detection ───────────────────────────────────────

    #[test]
    fn test_format_detection() {
        assert_eq!(
            FileFormat::from_path(Path::new("cfg.toml")),
            Some(FileFormat::Toml)
        );
        assert_eq!(
            FileFormat::from_path(Path::new("cfg.yaml")),
            Some(FileFormat::Yaml)
        );
        assert_eq!(
            FileFormat::from_path(Path::new("cfg.yml")),
            Some(FileFormat::Yaml)
        );
        assert_eq!(
            FileFormat::from_path(Path::new("cfg.json")),
            Some(FileFormat::Json)
        );
        assert_eq!(FileFormat::from_path(Path::new("cfg.xml")), None);
    }
}
