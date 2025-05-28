use crate::domain::{SubscriberEmail, email_verification::EmailVerificationStore};
use crate::email_client::EmailClient;
use secrecy::ExposeSecret;
use sqlx::PgPool;

pub struct EmailVerificationService {
    store: EmailVerificationStore,
    email_client: EmailClient,
}

impl EmailVerificationService {
    pub fn new(pool: PgPool, email_client: EmailClient) -> Self {
        Self {
            store: EmailVerificationStore::new(pool),
            email_client,
        }
    }

    pub async fn send_verification_email(&self, email: String) -> Result<(), String> {
        let verification = self
            .store
            .create_verification(email.clone())
            .await
            .map_err(|e| format!("Failed to create verification: {}", e))?;

        let subscriber_email =
            SubscriberEmail::new(email.clone()).map_err(|_| "Invalid email format".to_string())?;

        let subject = "이메일 인증 코드";
        let html_content = format!(
            r#"
            <h1>이메일 인증</h1>
            <p>귀하의 이메일 인증 코드는 다음과 같습니다:</p>
            <h2>{}</h2>
            <p>이 코드는 10분 후에 만료됩니다.</p>
            "#,
            verification.code.expose_secret()
        );
        let text_content = format!(
            "귀하의 이메일 인증 코드는 {} 입니다. 이 코드는 10분 후에 만료됩니다.",
            verification.code.expose_secret()
        );

        self.email_client
            .send_email(
                subscriber_email,
                subject.to_string(),
                html_content,
                text_content,
            )
            .await
    }

    pub async fn verify_code(&self, email: &str, code: &str) -> Result<bool, String> {
        self.store
            .verify_code(email, code)
            .await
            .map_err(|e| format!("Failed to verify code: {}", e))
    }

    pub async fn is_verified(&self, email: &str) -> Result<bool, String> {
        self.store
            .is_verified(email)
            .await
            .map_err(|e| format!("Failed to check verification status: {}", e))
    }

    pub async fn cleanup_expired(&self) -> Result<(), String> {
        self.store
            .cleanup_expired()
            .await
            .map_err(|e| format!("Failed to cleanup expired verifications: {}", e))?;
        Ok(())
    }
}
