use crate::domain::repository_url::RepositoryURL;
use crate::domain::subscriber_email::SubscriberEmail;

pub struct NewSubscriber {
    email: SubscriberEmail,
    repository_url: RepositoryURL,
}

impl NewSubscriber {
    pub fn new(email: SubscriberEmail, repository_url: RepositoryURL) -> Self {
        Self {
            email,
            repository_url,
        }
    }

    pub fn email(&self) -> &SubscriberEmail {
        &self.email
    }

    pub fn repository_url(&self) -> &RepositoryURL {
        &self.repository_url
    }
}
