use crate::error::ConfigError;
use crate::traits::{ConfigValue, FromConfigValue};

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
            max_body_size_bytes: get_i64_or(val, "max_body_size_bytes", 10_485_760),
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
    pub format: String,
    pub output: String,
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
            Some(ConfigValue::String(s)) => s.split(',').map(|o| o.trim().to_string()).collect(),
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
// Helper extractors
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

fn extract_section<T: FromConfigValue>(
    root: &ConfigValue,
    section: &str,
) -> Result<T, ConfigError> {
    let sub = root
        .get_path(section)
        .ok_or_else(|| ConfigError::MissingField(section.into()))?;
    T::from_config_value(sub)
}

fn extract_section_or_default<T: FromConfigValue + Default>(
    root: &ConfigValue,
    section: &str,
) -> Result<T, ConfigError> {
    match root.get_path(section) {
        Some(sub) => T::from_config_value(sub),
        None => Ok(T::default()),
    }
}

fn extract_optional_section<T: FromConfigValue>(
    root: &ConfigValue,
    section: &str,
) -> Result<Option<T>, ConfigError> {
    match root.get_path(section) {
        Some(sub) => T::from_config_value(sub).map(Some),
        None => Ok(None),
    }
}
