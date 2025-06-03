use crate::domain::SubscriberEmail;
use config::{Config, File, FileFormat};
use secrecy::Secret;
use serde::Deserialize;
use std::time::Duration;

const BASE_CONFIG: &str = include_str!("../configuration/base.yaml");
const LOCAL_CONFIG: &str = include_str!("../configuration/local.yaml");
const PRODUCTION_CONFIG: &str = include_str!("../configuration/production.yaml");

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub email_client: EmailClientSettings,
}

#[derive(Debug, Deserialize)]
pub struct EmailClientSettings {
    pub base_url: String,
    pub sender_email: String,
    #[serde(deserialize_with = "deserialize_secret")]
    pub authorization_token: Secret<String>,
    pub timeout_seconds: u64,
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

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    // Detect environment
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");

    let enviroment_config = match environment {
        Environment::Local => LOCAL_CONFIG,
        Environment::Production => PRODUCTION_CONFIG,
    };

    let settings = Config::builder()
        .add_source(File::from_str(BASE_CONFIG, FileFormat::Yaml))
        .add_source(File::from_str(enviroment_config, FileFormat::Yaml))
        .add_source(config::Environment::with_prefix("APP").separator("__"))
        .build()?;

    settings.try_deserialize::<Settings>()
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
                "{} is not a supported environment. Use either `local` or `production`.",
                other
            )),
        }
    }
}
