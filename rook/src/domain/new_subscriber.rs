use crate::domain::subscriber_email::SubscriberEmail;

pub struct NewSubscriber {
    _email: SubscriberEmail,
    _repository_url: String, // TODO: replace with RepositoryURL
}
