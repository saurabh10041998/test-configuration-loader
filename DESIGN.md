# Design Document — Unified Configuration Loader

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                        ConfigLoader                              │
│                                                                  │
│  sources: Vec<Box<dyn ConfigSource>>    (plugin list, ordered)   │
│  validators: Vec<Box<dyn ConfigValidator>>  (hook list)          │
│                                                                  │
│  load_raw() → merge all sources → ConfigValue tree               │
│  load::<T>() → merge → validate → FromConfigValue → T           │
└──────────────────────────────────────────────────────────────────┘

        ┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐
        │DefaultSource│   │ EnvSource  │   │ FileSource │   │ Your Plugin│
        └─────┬──────┘   └─────┬──────┘   └─────┬──────┘   └─────┬──────┘
              │                │                 │                 │
              └────────────────┴─────────────────┴─────────────────┘
                              ConfigValue (tree)
                                    │
                          ┌─────────▼──────────┐
                          │   Deep Merge       │
                          │ (last source wins) │
                          └─────────┬──────────┘
                                    │
                          ┌─────────▼──────────┐
                          │   Validators       │
                          │ (all run, errors   │
                          │  collected)        │
                          └─────────┬──────────┘
                                    │
                          ┌─────────▼──────────┐
                          │  FromConfigValue   │
                          │  → AppConfig       │
                          └────────────────────┘
```

## Application Config Domain

The sample application models eight configuration sections — deliberately
chosen to exercise every feature of the framework (required vs optional
sections, scalar types, arrays, cross-field dependencies, optional blocks):

| Section          | Required? | Purpose                                        | Key design aspect exercised        |
|------------------|-----------|-------------------------------------------------|------------------------------------|
| `http`           | Yes       | Bind address, port, workers, timeouts           | Required scalars + sensible defaults|
| `auth`           | Yes       | JWT secret/issuer, token TTLs, bcrypt cost      | Secrets from env/vault, range checks|
| `logging`        | No*       | Level, format, output target, file path         | Cross-field: output=file → need path|
| `rate_limit`     | No*       | Enable/disable, RPM, burst size                 | All-defaultable section            |
| `mail`           | Yes       | SMTP host/port/creds, from address, TLS toggle  | Multiple required string fields    |
| `feature_flags`  | No*       | Signup, OAuth, API v2, maintenance mode          | Boolean toggles, all default false |
| `cors`           | No*       | Allowed origins (array or CSV), creds, max-age  | Array **or** comma-separated string|
| `tls`            | No**      | Cert/key/CA paths                                | Truly optional (`Option<TlsConfig>`)|

\* Falls back to `Default::default()` if the entire section is absent.
\** Returns `None` if absent; if present, cert_path and key_path are required.

This gives coverage of: nested structs, optional sections, defaultable
sections, arrays, cross-field constraints, type coercion (env vars are strings,
parsed into int/bool), and secrets handling.

## How Configuration Sources Are Loaded and Merged

Every source implements the `ConfigSource` trait:

```rust
pub trait ConfigSource {
    fn name(&self) -> &str;
    fn load(&self) -> Result<ConfigValue, ConfigError>;
}
```

Each source produces a `ConfigValue::Table` — a recursive tree of string-keyed
values. The loader calls `.load()` on every registered source in order and
deep-merges the results. For nested tables, merge is recursive (keys from both
sides are preserved; conflicts at leaves are resolved by replacement). For
non-table values, the later value replaces the earlier one entirely.

### Built-in Sources

| Source          | Purpose                                                    |
|-----------------|------------------------------------------------------------|
| `DefaultSource` | Hardcoded fallback values built at construction time       |
| `EnvSource`     | Reads `{PREFIX}_KEY` vars; `__` maps to nesting            |
| `FileSource`    | Parses TOML, YAML, or JSON from disk (or injected reader)  |

### Plugin System — Adding a New Source

Adding a new source is a single trait implementation:

```rust
struct ConsulSource { endpoint: String }

impl ConfigSource for ConsulSource {
    fn name(&self) -> &str { "consul" }
    fn load(&self) -> Result<ConfigValue, ConfigError> {
        // fetch from Consul KV, convert to ConfigValue::Table
    }
}

// Register it — no framework code changes:
ConfigLoader::new()
    .add_source(Box::new(defaults))
    .add_source(Box::new(consul))
    .add_source(Box::new(env))
    .load::<AppConfig>()?;
```

Examples of custom sources you might write:

- **Vault / Secrets Manager** — load `auth.jwt_secret`, `mail.smtp_password`
  at startup without putting them in files
- **CLI arguments** — parse `--http.port=9090` flags
- **Remote HTTP** — `GET /config` from a config service
- **Database** — `SELECT key, value FROM settings`
- **Consul / etcd** — distributed KV store

## Precedence Rules

Precedence is **fully determined by insertion order**. Sources added later to
`ConfigLoader` have higher precedence. There is no hardcoded ordering.

Typical convention (lowest → highest):

1. `DefaultSource` — safe fallbacks
2. `FileSource` — deployment-specific overrides
3. `EnvSource` — runtime / container / 12-factor overrides

But the user can reverse this or interleave freely:

```rust
// File wins over env in this setup:
ConfigLoader::new()
    .add_source(Box::new(env))       // lowest
    .add_source(Box::new(file))      // highest
