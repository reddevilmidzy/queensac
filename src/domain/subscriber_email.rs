use serde::{Deserialize, Serialize};
use validator::ValidateEmail;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct SubscriberEmail(String);

impl SubscriberEmail {
    /// Creates a new `SubscriberEmail` instance.
    ///
    /// # Arguments
    ///
    /// * `email` - The email address to validate and store.
    ///
    /// # Returns
    ///
    /// Returns `Ok(SubscriberEmail)` if the email is valid, or `Err(String)` if the email is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use queensac::domain::SubscriberEmail;
    ///
    /// let email = SubscriberEmail::new("areyou@redddy.com").unwrap();
    /// ```
    pub fn new(email: impl Into<String>) -> Result<Self, String> {
        let email = email.into();
        if email.validate_email() {
            Ok(Self(email))
        } else {
            Err(format!("{email} is not a valid subscriber email."))
        }
    }

    /// Returns a reference to the email address.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriberEmail;
    use fake::Fake;
    use fake::faker::internet::en::SafeEmail;
    use quickcheck::Gen;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn empty_string_is_rejected() {
        assert!(SubscriberEmail::new("").is_err());
    }

    #[test]
    fn email_missing_at_symbol_is_rejected() {
        assert!(SubscriberEmail::new("redddy.com").is_err());
    }

    #[test]
    fn email_missing_subject_is_rejected() {
        assert!(SubscriberEmail::new("@redddy.com").is_err());
    }

    #[derive(Debug, Clone)]
    struct ValidEmailFixture(pub String);

    impl quickcheck::Arbitrary for ValidEmailFixture {
        fn arbitrary(g: &mut Gen) -> Self {
            let mut rng = StdRng::seed_from_u64(u64::arbitrary(g));
            let email = SafeEmail().fake_with_rng(&mut rng);
            Self(email)
        }
    }

    #[quickcheck_macros::quickcheck]
    fn valid_emails_are_parsed_successfully(valid_email: ValidEmailFixture) -> bool {
        SubscriberEmail::new(valid_email.0).is_ok()
    }
}
