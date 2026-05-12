use std::collections::HashMap;
use unified_config_loader::app_config::AppConfig;
use unified_config_loader::prelude::*;
use unified_config_loader::sources::{DefaultSource, EnvSource, FileFormat, FileSource};
use unified_config_loader::validators::{FnValidator, RangeValidator, RequiredFieldsValidator};

// ───────────────────────────────────────────────────────────────────────────
// Helpers
// ───────────────────────────────────────────────────────────────────────────

/// Build a minimal valid ConfigValue tree that satisfies all required fields.
fn base_defaults() -> ConfigValue {
    ConfigValue::Table(HashMap::from([
        (
            "http".into(),
            ConfigValue::Table(HashMap::from([
                ("host".into(), ConfigValue::String("127.0.0.1".into())),
                ("port".into(), ConfigValue::Integer(3000)),
            ])),
        ),
        (
            "auth".into(),
            ConfigValue::Table(HashMap::from([(
                "jwt_secret".into(),
                ConfigValue::String("dev-secret-key-for-testing".into()),
            )])),
        ),
        (
            "mail".into(),
            ConfigValue::Table(HashMap::from([
                (
                    "smtp_host".into(),
                    ConfigValue::String("smtp.dev.local".into()),
                ),
                (
                    "smtp_username".into(),
                    ConfigValue::String("dev@test.com".into()),
                ),
                (
                    "smtp_password".into(),
                    ConfigValue::String("devpass".into()),
                ),
                (
                    "from_address".into(),
                    ConfigValue::String("dev@test.com".into()),
                ),
            ])),
        ),
    ]))
}

fn sample_toml() -> &'static str {
    r#"
[http]
host = "0.0.0.0"
port = 8080
workers = 8
request_timeout_secs = 60
max_body_size_bytes = 52428800

[auth]
jwt_secret = "toml-production-secret-key-256bit"
jwt_issuer = "my-saas-app"
token_expiry_secs = 1800
refresh_token_expiry_secs = 2592000
bcrypt_cost = 14

[logging]
level = "warn"
format = "json"
output = "file"
file_path = "/var/log/myapp/app.log"

[rate_limit]
enabled = true
requests_per_minute = 120
burst_size = 20

[mail]
smtp_host = "smtp.sendgrid.net"
smtp_port = 465
smtp_username = "apikey"
smtp_password = "SG.secret-key"
from_address = "hello@myapp.com"
use_tls = true

[feature_flags]
enable_signup = true
enable_oauth = true
enable_api_v2 = false
maintenance_mode = false

[cors]
allowed_origins = ["https://myapp.com", "https://admin.myapp.com"]
allow_credentials = true
max_age_secs = 7200

[tls]
cert_path = "/etc/letsencrypt/live/myapp.com/fullchain.pem"
key_path = "/etc/letsencrypt/live/myapp.com/privkey.pem"
"#
}

// ───────────────────────────────────────────────────────────────────────────
// 1. Full pipeline: defaults + TOML file + env overrides
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_full_pipeline_all_three_sources() {
    let defaults = DefaultSource::new(base_defaults());

    let file_source = FileSource::with_reader("config.toml", FileFormat::Toml, true, |_| {
        Ok(sample_toml().to_string())
    });

    // Env overrides: bump port, switch to debug logging, enable maintenance
    let env_source = EnvSource::new("APP", || {
        vec![
            ("APP_HTTP__PORT".into(), "9090".into()),
            ("APP_LOGGING__LEVEL".into(), "debug".into()),
            ("APP_FEATURE_FLAGS__MAINTENANCE_MODE".into(), "true".into()),
        ]
    });

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(defaults)) // lowest
        .add_source(Box::new(file_source)) // middle
        .add_source(Box::new(env_source)) // highest
        .load()
        .expect("full pipeline load");

    // Env wins for port
    assert_eq!(config.http.port, 9090);
    // File wins for host (env didn't set it)
    assert_eq!(config.http.host, "0.0.0.0");
    // File value preserved
    assert_eq!(config.http.workers, 8);
    // Env overrode logging level
    assert_eq!(config.logging.level, "debug");
    // Env enabled maintenance mode
    assert!(config.feature_flags.maintenance_mode);
    // File set oauth
    assert!(config.feature_flags.enable_oauth);
    // TLS from file
    let tls = config.tls.expect("tls should be present from file");
    assert!(tls.cert_path.contains("fullchain.pem"));
    // CORS from file
    assert_eq!(config.cors.allowed_origins.len(), 2);
    assert!(config.cors.allow_credentials);
    // Auth from file
    assert_eq!(config.auth.jwt_issuer, "my-saas-app");
    assert_eq!(config.auth.bcrypt_cost, 14);
    // Mail from file
    assert_eq!(config.mail.smtp_host, "smtp.sendgrid.net");
    assert_eq!(config.mail.smtp_port, 465);
}

