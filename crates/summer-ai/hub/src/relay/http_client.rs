use std::time::Duration;

/// Shared upstream HTTP client for provider requests.
///
/// The client is created once at startup and reused across all requests.
/// `reqwest` maintains connection pools per host internally, so sharing a
/// single client still gives each upstream host its own pool while avoiding
/// repeated connector setup on the hot path.
#[derive(Clone)]
pub struct UpstreamHttpClient(reqwest::Client);

impl UpstreamHttpClient {
    pub fn build() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .tcp_keepalive(Duration::from_secs(60))
            .build()?;

        Ok(Self(client))
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.0
    }
}

impl std::ops::Deref for UpstreamHttpClient {
    type Target = reqwest::Client;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
