use config::{Config, File, FileFormat};
use secrecy::Secret;
use serde::Deserialize;
use std::env;

const BASE_CONFIG: &str = include_str!("../configuration/base.yaml");
const LOCAL_CONFIG: &str = include_str!("../configuration/local.yaml");
const PRODUCTION_CONFIG: &str = include_str!("../configuration/production.yaml");

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub cors: CorsSettings,
    pub application: ApplicationSettings,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApplicationSettings {
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmailClientSettings {
    pub base_url: String,
    pub sender_email: String,
    #[serde(deserialize_with = "deserialize_secret")]
    pub authorization_token: Secret<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CorsSettings {
    pub allowed_origins: Vec<String>,
}

/// Deserializes a string into a `Secret<String>`.
///
/// This function is intended for use with Serde to securely handle sensitive string values during deserialization.
///
/// # Examples
///
/// ```
/// use secrecy::Secret;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(deserialize_with = "deserialize_secret")]
///     secret_value: Secret<String>,
/// }
///
/// let yaml = "secret_value: supersecret";
/// let config: Config = serde_yaml::from_str(yaml).unwrap();
/// assert_eq!(config.secret_value.expose_secret(), "supersecret");
/// ```
fn deserialize_secret<'de, D>(deserializer: D) -> Result<Secret<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Secret::new(s))
}

/// Loads application configuration by merging YAML files and environment variables.
///
/// Loads environment variables from a `.env` file if present, determines the current environment from `APP_ENVIRONMENT`, and builds the configuration by layering base and environment-specific YAML with environment variable overrides.
///
/// # Returns
///
/// A `Settings` struct containing the merged configuration, or a `config::ConfigError` if loading fails.
///
/// # Examples
///
/// ```
/// let settings = get_configuration().expect("Failed to load configuration");
/// assert!(settings.application.port > 0);
/// ```
pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    dotenvy::dotenv().ok();

    let environment = get_environment()?;
    build_configuration(environment)
}

/// Determines the current application environment from the `APP_ENVIRONMENT` environment variable.
///
/// Returns the corresponding `Environment` variant, defaulting to `Local` if the variable is unset.
/// Returns an error if the environment value is invalid.
fn get_environment() -> Result<Environment, config::ConfigError> {
    let env_var = env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".to_string());

    env_var
        .try_into()
        .map_err(|_| config::ConfigError::NotFound("Failed to parse APP_ENVIRONMENT".to_string()))
}

/// Builds the application configuration for the specified environment.
///
/// Selects the appropriate configuration sources based on the environment and returns the resulting settings.
///
/// # Examples
///
/// ```
/// let settings = build_configuration(Environment::Local).unwrap();
/// assert_eq!(settings.application.port, 8000);
/// ```
fn build_configuration(environment: Environment) -> Result<Settings, config::ConfigError> {
    match environment {
        Environment::Local => build_local_configuration(),
        Environment::Production => build_production_configuration(),
    }
}

/// Builds the application configuration for the local environment.
///
/// Loads configuration by layering the embedded base YAML, local YAML, and environment variables prefixed with `APP`. Environment variables override YAML values when present.
///
/// # Returns
/// A `Settings` struct populated with the merged configuration, or a `config::ConfigError` if loading or deserialization fails.
///
/// # Examples
///
/// ```
/// let settings = build_local_configuration().unwrap();
/// assert_eq!(settings.application.port, 8000);
/// ```
fn build_local_configuration() -> Result<Settings, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::from_str(BASE_CONFIG, FileFormat::Yaml))
        .add_source(File::from_str(LOCAL_CONFIG, FileFormat::Yaml))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    config.try_deserialize::<Settings>()
}

/// Builds the application configuration for the production environment.
///
/// Loads the base and production YAML configuration layers, then applies environment variable overrides with the `APP` prefix. Returns the resulting `Settings` struct or a configuration error if loading or deserialization fails.
///
/// # Returns
/// A `Settings` instance containing the merged production configuration, or a `config::ConfigError` on failure.
///
/// # Examples
///
/// ```
/// let settings = build_production_configuration().unwrap();
/// assert_eq!(settings.application.port, 8000);
/// ```
fn build_production_configuration() -> Result<Settings, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::from_str(BASE_CONFIG, FileFormat::Yaml))
        .add_source(File::from_str(PRODUCTION_CONFIG, FileFormat::Yaml))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    config.try_deserialize::<Settings>()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{other} is not a supported environment. Use either `local` or `production`."
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_as_str() {
        assert_eq!(Environment::Local.as_str(), "local");
        assert_eq!(Environment::Production.as_str(), "production");
    }

    #[test]
    fn test_try_from_string() {
        assert_eq!(
            Environment::try_from("local".to_string()),
            Ok(Environment::Local)
        );
        assert_eq!(
            Environment::try_from("production".to_string()),
            Ok(Environment::Production)
        );
        assert_eq!(
            Environment::try_from("invalid".to_string()),
            Err(
                "invalid is not a supported environment. Use either `local` or `production`."
                    .to_string()
            )
        );
    }

    #[test]
    fn test_get_local_configuration() -> Result<(), config::ConfigError> {
        let settings = get_configuration()?;
        assert_eq!(
            settings.cors.allowed_origins,
            vec![
                "http://localhost:3000",
                "https://queens.ac",
                "https://www.queens.ac"
            ]
        );
        assert_eq!(settings.application.port, 8080);
        Ok(())
    }
}