```

This is tested explicitly in `test_reversed_precedence_file_wins_over_env`.

## Error Modeling and Reporting Strategy

All errors flow through a single `ConfigError` enum:

| Variant            | When                                          |
|--------------------|-----------------------------------------------|
| `MissingField`     | Required field absent after merge              |
| `ValidationFailed` | A validator rejects a value (field + reason)   |
| `SourceError`      | A source's `.load()` failed                    |
| `IoError`          | File I/O failure (wraps `std::io::Error`)      |
| `ParseError`       | Deserialization failure (TOML/YAML/JSON)       |
| `Multiple`         | Collects several errors from validation phase  |

Key design choices:

- **No `unwrap()` / `expect()`** anywhere in library code.
- Validators run exhaustively — all errors are collected into
  `ConfigError::Multiple` so the user sees every problem at once (e.g. both
  "port out of range" and "workers out of range" in a single error).
- Every error variant carries enough context (field name, source name, reason)
  to produce an actionable diagnostic.

## Validation

Validation happens **after merge, before deserialization**. Validators
implement:

```rust
pub trait ConfigValidator {
    fn validate(&self, config: &ConfigValue) -> Result<(), ConfigError>;
}
```

### Built-in Validators

| Validator                 | Purpose                                   |
|---------------------------|-------------------------------------------|
| `RequiredFieldsValidator` | Asserts dotted paths are present & non-null|
| `RangeValidator`          | Checks a numeric field is within [min,max]|
| `FnValidator`             | Wraps any `Fn(&ConfigValue) → Result`     |

### Custom Validation Hooks — Examples in This Project

The integration tests demonstrate several real cross-field validators:

1. **TLS completeness** — if `tls` section exists, both `cert_path` and
   `key_path` must be present.
2. **Log file path** — if `logging.output == "file"`, then
   `logging.file_path` must be set.
3. **Port and workers ranges** — `http.port` in 1–65535, `http.workers` in
   1–256, `auth.bcrypt_cost` in 4–31.

Users implement `ConfigValidator` for arbitrarily complex logic. The
`FnValidator` wrapper enables closures for quick one-offs without a named
struct.

## Testability

The design is test-friendly by construction:

1. **`ConfigSource` is a trait** — tests use `DefaultSource` with in-memory
   trees. No filesystem, no environment needed.
2. **`EnvSource` accepts an injectable reader** — tests pass a fake
   `Vec<(String,String)>`.
3. **`FileSource` accepts an injectable reader** — tests pass in-memory TOML/
   YAML/JSON strings.
4. **`ConfigValidator` is a trait** — tests add or omit validators freely.
5. **`FromConfigValue` is a trait** — tests can define minimal test-only
   config structs.
6. **No global mutable state** — every `ConfigLoader` is self-contained.

## Trade-offs

| Decision                              | Upside                                | Downside                                    |
|---------------------------------------|---------------------------------------|---------------------------------------------|
| `ConfigValue` intermediate repr       | Sources fully decoupled from app type | Extra conversion step; runtime type checks  |
| Manual `FromConfigValue` impl         | Full control over missing/default     | More boilerplate than derive macro          |
| Validators run on untyped tree        | Cross-field checks across sections    | Can't leverage Rust's type system in rules  |
| Precedence by insertion order         | Maximum flexibility                   | Easy to mis-order if not careful            |
| All validators run (collect errors)   | User sees every problem at once       | Slightly more complex error handling        |
| Section-level Default trait           | Optional sections "just work"         | Default must be sensible (no silent junk)   |
| `Option<TlsConfig>` for truly optional| Clear None-vs-misconfigured semantics | Cross-field validation must be external     |

---

## Bonus Features — Hints for Incorporation

### 1. Multiple File Formats (TOML, YAML, JSON) ✅ Already Implemented

`FileSource` supports all three via the `FileFormat` enum and
`FileFormat::from_path()` auto-detection. Adding a new format (e.g. INI, HCL)
means adding a variant to `FileFormat` and a `*_to_config_value` converter —
no loader changes needed.

### 2. Hot-Reload Support

Two approaches fit naturally:

**A. Polling reload (simpler):**

```rust
pub struct ReloadableConfig<T> {
    loader: ConfigLoader,
    current: Arc<RwLock<T>>,
}

impl<T: FromConfigValue + Send + Sync> ReloadableConfig<T> {
    pub fn start(loader: ConfigLoader, interval: Duration) -> Self { /* ... */ }
    pub fn get(&self) -> RwLockReadGuard<T> { /* ... */ }
}
```

**B. File-watcher (reactive):** use the `notify` crate to watch the config
file. On change, re-run the full pipeline. Validators run on reload too, so
an invalid change is rejected and the old config remains.

The loader is already stateless, so reload = call `load()` again. For
sections like `feature_flags` this is especially valuable — toggle
`maintenance_mode` by editing the TOML without restarting.

### 3. Partial Configuration Validation

Track which keys each source contributed (diff before/after merge) and pass
that set to partial validators:

```rust
pub trait PartialValidator {
    fn validate_partial(
        &self,
        config: &ConfigValue,
        changed_paths: &[String],
    ) -> Result<(), ConfigError>;
}
```

This pairs well with hot-reload — only validate what changed. Example: if an
env override only touched `rate_limit.requests_per_minute`, skip the TLS
completeness check.

### 4. Schema / Documentation Generation

A derive macro on the config struct could emit both `FromConfigValue` and a
`SchemaInfo` metadata object:

```rust
#[derive(ConfigSchema)]
struct HttpConfig {
    /// Bind address
    #[config(default = "127.0.0.1", env = "APP_HTTP__HOST")]
    host: String,

    /// Port (1–65535)
    #[config(required, range(1, 65535))]
    port: u16,
}
```

This schema can render to Markdown reference docs, JSON Schema, or
`--help` output. The trait-based architecture means schema generation
is additive — existing code doesn't change.
