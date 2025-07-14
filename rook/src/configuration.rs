use crate::domain::SubscriberEmail;
use config::{Config, File, FileFormat};
use secrecy::Secret;
use serde::Deserialize;
use shuttle_runtime::SecretStore;
use std::time::Duration;

const BASE_CONFIG: &str = include_str!("../configuration/base.yaml");
const LOCAL_CONFIG: &str = include_str!("../configuration/local.yaml");
const PRODUCTION_CONFIG: &str = include_str!("../configuration/production.yaml");

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub email_client: EmailClientSettings,
    pub cors: CorsSettings,
    pub repository_checker: RepositoryCheckerSettings,
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

#[derive(Debug, Deserialize, Clone)]
pub struct RepositoryCheckerSettings {
    pub interval_seconds: u64,
}

fn deserialize_secret<'de, D>(deserializer: D) -> Result<Secret<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Secret::new(s))
}

impl EmailClientSettings {
    pub fn sender(&self) -> Result<SubscriberEmail, String> {
        SubscriberEmail::new(self.sender_email.clone())
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }
}

pub fn get_configuration_with_secrets(
    secrets: Option<&SecretStore>,
) -> Result<Settings, config::ConfigError> {
    dotenvy::dotenv().ok();
    let secrets =
        secrets.ok_or_else(|| config::ConfigError::NotFound("Secrets not provided".to_string()))?;

    let environment: Environment = secrets
        .get("APP_ENVIRONMENT")
        .ok_or_else(|| {
            config::ConfigError::NotFound("APP_ENVIRONMENT not found in secrets".to_string())
        })?
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");

    match environment {
        Environment::Local => get_local_configuration(),
        Environment::Production => get_production_configuration(secrets),
    }
}

fn get_local_configuration() -> Result<Settings, config::ConfigError> {
    let settings = Config::builder()
        .add_source(File::from_str(BASE_CONFIG, FileFormat::Yaml))
        .add_source(File::from_str(LOCAL_CONFIG, FileFormat::Yaml))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    settings.try_deserialize::<Settings>()
}

fn get_production_configuration(secrets: &SecretStore) -> Result<Settings, config::ConfigError> {
    let sender_email = secrets.get("POSTMARK_SENDER_EMAIL").ok_or_else(|| {
        config::ConfigError::NotFound("POSTMARK_SENDER_EMAIL not found in secrets".to_string())
    })?;
    let auth_token = secrets.get("POSTMARK_AUTH_TOKEN").ok_or_else(|| {
        config::ConfigError::NotFound("POSTMARK_AUTH_TOKEN not found in secrets".to_string())
    })?;

    let base_settings = Config::builder()
        .set_override("email_client.sender_email", sender_email)?
        .set_override("email_client.authorization_token", auth_token)?
        .add_source(File::from_str(BASE_CONFIG, FileFormat::Yaml))
        .build()?;

    let production_settings = Config::builder()
        .add_source(base_settings)
        .add_source(File::from_str(PRODUCTION_CONFIG, FileFormat::Yaml))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    production_settings.try_deserialize::<Settings>()
}

#[derive(Debug)]
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
