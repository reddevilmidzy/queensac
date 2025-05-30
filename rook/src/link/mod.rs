#[derive(Debug, Eq, PartialEq)]
pub enum LinkCheckResult {
    Valid,
    Redirect(String),
    Invalid(String),
}

pub async fn check_link(url: &str) -> LinkCheckResult {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let mut attempts = 3;
    while attempts > 0 {
        match client.get(url).send().await {
            Ok(res) => {
                let status = res.status();
                if status.is_success() {
                    return LinkCheckResult::Valid;
                } else if status.is_redirection() {
                    if let Some(redirect_url) = res.headers().get("location") {
                        if let Ok(redirect_str) = redirect_url.to_str() {
                            return LinkCheckResult::Redirect(redirect_str.to_string());
                        }
                    }
                    return LinkCheckResult::Valid;
                } else {
                    return LinkCheckResult::Invalid(format!("HTTP status code: {}", status));
                }
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
        let link = "https://redddy.ai";
        assert!(matches!(
            check_link(link).await,
            LinkCheckResult::Invalid(_)
        ));
        let link = "https://lazypazy.tistory.com";
        assert_eq!(check_link(link).await, LinkCheckResult::Valid);
    }

    #[tokio::test]
    async fn change_organization_name() {
        let link = "https://github.com/Bibimbap-Team/git-playground";
        assert_eq!(
            check_link(link).await,
            LinkCheckResult::Redirect("https://github.com/Coduck-Team/git-playground".to_string())
        );
    }

    #[tokio::test]
    async fn change_branch_name() {
        let link = "https://github.com/sindresorhus/cli-spinners/tree/master";
        assert_eq!(
            check_link(link).await,
            LinkCheckResult::Redirect(
                "https://github.com/sindresorhus/cli-spinners/tree/main".to_string()
            )
        );
    }

    #[tokio::test]
    async fn change_repository_name() {
        let link = "https://github.com/reddevilmidzy/coduo-java-rps";
        assert_eq!(
            check_link(link).await,
            LinkCheckResult::Redirect("https://github.com/reddevilmidzy/coduo-rps".to_string())
        );
    }
}