// ───────────────────────────────────────────────────────────────────────────
// 2. Configurable precedence: file > env (reversed)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_reversed_precedence_file_wins_over_env() {
    let env_source = EnvSource::new("APP", || {
        vec![
            ("APP_HTTP__HOST".into(), "env-host".into()),
            ("APP_HTTP__PORT".into(), "1111".into()),
            ("APP_AUTH__JWT_SECRET".into(), "env-secret".into()),
            ("APP_MAIL__SMTP_HOST".into(), "env-smtp".into()),
            ("APP_MAIL__SMTP_USERNAME".into(), "env-user".into()),
            ("APP_MAIL__SMTP_PASSWORD".into(), "env-pass".into()),
            ("APP_MAIL__FROM_ADDRESS".into(), "env@test.com".into()),
        ]
    });

    let file_source = FileSource::with_reader("config.toml", FileFormat::Toml, true, |_| {
        Ok(r#"
[http]
host = "file-host"
port = 2222

[auth]
jwt_secret = "file-secret"

[mail]
smtp_host = "file-smtp"
smtp_username = "file-user"
smtp_password = "file-pass"
from_address = "file@test.com"
"#
        .to_string())
    });

    // Env added FIRST (lower), file SECOND (higher) → file wins
    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(env_source))
        .add_source(Box::new(file_source))
        .load()
        .expect("reversed precedence");

    assert_eq!(config.http.host, "file-host");
    assert_eq!(config.http.port, 2222);
    assert_eq!(config.auth.jwt_secret, "file-secret");
    assert_eq!(config.mail.smtp_host, "file-smtp");
}

// ───────────────────────────────────────────────────────────────────────────
// 3. Validation: required fields
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_required_fields_validator_catches_missing_jwt_secret() {
    let incomplete = DefaultSource::new(ConfigValue::Table(HashMap::from([(
        "http".into(),
        ConfigValue::Table(HashMap::from([
            ("host".into(), ConfigValue::String("localhost".into())),
            ("port".into(), ConfigValue::Integer(3000)),
        ])),
    )])));

    let result: Result<AppConfig, _> = ConfigLoader::new()
        .add_source(Box::new(incomplete))
        .add_validator(Box::new(RequiredFieldsValidator::new(vec![
            "auth.jwt_secret",
        ])))
        .load();

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("jwt_secret"));
}

// ───────────────────────────────────────────────────────────────────────────
// 4. Validation: port range
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_port_out_of_range_rejected() {
    let mut tree = base_defaults();
    if let ConfigValue::Table(ref mut m) = tree {
        m.insert(
            "http".into(),
            ConfigValue::Table(HashMap::from([
                ("host".into(), ConfigValue::String("localhost".into())),
                ("port".into(), ConfigValue::Integer(99999)),
            ])),
        );
    }

    let result: Result<AppConfig, _> = ConfigLoader::new()
        .add_source(Box::new(DefaultSource::new(tree)))
        .add_validator(Box::new(RangeValidator::new("http.port", 1, 65535)))
        .load();

    assert!(result.is_err());
}

// ───────────────────────────────────────────────────────────────────────────
// 5. Cross-field validation: TLS completeness
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_tls_missing_key_path_rejected() {
    let mut tree = base_defaults();
    if let ConfigValue::Table(ref mut m) = tree {
        m.insert(
            "tls".into(),
            ConfigValue::Table(HashMap::from([(
                "cert_path".into(),
                ConfigValue::String("/cert.pem".into()),
            )])),
        );
    }

    let tls_check = FnValidator::new(|cfg| {
        if cfg.get_path("tls").is_some() {
            if cfg.get_path("tls.cert_path").is_none() {
                return Err(ConfigError::MissingField("tls.cert_path".into()));
            }
            if cfg.get_path("tls.key_path").is_none() {
                return Err(ConfigError::MissingField("tls.key_path".into()));
            }
        }
        Ok(())
    });

    let result: Result<AppConfig, _> = ConfigLoader::new()
        .add_source(Box::new(DefaultSource::new(tree)))
        .add_validator(Box::new(tls_check))
        .load();

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("tls.key_path"));
}

// ───────────────────────────────────────────────────────────────────────────
// 6. Cross-field validation: log file output without file_path
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_log_file_output_requires_path() {
    let mut tree = base_defaults();
    if let ConfigValue::Table(ref mut m) = tree {
        m.insert(
            "logging".into(),
            ConfigValue::Table(HashMap::from([
                ("output".into(), ConfigValue::String("file".into())),
                // file_path intentionally omitted
            ])),
        );
    }

    let log_check = FnValidator::new(|cfg| {
        let output = cfg
            .get_path("logging.output")
            .and_then(|v| v.as_str())
            .unwrap_or("stdout");
        if output == "file" && cfg.get_path("logging.file_path").is_none() {
            return Err(ConfigError::ValidationFailed(
                "logging.file_path".into(),
                "required when output is 'file'".into(),
            ));
        }
        Ok(())
    });

    let result: Result<AppConfig, _> = ConfigLoader::new()
        .add_source(Box::new(DefaultSource::new(tree)))
        .add_validator(Box::new(log_check))
        .load();

    assert!(result.is_err());
}

