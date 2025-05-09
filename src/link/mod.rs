#[derive(Debug, Eq, PartialEq)]
pub enum LinkCheckResult {
    Valid,
    Invalid(String),
}

pub async fn check_link(url: &str) -> LinkCheckResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let mut attempts = 3;
    while attempts > 0 {
        match client.get(url).send().await {
            Ok(res) => {
                let status = res.status();
                return if status.is_success() || status.is_redirection() {
                    LinkCheckResult::Valid
                } else {
                    LinkCheckResult::Invalid(format!("HTTP status code: {}", status))
                };
            }
            Err(e) => {
                if attempts == 1 {
                    return LinkCheckResult::Invalid(format!("Request error: {}", e));
                }
            }
        }
        attempts -= 1;  
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }   

    
    LinkCheckResult::Invalid("Max retries exceeded".to_string())
}

#[cfg(test)]



mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_link() {
        let link = "https://redddy.com";
        assert!(matches!(
            check_link(link).await,
            LinkCheckResult::Invalid(_)
        ));
        let link = "https://lazypazy.tistory.com";
        assert_eq!(check_link(link).await, LinkCheckResult::Valid);
    }
}
