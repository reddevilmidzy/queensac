use axum::{Router, routing::MethodRouter};
use queensac::{NewSubscriber, RepositoryURL, SubscriberEmail};
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub struct TestRouter {
    addr: SocketAddr,
}

impl TestRouter {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        Self { addr }
    }

    pub fn with_route(self, path: &str, method: MethodRouter) -> Self {
        let app = Router::new().route(path, method);
        let app_clone = app.clone();

        tokio::spawn(async move {
            let listener = TcpListener::bind(self.addr).await.unwrap();
            axum::serve(listener, app_clone).await.unwrap();
        });

        self
    }

    pub fn get_client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    pub fn get_url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

/// generate a test subscriber
pub fn create_test_subscriber() -> NewSubscriber {
    NewSubscriber::new(
        SubscriberEmail::new("test@example.com").unwrap(),
        create_test_repo_url(),
        Some("main".to_string()),
    )
}

/// generate a test repo url
pub fn create_test_repo_url() -> RepositoryURL {
    RepositoryURL::new("https://github.com/reddevilmidzy/woowalog").unwrap()
}
