mod app_config;

use app_config::AppConfig;
use std::collections::HashMap;
use unified_config_loader::prelude::*;
use unified_config_loader::sources::{DefaultSource, EnvSource, FileFormat, FileSource};
use unified_config_loader::validators::{FnValidator, RangeValidator, RequiredFieldsValidator};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Layer 1 (lowest precedence): hardcoded defaults ─────────────────

    let defaults = DefaultSource::new(ConfigValue::Table(HashMap::from([
        (
            "http".into(),
            ConfigValue::Table(HashMap::from([
                ("host".into(), ConfigValue::String("127.0.0.1".into())),
                ("port".into(), ConfigValue::Integer(3000)),
                ("workers".into(), ConfigValue::Integer(4)),
                ("request_timeout_secs".into(), ConfigValue::Integer(30)),
                (
                    "max_body_size_bytes".into(),
                    ConfigValue::Integer(10_485_760),
                ),
            ])),
        ),
        (
            "auth".into(),
            ConfigValue::Table(HashMap::from([
                ("jwt_issuer".into(), ConfigValue::String("unified-app".into())),
                ("token_expiry_secs".into(), ConfigValue::Integer(3600)),
                (
                    "refresh_token_expiry_secs".into(),
                    ConfigValue::Integer(604_800),
                ),
                ("bcrypt_cost".into(), ConfigValue::Integer(12)),
            ])),
        ),
        (
            "logging".into(),
            ConfigValue::Table(HashMap::from([
                ("level".into(), ConfigValue::String("info".into())),
                ("format".into(), ConfigValue::String("json".into())),
                ("output".into(), ConfigValue::String("stdout".into())),
            ])),
        ),
        (
            "rate_limit".into(),
            ConfigValue::Table(HashMap::from([
                ("enabled".into(), ConfigValue::Bool(true)),
                ("requests_per_minute".into(), ConfigValue::Integer(60)),
                ("burst_size".into(), ConfigValue::Integer(10)),
            ])),
        ),
        (
            "feature_flags".into(),
            ConfigValue::Table(HashMap::from([
                ("enable_signup".into(), ConfigValue::Bool(true)),
                ("enable_oauth".into(), ConfigValue::Bool(false)),
                ("enable_api_v2".into(), ConfigValue::Bool(false)),
                ("maintenance_mode".into(), ConfigValue::Bool(false)),
            ])),
        ),
        (
            "cors".into(),
            ConfigValue::Table(HashMap::from([
                (
                    "allowed_origins".into(),
                    ConfigValue::Array(vec![ConfigValue::String("*".into())]),
                ),
                ("allow_credentials".into(), ConfigValue::Bool(false)),
                ("max_age_secs".into(), ConfigValue::Integer(86400)),
            ])),
        ),
    ])));

    // ── Layer 2: config file (optional) ─────────────────────────────────

    let file_source = FileSource::new("config.toml", FileFormat::Toml, false);

    // ── Layer 3 (highest precedence): environment variables ─────────────

    let env_source = EnvSource::from_env("APP");

    // ── Validators ──────────────────────────────────────────────────────

    let required = RequiredFieldsValidator::new(vec![
        "http.host",
        "http.port",
        "auth.jwt_secret",
        "mail.smtp_host",
        "mail.smtp_username",
        "mail.smtp_password",
        "mail.from_address",
    ]);

    let port_range = RangeValidator::new("http.port", 1, 65535);
    let smtp_port_range = RangeValidator::new("mail.smtp_port", 1, 65535);
    let bcrypt_range = RangeValidator::new("auth.bcrypt_cost", 4, 31);
    let workers_range = RangeValidator::new("http.workers", 1, 256);

    // Cross-field: if TLS section exists, cert and key paths must both be set
    let tls_completeness = FnValidator::new(|cfg| {
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

    // Cross-field: file logging requires a file_path
    let log_file_check = FnValidator::new(|cfg| {
        let output = cfg
            .get_path("logging.output")
            .and_then(|v| v.as_str())
            .unwrap_or("stdout");
        if output == "file" && cfg.get_path("logging.file_path").is_none() {
            return Err(ConfigError::ValidationFailed(
                "logging.file_path".into(),
                "file_path is required when logging output is 'file'".into(),
            ));
        }
        Ok(())
    });

    // ── Load ────────────────────────────────────────────────────────────

    let config: AppConfig = ConfigLoader::new()
        .add_source(Box::new(defaults))
        .add_source(Box::new(file_source))
        .add_source(Box::new(env_source))
        .add_validator(Box::new(required))
        .add_validator(Box::new(port_range))
        .add_validator(Box::new(smtp_port_range))
        .add_validator(Box::new(bcrypt_range))
        .add_validator(Box::new(workers_range))
        .add_validator(Box::new(tls_completeness))
        .add_validator(Box::new(log_file_check))
        .load()?;

    println!("Configuration loaded successfully:");
    println!();
    println!("[http]");
    println!("  bind       = {}:{}", config.http.host, config.http.port);
    println!("  workers    = {}", config.http.workers);
    println!("  timeout    = {}s", config.http.request_timeout_secs);
    println!();
    println!("[auth]");
    println!("  issuer     = {}", config.auth.jwt_issuer);
    println!("  token TTL  = {}s", config.auth.token_expiry_secs);
    println!("  bcrypt     = {}", config.auth.bcrypt_cost);
    println!();
    println!("[logging]");
    println!("  level      = {}", config.logging.level);
    println!("  format     = {}", config.logging.format);
    println!("  output     = {}", config.logging.output);
    println!();
    println!("[rate_limit]");
    println!("  enabled    = {}", config.rate_limit.enabled);
    println!("  rpm        = {}", config.rate_limit.requests_per_minute);
    println!();
    println!("[mail]");
    println!(
        "  smtp       = {}:{}",
        config.mail.smtp_host, config.mail.smtp_port
    );
    println!("  from       = {}", config.mail.from_address);
    println!("  tls        = {}", config.mail.use_tls);
    println!();
    println!("[feature_flags]");
    println!("  signup     = {}", config.feature_flags.enable_signup);
    println!("  oauth      = {}", config.feature_flags.enable_oauth);
    println!("  api_v2     = {}", config.feature_flags.enable_api_v2);
    println!("  maint      = {}", config.feature_flags.maintenance_mode);
    println!();
    println!("[cors]");
    println!("  origins    = {:?}", config.cors.allowed_origins);
    println!("  creds      = {}", config.cors.allow_credentials);
    println!();
    match &config.tls {
        Some(tls) => {
            println!("[tls]");
            println!("  cert       = {}", tls.cert_path);
            println!("  key        = {}", tls.key_path);
        }
        None => println!("[tls] not configured"),
    }

    Ok(())
}
