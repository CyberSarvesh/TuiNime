pub mod aggregator;
pub mod allanime;
pub mod anilist;
pub mod scraper;
pub mod traits;

use std::time::Duration;

/// Shared HTTP client builder. Every client gets connect + total timeouts so a stalled
/// server can never hang a request (and therefore the UI task waiting on it) forever.
pub fn http_client(user_agent: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .expect("reqwest client")
}
