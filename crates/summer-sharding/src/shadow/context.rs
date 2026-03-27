use std::{collections::BTreeMap, future::Future};

tokio::task_local! {
    static SHADOW_HEADERS: BTreeMap<String, String>;
}

pub async fn with_shadow_headers<F, T>(headers: BTreeMap<String, String>, future: F) -> T
where
    F: Future<Output = T>,
{
    SHADOW_HEADERS.scope(headers, future).await
}

pub fn current_headers() -> BTreeMap<String, String> {
    SHADOW_HEADERS.try_with(Clone::clone).unwrap_or_default()
}