// ───────────────────────────────────────────────────────────────────────────
// 7. Multiple validators, multiple errors collected
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_multiple_validation_errors_collected() {
    let bad_config = DefaultSource::new(ConfigValue::Table(HashMap::from([
        (
            "http".into(),
            ConfigValue::Table(HashMap::from([
                ("host".into(), ConfigValue::String("localhost".into())),
                ("port".into(), ConfigValue::Integer(99999)), // out of range
                ("workers".into(), ConfigValue::Integer(0)),  // out of range
            ])),
        ),
        (
            "auth".into(),
            ConfigValue::Table(HashMap::from([(
                "jwt_secret".into(),
                ConfigValue::String("s".into()),
            )])),
        ),
        (
            "mail".into(),
            ConfigValue::Table(HashMap::from([
                ("smtp_host".into(), ConfigValue::String("x".into())),
                ("smtp_username".into(), ConfigValue::String("x".into())),
                ("smtp_password".into(), ConfigValue::String("x".into())),
                ("from_address".into(), ConfigValue::String("x".into())),
            ])),
        ),
    ])));

    let result: Result<AppConfig, _> = ConfigLoader::new()
        .add_source(Box::new(bad_config))
        .add_validator(Box::new(RangeValidator::new("http.port", 1, 65535)))
        .add_validator(Box::new(RangeValidator::new("http.workers", 1, 256)))
        .load();

    match result {
        Err(ConfigError::Multiple(errs)) => {
            assert_eq!(errs.len(), 2);
        }
        other => panic!("expected Multiple with 2 errors, got {other:?}"),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// 8. YAML file source works end-to-end
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_yaml_source_full_config() {
    let yaml = r#"
http:
  host: "10.0.0.1"
  port: 443
  workers: 16
auth:
  jwt_secret: "yaml-secret-key"
  jwt_issuer: "yaml-app"
  token_expiry_secs: 900
mail:
  smtp_host: "mail.yaml.io"
  smtp_username: "yaml-user"
  smtp_password: "yaml-pass"
  from_address: "yaml@app.io"
  smtp_port: 25
  use_tls: false
feature_flags:
  enable_signup: false
  maintenance_mode: true
logging:
  level: "trace"
  format: "pretty"
cors:
  allowed_origins: "https://a.com,https://b.com"
  allow_credentials: true
"#;

    let src = FileSource::with_reader("config.yaml", FileFormat::Yaml, true, |_| {
        Ok(yaml.to_string())
    });

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(src))
        .load()
        .expect("yaml load");

    assert_eq!(config.http.host, "10.0.0.1");
    assert_eq!(config.http.port, 443);
    assert_eq!(config.http.workers, 16);
    assert_eq!(config.auth.jwt_issuer, "yaml-app");
    assert_eq!(config.auth.token_expiry_secs, 900);
    assert_eq!(config.mail.smtp_port, 25);
    assert!(!config.mail.use_tls);
    assert!(!config.feature_flags.enable_signup);
    assert!(config.feature_flags.maintenance_mode);
    assert_eq!(config.logging.level, "trace");
    // Comma-separated string auto-split
    assert_eq!(
        config.cors.allowed_origins,
        vec!["https://a.com", "https://b.com"]
    );
    assert!(config.cors.allow_credentials);
    // TLS not in yaml → None
    assert!(config.tls.is_none());
}

// ───────────────────────────────────────────────────────────────────────────
// 9. JSON file source works end-to-end
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_json_source_full_config() {
    let json = r#"{
  "http": { "host": "json-host", "port": 5555 },
  "auth": { "jwt_secret": "json-secret", "bcrypt_cost": 10 },
  "mail": {
    "smtp_host": "json-smtp",
    "smtp_username": "json-user",
    "smtp_password": "json-pass",
    "from_address": "json@test.com"
  },
  "rate_limit": { "enabled": false, "requests_per_minute": 999 }
}"#;

    let src = FileSource::with_reader("config.json", FileFormat::Json, true, |_| {
        Ok(json.to_string())
    });

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(src))
        .load()
        .expect("json load");

    assert_eq!(config.http.host, "json-host");
    assert_eq!(config.http.port, 5555);
    assert_eq!(config.auth.bcrypt_cost, 10);
    assert!(!config.rate_limit.enabled);
    assert_eq!(config.rate_limit.requests_per_minute, 999);
}

