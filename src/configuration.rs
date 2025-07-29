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

fn deserialize_secret<'de, D>(deserializer: D) -> Result<Secret<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Secret::new(s))
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    dotenvy::dotenv().ok();

    let environment = get_environment()?;
    build_configuration(environment)
}

fn get_environment() -> Result<Environment, config::ConfigError> {
    let env_var = env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".to_string());

    env_var
        .try_into()
        .map_err(|_| config::ConfigError::NotFound("Failed to parse APP_ENVIRONMENT".to_string()))
}

fn build_configuration(environment: Environment) -> Result<Settings, config::ConfigError> {
    match environment {
        Environment::Local => build_local_configuration(),
        Environment::Production => build_production_configuration(),
    }
}

fn build_local_configuration() -> Result<Settings, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::from_str(BASE_CONFIG, FileFormat::Yaml))
        .add_source(File::from_str(LOCAL_CONFIG, FileFormat::Yaml))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    config.try_deserialize::<Settings>()
}

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
