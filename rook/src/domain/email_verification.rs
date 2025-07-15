use chrono::{DateTime, Duration, Utc};
use rand::{Rng, rngs::ThreadRng};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgQueryResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailVerification {
    pub email: String,
    #[serde(skip_serializing)]
    pub code: Secret<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub verified: bool,
}

impl EmailVerification {
    pub fn new(email: String) -> Self {
        let code = generate_verification_code();
        let created_at = Utc::now();
        let expires_at = created_at + Duration::minutes(10); // 10분 후 만료

        Self {
            email,
            code,
            created_at,
            expires_at,
            verified: false,
        }
    }

    pub async fn save(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE subscribers
            SET verification_code = $1,
                verification_code_created_at = $2,
                verification_code_expires_at = $3,
                is_verified = $4
            WHERE email = $5
            "#,
            self.code.expose_secret(),
            self.created_at,
            self.expires_at,
            self.verified,
            self.email
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn verify(&mut self, code: &str, pool: &PgPool) -> Result<bool, sqlx::Error> {
        if self.verified || Utc::now() > self.expires_at {
            return Ok(false);
        }

        if self.code.expose_secret() == code {
            self.verified = true;
            sqlx::query!(
                r#"
                UPDATE subscribers
                SET is_verified = true
                WHERE email = $1
                "#,
                self.email
            )
            .execute(pool)
            .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn generate_verification_code() -> Secret<String> {
    let mut rng = ThreadRng::default();
    let code: String = (0..6)
        .map(|_| rng.random_range(0..10).to_string())
        .collect();
    Secret::new(code)
}

#[derive(Debug)]
pub struct EmailVerificationStore {
    pool: PgPool,
}

impl EmailVerificationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_verification(
        &self,
        email: String,
    ) -> Result<EmailVerification, sqlx::Error> {
        let verification = EmailVerification::new(email);
        verification.save(&self.pool).await?;
        Ok(verification)
    }

    pub async fn get_verification(
        &self,
        email: &str,
    ) -> Result<Option<EmailVerification>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct VerificationRecord {
            email: String,
            verification_code: Option<String>,
            verification_code_created_at: Option<DateTime<Utc>>,
            verification_code_expires_at: Option<DateTime<Utc>>,
            is_verified: bool,
        }

        let record = sqlx::query_as::<_, VerificationRecord>(
            r#"
            SELECT email,
                   verification_code,
                   verification_code_created_at,
                   verification_code_expires_at,
                   is_verified
            FROM subscribers
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record.map(|r| EmailVerification {
            email: r.email,
            code: Secret::new(r.verification_code.unwrap_or_default()),
            created_at: r.verification_code_created_at.unwrap_or_else(Utc::now),
            expires_at: r.verification_code_expires_at.unwrap_or_else(Utc::now),
            verified: r.is_verified,
        }))
    }

    pub async fn verify_code(&self, email: &str, code: &str) -> Result<bool, sqlx::Error> {
        if let Some(mut verification) = self.get_verification(email).await? {
            verification.verify(code, &self.pool).await
        } else {
            Ok(false)
        }
    }

    pub async fn is_verified(&self, email: &str) -> Result<bool, sqlx::Error> {
        let record = sqlx::query!(
            r#"
            SELECT is_verified
            FROM subscribers
            WHERE email = $1
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(record.map(|r| r.is_verified).unwrap_or(false))
    }

    pub async fn cleanup_expired(&self) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE subscribers
            SET verification_code = NULL,
                verification_code_created_at = NULL,
                verification_code_expires_at = NULL
            WHERE verification_code_expires_at < CURRENT_TIMESTAMP
            "#
        )
        .execute(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_verification_code() {
        let code = generate_verification_code();
        assert_eq!(code.expose_secret().len(), 6);
        assert!(code.expose_secret().chars().all(|c| c.is_digit(10)));
    }

    #[test]
    fn test_create_email_verification() {
        let email = "test@example.com";
        let verification = EmailVerification::new(email.to_string());
        assert_eq!(verification.email, email);
        assert!(verification.expires_at > verification.created_at);
    }
}