// ───────────────────────────────────────────────────────────────────────────
// 10. Custom plugin source
// ───────────────────────────────────────────────────────────────────────────

/// Simulates a remote key-value store (e.g. Consul, Vault).
struct RemoteKVSource {
    data: HashMap<String, ConfigValue>,
}

impl ConfigSource for RemoteKVSource {
    fn name(&self) -> &str {
        "remote-kv"
    }

    fn load(&self) -> Result<ConfigValue, ConfigError> {
        Ok(ConfigValue::Table(self.data.clone()))
    }
}

#[test]
fn test_custom_source_plugin_overrides_secret() {
    let defaults = DefaultSource::new(base_defaults());

    // Vault-like source that only provides the JWT secret
    let vault = RemoteKVSource {
        data: HashMap::from([(
            "auth".into(),
            ConfigValue::Table(HashMap::from([(
                "jwt_secret".into(),
                ConfigValue::String("vault-rotated-secret-2024".into()),
            )])),
        )]),
    };

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(defaults))
        .add_source(Box::new(vault)) // higher precedence
        .load()
        .expect("vault override");

    // Secret came from vault, everything else from defaults
    assert_eq!(config.auth.jwt_secret, "vault-rotated-secret-2024");
    assert_eq!(config.http.host, "127.0.0.1"); // still from defaults
}

// ───────────────────────────────────────────────────────────────────────────
// 11. Env-only: deeply nested keys via double-underscore
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_env_deep_nesting() {
    let env_source = EnvSource::new("APP", || {
        vec![
            ("APP_HTTP__HOST".into(), "env-host".into()),
            ("APP_HTTP__PORT".into(), "7777".into()),
            ("APP_HTTP__WORKERS".into(), "32".into()),
            ("APP_AUTH__JWT_SECRET".into(), "env-jwt".into()),
            ("APP_AUTH__BCRYPT_COST".into(), "16".into()),
            ("APP_MAIL__SMTP_HOST".into(), "env-smtp".into()),
            ("APP_MAIL__SMTP_USERNAME".into(), "eu".into()),
            ("APP_MAIL__SMTP_PASSWORD".into(), "ep".into()),
            ("APP_MAIL__FROM_ADDRESS".into(), "ef".into()),
            ("APP_MAIL__USE_TLS".into(), "false".into()),
            ("APP_LOGGING__LEVEL".into(), "error".into()),
            ("APP_RATE_LIMIT__ENABLED".into(), "false".into()),
            ("APP_RATE_LIMIT__REQUESTS_PER_MINUTE".into(), "500".into()),
            ("APP_FEATURE_FLAGS__ENABLE_OAUTH".into(), "true".into()),
            ("APP_FEATURE_FLAGS__ENABLE_API_V2".into(), "true".into()),
            ("APP_CORS__ALLOW_CREDENTIALS".into(), "true".into()),
        ]
    });

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(env_source))
        .load()
        .expect("env-only load");

    assert_eq!(config.http.host, "env-host");
    assert_eq!(config.http.port, 7777);
    assert_eq!(config.http.workers, 32);
    assert_eq!(config.auth.jwt_secret, "env-jwt");
    assert_eq!(config.auth.bcrypt_cost, 16);
    assert!(!config.mail.use_tls);
    assert_eq!(config.logging.level, "error");
    assert!(!config.rate_limit.enabled);
    assert_eq!(config.rate_limit.requests_per_minute, 500);
    assert!(config.feature_flags.enable_oauth);
    assert!(config.feature_flags.enable_api_v2);
    assert!(config.cors.allow_credentials);
}

// ───────────────────────────────────────────────────────────────────────────
// 12. Optional TLS: absent → None, present → Some
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_tls_absent_yields_none() {
    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(DefaultSource::new(base_defaults())))
        .load()
        .expect("no tls");

    assert!(config.tls.is_none());
}

#[test]
fn test_tls_present_yields_some() {
    let mut tree = base_defaults();
    if let ConfigValue::Table(ref mut m) = tree {
        m.insert(
            "tls".into(),
            ConfigValue::Table(HashMap::from([
                ("cert_path".into(), ConfigValue::String("/c.pem".into())),
                ("key_path".into(), ConfigValue::String("/k.pem".into())),
                ("ca_path".into(), ConfigValue::String("/ca.pem".into())),
            ])),
        );
    }

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(DefaultSource::new(tree)))
        .load()
        .expect("with tls");

    let tls = config.tls.expect("should be Some");
    assert_eq!(tls.cert_path, "/c.pem");
    assert_eq!(tls.key_path, "/k.pem");
    assert_eq!(tls.ca_path.as_deref(), Some("/ca.pem"));
}
