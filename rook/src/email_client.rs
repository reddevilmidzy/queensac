use crate::domain::SubscriberEmail;
use reqwest::Client;
use secrecy::{ExposeSecret, Secret};
use std::time::Duration;

pub struct EmailClient {
    http_client: Client,
    base_url: String,
    sender: SubscriberEmail,
    authorization_token: Secret<String>,
}

impl EmailClient {
    pub fn new(
        base_url: String,
        sender: SubscriberEmail,
        authorization_token: Secret<String>,
        timeout: Duration,
    ) -> Self {
        let http_client = Client::builder().timeout(timeout).build().unwrap();
        Self {
            http_client,
            base_url,
            sender,
            authorization_token,
        }
    }

    pub async fn send_email(
        &self,
        recipient: SubscriberEmail,
        subject: String,
        html_content: String,
        message_stream: String,
    ) -> Result<(), String> {
        let url = format!("{}/email", self.base_url);
        let request_body = SendEmailRequest {
            from: self.sender.as_ref().to_owned(),
            to: recipient.as_ref().to_owned(),
            subject: subject.to_owned(),
            html_body: html_content.to_owned(),
            message_stream: message_stream.to_owned(),
        };

        let response = self
            .http_client
            .post(&url)
            .header(
                "X-Postmark-Server-Token",
                self.authorization_token.expose_secret(),
            )
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Failed to send email: {}", e))?;

        match response.error_for_status() {
            Ok(_) => Ok(()),
            Err(e) => {
                let status = e
                    .status()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Unknown status".to_string());
                let error_message = e.to_string();
                Err(format!(
                    "Failed to send email. Status: {}. Error: {}",
                    status, error_message
                ))
            }
        }
    }

    pub async fn send_email_with_retry(
        &self,
        recipient: SubscriberEmail,
        subject: String,
        html_content: String,
        message_stream: String,
        max_retries: usize,
        retry_delay: Duration,
    ) -> Result<(), String> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self
                .send_email(
                    recipient.clone(),
                    subject.clone(),
                    html_content.clone(),
                    message_stream.clone(),
                )
                .await
            {
                Ok(_) => {
                    return Ok(());
                }
                Err(e) => {
                    if attempt >= max_retries {
                        return Err(format!(
                            "Failed to send email after {} attempts. Last error: {}",
                            attempt, e
                        ));
                    } else {
                        tokio::time::sleep(retry_delay).await;
                    }
                }
            }
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct SendEmailRequest {
    from: String,
    to: String,
    subject: String,
    html_body: String,
    message_stream: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn send_email_sends_the_expected_request() {
        // Arrange
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::new("sender@example.com").unwrap();
        let recipient = SubscriberEmail::new("recipient@example.com").unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            Secret::new("test-token".to_string()),
            Duration::from_secs(10),
        );

        Mock::given(header("X-Postmark-Server-Token", "test-token"))
            .and(path("/email"))
            .and(method("POST"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "From": "sender@example.com",
                "To": "recipient@example.com",
                "Subject": "Test subject",
                "HtmlBody": "<p>Test HTML content</p>",
                "MessageStream": "broadcast"
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(
                recipient,
                "Test subject".to_string(),
                "<p>Test HTML content</p>".to_string(),
                "broadcast".to_string(),
            )
            .await;

        // Assert
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn send_email_fails_if_the_server_returns_500() {
        // Arrange
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::new("sender@example.com").unwrap();
        let recipient = SubscriberEmail::new("recipient@example.com").unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            Secret::new("test-token".to_string()),
            Duration::from_secs(10),
        );

        Mock::given(header("X-Postmark-Server-Token", "test-token"))
            .and(path("/email"))
            .and(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Act
        let outcome = email_client
            .send_email(
                recipient,
                "Test subject".to_string(),
                "<p>Test HTML content</p>".to_string(),
                "broadcast".to_string(),
            )
            .await;

        // Assert
        assert!(outcome.is_err());
    }

    #[tokio::test]
    async fn send_email_with_retry_succeeds_on_first_try() {
        let mock_server = wiremock::MockServer::start().await;
        let sender = SubscriberEmail::new("sender@example.com").unwrap();
        let recipient = SubscriberEmail::new("recipient@example.com").unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            Secret::new("test-token".to_string()),
            Duration::from_secs(10),
        );

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = email_client
            .send_email_with_retry(
                recipient,
                "subject".to_string(),
                "<p>content</p>".to_string(),
                "broadcast".to_string(),
                3,
                Duration::from_millis(10),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_email_with_retry_succeeds_after_retries() {
        let mock_server = wiremock::MockServer::start().await;
        let sender = SubscriberEmail::new("sender@example.com").unwrap();
        let recipient = SubscriberEmail::new("recipient@example.com").unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            Secret::new("test-token".to_string()),
            Duration::from_secs(10),
        );

        // 처음 두 번은 실패, 세 번째는 성공
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .up_to_n_times(2)
            .expect(2)
            .mount(&mock_server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = email_client
            .send_email_with_retry(
                recipient,
                "subject".to_string(),
                "<p>content</p>".to_string(),
                "broadcast".to_string(),
                3,
                Duration::from_millis(10),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_email_with_retry_fails_after_all_retries() {
        let mock_server = wiremock::MockServer::start().await;
        let sender = SubscriberEmail::new("sender@example.com").unwrap();
        let recipient = SubscriberEmail::new("recipient@example.com").unwrap();
        let email_client = EmailClient::new(
            mock_server.uri(),
            sender,
            Secret::new("test-token".to_string()),
            Duration::from_secs(10),
        );

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .expect(3)
            .mount(&mock_server)
            .await;

        let result = email_client
            .send_email_with_retry(
                recipient,
                "subject".to_string(),
                "<p>content</p>".to_string(),
                "broadcast".to_string(),
                3,
                Duration::from_millis(10),
            )
            .await;

        assert!(result.is_err());
    }
}
