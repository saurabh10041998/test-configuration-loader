//! Example strongly-typed application configuration.
//!
//! This module shows how to model a realistic multi-concern config using
//! [`FromConfigValue`]. Each sub-section (http, auth, logging, etc.) is its
//! own struct with its own `FromConfigValue` impl, composed into a top-level
//! [`AppConfig`].
//!
//! None of this is framework code — it lives in the *consumer's* crate. The
//! library only provides [`ConfigValue`], [`FromConfigValue`], and friends.

use unified_config_loader::error::ConfigError;
use unified_config_loader::traits::{ConfigValue, FromConfigValue};

// ───────────────────────────────────────────────────────────────────────────
// Top-level application config
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct AppConfig {
    pub http: HttpConfig,
    pub auth: AuthConfig,
    pub logging: LoggingConfig,
    pub rate_limit: RateLimitConfig,
    pub mail: MailConfig,
    pub feature_flags: FeatureFlagsConfig,
    pub cors: CorsConfig,
    pub tls: Option<TlsConfig>,
}

impl FromConfigValue for AppConfig {
    fn from_config_value(root: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            http: extract_section(root, "http")?,
            auth: extract_section(root, "auth")?,
            logging: extract_section_or_default(root, "logging")?,
            rate_limit: extract_section_or_default(root, "rate_limit")?,
            mail: extract_section(root, "mail")?,
            feature_flags: extract_section_or_default(root, "feature_flags")?,
            cors: extract_section_or_default(root, "cors")?,
            tls: extract_optional_section(root, "tls")?,
        })
    }
}

// ───────────────────────────────────────────────────────────────────────────
// HTTP server settings
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct HttpConfig {
    pub host: String,
    pub port: i64,
    pub workers: i64,
    pub request_timeout_secs: i64,
    pub max_body_size_bytes: i64,
}

impl FromConfigValue for HttpConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            host: get_str(val, "host", "http.host")?,
            port: get_i64(val, "port", "http.port")?,
            workers: get_i64_or(val, "workers", 4),
            request_timeout_secs: get_i64_or(val, "request_timeout_secs", 30),
            max_body_size_bytes: get_i64_or(val, "max_body_size_bytes", 10_485_760), // 10 MiB
        })
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Authentication / JWT
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_issuer: String,
    pub token_expiry_secs: i64,
    pub refresh_token_expiry_secs: i64,
    pub bcrypt_cost: i64,
}

impl FromConfigValue for AuthConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            jwt_secret: get_str(val, "jwt_secret", "auth.jwt_secret")?,
            jwt_issuer: get_str_or(val, "jwt_issuer", "unified-app"),
            token_expiry_secs: get_i64_or(val, "token_expiry_secs", 3600),
            refresh_token_expiry_secs: get_i64_or(val, "refresh_token_expiry_secs", 604_800),
            bcrypt_cost: get_i64_or(val, "bcrypt_cost", 12),
        })
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Logging
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String, // "json" | "pretty"
    pub output: String, // "stdout" | "file"
    pub file_path: Option<String>,
}

impl FromConfigValue for LoggingConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            level: get_str_or(val, "level", "info"),
            format: get_str_or(val, "format", "json"),
            output: get_str_or(val, "output", "stdout"),
            file_path: val
                .get_path("file_path")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            format: "json".into(),
            output: "stdout".into(),
            file_path: None,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Rate limiting
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: i64,
    pub burst_size: i64,
}

impl FromConfigValue for RateLimitConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            enabled: get_bool_or(val, "enabled", true),
            requests_per_minute: get_i64_or(val, "requests_per_minute", 60),
            burst_size: get_i64_or(val, "burst_size", 10),
        })
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            burst_size: 10,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// SMTP / Mail
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct MailConfig {
    pub smtp_host: String,
    pub smtp_port: i64,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_address: String,
    pub use_tls: bool,
}

impl FromConfigValue for MailConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            smtp_host: get_str(val, "smtp_host", "mail.smtp_host")?,
            smtp_port: get_i64_or(val, "smtp_port", 587),
            smtp_username: get_str(val, "smtp_username", "mail.smtp_username")?,
            smtp_password: get_str(val, "smtp_password", "mail.smtp_password")?,
            from_address: get_str(val, "from_address", "mail.from_address")?,
            use_tls: get_bool_or(val, "use_tls", true),
        })
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Feature flags
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct FeatureFlagsConfig {
    pub enable_signup: bool,
    pub enable_oauth: bool,
    pub enable_api_v2: bool,
    pub maintenance_mode: bool,
}

