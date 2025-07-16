use crate::domain::repository_url::RepositoryURL;
use crate::domain::subscriber_email::SubscriberEmail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct NewSubscriber {
    email: SubscriberEmail,
    repository_url: RepositoryURL,
    branch: Option<String>, // TODO: 브랜치 이름 제약 조건 확인하기
}

impl NewSubscriber {
    pub fn new(
        email: SubscriberEmail,
        repository_url: RepositoryURL,
        branch: Option<String>,
    ) -> Self {
        Self {
            email,
            repository_url,
            branch,
        }
    }

    pub fn email(&self) -> &SubscriberEmail {
        &self.email
    }

    pub fn repository_url(&self) -> &RepositoryURL {
        &self.repository_url
    }

    pub fn branch(&self) -> Option<&String> {
        self.branch.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_subscriber_creation_and_getters() {
        let email = SubscriberEmail::new("test@example.com").unwrap();
        let repo_url = RepositoryURL::new("https://github.com/owner/repo").unwrap();
        let branch = Some("main".to_string());
        let subscriber = NewSubscriber::new(email.clone(), repo_url.clone(), branch.clone());

        assert_eq!(subscriber.email().as_str(), email.as_str());
        assert_eq!(subscriber.repository_url().url(), repo_url.url());
        assert_eq!(subscriber.branch(), branch.as_ref());
    }
}