impl FromConfigValue for FeatureFlagsConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            enable_signup: get_bool_or(val, "enable_signup", true),
            enable_oauth: get_bool_or(val, "enable_oauth", false),
            enable_api_v2: get_bool_or(val, "enable_api_v2", false),
            maintenance_mode: get_bool_or(val, "maintenance_mode", false),
        })
    }
}

impl Default for FeatureFlagsConfig {
    fn default() -> Self {
        Self {
            enable_signup: true,
            enable_oauth: false,
            enable_api_v2: false,
            maintenance_mode: false,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// CORS
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allow_credentials: bool,
    pub max_age_secs: i64,
}

impl FromConfigValue for CorsConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        let allowed_origins = match val.get_path("allowed_origins") {
            Some(ConfigValue::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(ConfigValue::String(s)) => {
                // Accept comma-separated string from env vars
                s.split(',').map(|o| o.trim().to_string()).collect()
            }
            _ => vec!["*".to_string()],
        };
        Ok(Self {
            allowed_origins,
            allow_credentials: get_bool_or(val, "allow_credentials", false),
            max_age_secs: get_i64_or(val, "max_age_secs", 86400),
        })
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allow_credentials: false,
            max_age_secs: 86400,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// TLS (optional)
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: Option<String>,
}

impl FromConfigValue for TlsConfig {
    fn from_config_value(val: &ConfigValue) -> Result<Self, ConfigError> {
        Ok(Self {
            cert_path: get_str(val, "cert_path", "tls.cert_path")?,
            key_path: get_str(val, "key_path", "tls.key_path")?,
            ca_path: val
                .get_path("ca_path")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Helper extractors (reduce boilerplate in FromConfigValue impls)
// ───────────────────────────────────────────────────────────────────────────

fn get_str(val: &ConfigValue, key: &str, full_path: &str) -> Result<String, ConfigError> {
    val.get_path(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ConfigError::MissingField(full_path.into()))
}

fn get_str_or(val: &ConfigValue, key: &str, default: &str) -> String {
    val.get_path(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| default.to_string())
}

fn get_i64(val: &ConfigValue, key: &str, full_path: &str) -> Result<i64, ConfigError> {
    val.get_path(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ConfigError::MissingField(full_path.into()))
}

fn get_i64_or(val: &ConfigValue, key: &str, default: i64) -> i64 {
    val.get_path(key)
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
}

fn get_bool_or(val: &ConfigValue, key: &str, default: bool) -> bool {
    val.get_path(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

/// Extract a required sub-section and deserialize it.
fn extract_section<T: FromConfigValue>(
    root: &ConfigValue,
    section: &str,
) -> Result<T, ConfigError> {
    let sub = root
        .get_path(section)
        .ok_or_else(|| ConfigError::MissingField(section.into()))?;
    T::from_config_value(sub)
}

/// Extract an optional sub-section, falling back to `T::default()` if absent.
fn extract_section_or_default<T: FromConfigValue + Default>(
    root: &ConfigValue,
    section: &str,
) -> Result<T, ConfigError> {
    match root.get_path(section) {
        Some(sub) => T::from_config_value(sub),
        None => Ok(T::default()),
    }
}

/// Extract a truly optional sub-section (returns `None` if absent).
fn extract_optional_section<T: FromConfigValue>(
    root: &ConfigValue,
    section: &str,
) -> Result<Option<T>, ConfigError> {
    match root.get_path(section) {
        Some(sub) => T::from_config_value(sub).map(Some),
        None => Ok(None),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Unit tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Build a minimal valid config tree for the full AppConfig.
    fn minimal_valid_tree() -> ConfigValue {
        ConfigValue::Table(HashMap::from([
            (
                "http".into(),
                ConfigValue::Table(HashMap::from([
                    ("host".into(), ConfigValue::String("0.0.0.0".into())),
                    ("port".into(), ConfigValue::Integer(8080)),
                ])),
            ),
            (
                "auth".into(),
                ConfigValue::Table(HashMap::from([
                    (
                        "jwt_secret".into(),
                        ConfigValue::String("super-secret-key-256-bit".into()),
                    ),
                ])),
            ),
            (
                "mail".into(),
                ConfigValue::Table(HashMap::from([
                    (
                        "smtp_host".into(),
                        ConfigValue::String("smtp.example.com".into()),
                    ),
                    (
                        "smtp_username".into(),
                        ConfigValue::String("noreply@example.com".into()),
                    ),
                    (
                        "smtp_password".into(),
                        ConfigValue::String("mail-pass".into()),
                    ),
                    (
                        "from_address".into(),
                        ConfigValue::String("noreply@example.com".into()),
                    ),
                ])),
            ),
        ]))
    }

    #[test]
    fn test_full_config_from_minimal_tree() {
        let config = AppConfig::from_config_value(&minimal_valid_tree())
            .expect("should parse minimal config");

        // Required fields present
        assert_eq!(config.http.host, "0.0.0.0");
        assert_eq!(config.http.port, 8080);
        assert_eq!(config.auth.jwt_secret, "super-secret-key-256-bit");

        // Defaults kick in
        assert_eq!(config.http.workers, 4);
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.rate_limit.requests_per_minute, 60);
        assert!(config.feature_flags.enable_signup);
        assert!(!config.feature_flags.maintenance_mode);
        assert_eq!(config.cors.allowed_origins, vec!["*".to_string()]);
        assert!(config.tls.is_none());
    }

    #[test]
    fn test_missing_required_http_host() {
        let tree = ConfigValue::Table(HashMap::from([(
            "http".into(),
            ConfigValue::Table(HashMap::from([(
                "port".into(),
                ConfigValue::Integer(8080),
            )])),
        )]));

        let result = AppConfig::from_config_value(&tree);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_auth_section_entirely() {
        let tree = ConfigValue::Table(HashMap::from([(
            "http".into(),
            ConfigValue::Table(HashMap::from([
                ("host".into(), ConfigValue::String("localhost".into())),
                ("port".into(), ConfigValue::Integer(3000)),
            ])),
        )]));

        let result = AppConfig::from_config_value(&tree);
        assert!(result.is_err()); // auth is required
    }

    #[test]
    fn test_tls_section_parsed_when_present() {
        let mut tree = minimal_valid_tree();
        if let ConfigValue::Table(ref mut map) = tree {
            map.insert(
                "tls".into(),
                ConfigValue::Table(HashMap::from([
                    (
                        "cert_path".into(),
                        ConfigValue::String("/etc/certs/app.pem".into()),
                    ),
                    (
                        "key_path".into(),
                        ConfigValue::String("/etc/certs/app.key".into()),
                    ),
                ])),
            );
        }

        let config = AppConfig::from_config_value(&tree).expect("parse with tls");
        let tls = config.tls.expect("tls should be Some");
        assert_eq!(tls.cert_path, "/etc/certs/app.pem");
        assert_eq!(tls.key_path, "/etc/certs/app.key");
        assert!(tls.ca_path.is_none());
    }

    #[test]
    fn test_cors_comma_separated_string() {
        let cors_val = ConfigValue::Table(HashMap::from([(
            "allowed_origins".into(),
            ConfigValue::String("https://a.com, https://b.com".into()),
        )]));

        let cors = CorsConfig::from_config_value(&cors_val).expect("parse cors");
        assert_eq!(cors.allowed_origins, vec!["https://a.com", "https://b.com"]);
    }

    #[test]
    fn test_cors_array_origins() {
        let cors_val = ConfigValue::Table(HashMap::from([(
            "allowed_origins".into(),
            ConfigValue::Array(vec![
                ConfigValue::String("https://x.com".into()),
                ConfigValue::String("https://y.com".into()),
            ]),
        )]));

        let cors = CorsConfig::from_config_value(&cors_val).expect("parse cors");
        assert_eq!(cors.allowed_origins, vec!["https://x.com", "https://y.com"]);
    }

    #[test]
    fn test_feature_flags_defaults() {
        let empty = ConfigValue::empty_table();
        let ff = FeatureFlagsConfig::from_config_value(&empty).expect("defaults");
        assert!(ff.enable_signup);
        assert!(!ff.enable_oauth);
        assert!(!ff.enable_api_v2);
        assert!(!ff.maintenance_mode);
    }

    #[test]
    fn test_logging_file_output() {
        let val = ConfigValue::Table(HashMap::from([
            ("level".into(), ConfigValue::String("debug".into())),
            ("format".into(), ConfigValue::String("pretty".into())),
            ("output".into(), ConfigValue::String("file".into())),
            (
                "file_path".into(),
                ConfigValue::String("/var/log/app.log".into()),
            ),
        ]));

        let log = LoggingConfig::from_config_value(&val).expect("parse logging");
        assert_eq!(log.level, "debug");
        assert_eq!(log.format, "pretty");
        assert_eq!(log.output, "file");
        assert_eq!(log.file_path.as_deref(), Some("/var/log/app.log"));
    }

    #[test]
    fn test_rate_limit_overrides() {
        let val = ConfigValue::Table(HashMap::from([
            ("enabled".into(), ConfigValue::Bool(false)),
            ("requests_per_minute".into(), ConfigValue::Integer(1000)),
            ("burst_size".into(), ConfigValue::Integer(50)),
        ]));

        let rl = RateLimitConfig::from_config_value(&val).expect("parse rate_limit");
        assert!(!rl.enabled);
        assert_eq!(rl.requests_per_minute, 1000);
        assert_eq!(rl.burst_size, 50);
    }
}
