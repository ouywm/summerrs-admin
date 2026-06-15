use std::sync::{Arc, RwLock};

use crate::{AuthError, UserSession, permission_matches};
use summer_web::axum::body::Body;
use summer_web::axum::extract::{OriginalUri, Request};
use summer_web::axum::http::Method;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::RequestPartsExt;
use tower_layer::Layer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePermissionDecision {
    Allowed,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePermissionRule {
    method: String,
    path: String,
    actions: Vec<String>,
}

impl ResourcePermissionRule {
    pub fn new(method: impl Into<String>, path: impl Into<String>, actions: Vec<String>) -> Self {
        Self {
            method: method.into().to_ascii_uppercase(),
            path: path.into(),
            actions,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResourcePermissionPolicy {
    rules: Vec<ResourcePermissionRule>,
}

impl ResourcePermissionPolicy {
    pub fn new(rules: Vec<ResourcePermissionRule>) -> Self {
        Self { rules }
    }

    #[must_use]
    pub fn check(
        &self,
        method: &Method,
        path: &str,
        user_permissions: &[String],
    ) -> ResourcePermissionDecision {
        self.check_any_path(method, &[path], user_permissions)
    }

    #[must_use]
    pub fn check_any_path(
        &self,
        method: &Method,
        paths: &[&str],
        user_permissions: &[String],
    ) -> ResourcePermissionDecision {
        let mut matched_registered_resource = false;

        for rule in &self.rules {
            if !paths.iter().any(|path| rule.matches(method, path)) {
                continue;
            }

            matched_registered_resource = true;
            if rule.actions.is_empty() {
                return ResourcePermissionDecision::Allowed;
            }

            let action_allowed = rule.actions.iter().any(|required| {
                user_permissions
                    .iter()
                    .any(|owned| permission_matches(owned, required))
            });

            if action_allowed {
                return ResourcePermissionDecision::Allowed;
            }
        }

        if matched_registered_resource {
            ResourcePermissionDecision::Forbidden
        } else {
            ResourcePermissionDecision::Allowed
        }
    }
}

#[derive(Clone)]
pub struct ResourcePermissionRegistry {
    policy: Arc<RwLock<ResourcePermissionPolicy>>,
}

impl ResourcePermissionRegistry {
    pub fn new(policy: ResourcePermissionPolicy) -> Self {
        Self {
            policy: Arc::new(RwLock::new(policy)),
        }
    }

    #[must_use]
    pub fn check(
        &self,
        method: &Method,
        path: &str,
        user_permissions: &[String],
    ) -> ResourcePermissionDecision {
        match self.policy.read() {
            Ok(policy) => policy.check(method, path, user_permissions),
            Err(_) => {
                tracing::error!("resource permission policy lock is poisoned");
                ResourcePermissionDecision::Forbidden
            }
        }
    }

    #[must_use]
    pub fn check_any_path(
        &self,
        method: &Method,
        paths: &[&str],
        user_permissions: &[String],
    ) -> ResourcePermissionDecision {
        match self.policy.read() {
            Ok(policy) => policy.check_any_path(method, paths, user_permissions),
            Err(_) => {
                tracing::error!("resource permission policy lock is poisoned");
                ResourcePermissionDecision::Forbidden
            }
        }
    }

    pub fn replace(&self, policy: ResourcePermissionPolicy) {
        match self.policy.write() {
            Ok(mut guard) => {
                *guard = policy;
            }
            Err(_) => {
                tracing::error!("resource permission policy lock is poisoned");
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ResourcePermissionLayer;

impl ResourcePermissionLayer {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl<Inner> Layer<Inner> for ResourcePermissionLayer
where
    Inner: Clone,
{
    type Service = ResourcePermissionMiddleware<Inner>;

    fn layer(&self, inner: Inner) -> Self::Service {
        ResourcePermissionMiddleware { inner }
    }
}

#[derive(Debug)]
pub struct ResourcePermissionMiddleware<Inner> {
    inner: Inner,
}

impl<Inner: Clone> Clone for ResourcePermissionMiddleware<Inner> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Inner> tower_service::Service<Request<Body>> for ResourcePermissionMiddleware<Inner>
where
    Inner:
        tower_service::Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = Inner::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        std::mem::swap(&mut inner, &mut self.inner);

        Box::pin(async move {
            if let Err(error) = authorize_resource(&mut req) {
                return Ok(error.into_response());
            }

            inner.call(req).await
        })
    }
}

fn authorize_resource(req: &mut Request<Body>) -> Result<(), AuthError> {
    let Some(session) = req.extensions().get::<UserSession>() else {
        return Ok(());
    };
    let user_permissions = session.profile.permissions().to_vec();

    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let original_path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|OriginalUri(uri)| uri.path().to_string());

    let (parts, body) = std::mem::take(req).into_parts();
    let registry = parts
        .get_component::<ResourcePermissionRegistry>()
        .map_err(|error| AuthError::Internal(error.to_string()))?;
    *req = Request::from_parts(parts, body);

    let mut paths = vec![path.as_str()];
    if let Some(original_path) = original_path.as_deref()
        && original_path != path
    {
        paths.push(original_path);
    }

    match registry.check_any_path(&method, &paths, &user_permissions) {
        ResourcePermissionDecision::Allowed => Ok(()),
        ResourcePermissionDecision::Forbidden => {
            Err(AuthError::NoPermission("访问该资源".to_string()))
        }
    }
}

impl ResourcePermissionRule {
    fn matches(&self, method: &Method, path: &str) -> bool {
        self.method == method.as_str() && path_matches(&self.path, path)
    }
}

fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }

    let pattern_segments: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let path_segments: Vec<&str> = path.trim_matches('/').split('/').collect();

    if pattern_segments.len() != path_segments.len() {
        return false;
    }

    pattern_segments
        .iter()
        .zip(path_segments.iter())
        .all(|(pattern_segment, path_segment)| {
            is_path_parameter(pattern_segment) || pattern_segment == path_segment
        })
}

fn is_path_parameter(segment: &str) -> bool {
    segment.len() > 2 && segment.starts_with('{') && segment.ends_with('}')
}

#[cfg(test)]
mod tests {
    use summer_web::axum::http::Method;

    use super::{ResourcePermissionDecision, ResourcePermissionPolicy, ResourcePermissionRule};

    #[test]
    fn unregistered_resource_allows_for_backward_compatibility() {
        let policy = ResourcePermissionPolicy::new(vec![]);

        let decision = policy.check(&Method::GET, "/api/user/list", &[]);

        assert_eq!(decision, ResourcePermissionDecision::Allowed);
    }

    #[test]
    fn registered_resource_requires_one_bound_action_permission() {
        let policy = ResourcePermissionPolicy::new(vec![ResourcePermissionRule::new(
            "POST",
            "/api/user",
            vec!["system:user:create".to_string()],
        )]);

        let denied = policy.check(
            &Method::POST,
            "/api/user",
            &["system:user:list".to_string()],
        );
        assert_eq!(denied, ResourcePermissionDecision::Forbidden);

        let allowed = policy.check(
            &Method::POST,
            "/api/user",
            &["system:user:create".to_string()],
        );
        assert_eq!(allowed, ResourcePermissionDecision::Allowed);
    }

    #[test]
    fn resource_path_supports_axum_style_parameters() {
        let policy = ResourcePermissionPolicy::new(vec![ResourcePermissionRule::new(
            "PUT",
            "/api/role/{role_id}/permissions",
            vec!["system:role:permission".to_string()],
        )]);

        let decision = policy.check(
            &Method::PUT,
            "/api/role/42/permissions",
            &["system:role:*".to_string()],
        );

        assert_eq!(decision, ResourcePermissionDecision::Allowed);
    }

    #[test]
    fn registered_resource_without_action_bindings_allows_during_rollout() {
        let policy = ResourcePermissionPolicy::new(vec![ResourcePermissionRule::new(
            "GET",
            "/api/role/list",
            vec![],
        )]);

        let decision = policy.check(&Method::GET, "/api/role/list", &[]);

        assert_eq!(decision, ResourcePermissionDecision::Allowed);
    }

    #[test]
    fn registered_resource_can_match_any_candidate_path() {
        let policy = ResourcePermissionPolicy::new(vec![ResourcePermissionRule::new(
            "GET",
            "/api/user/list",
            vec!["system:user:list".to_string()],
        )]);

        let decision = policy.check_any_path(
            &Method::GET,
            &["/user/list", "/api/user/list"],
            &["system:user:list".to_string()],
        );

        assert_eq!(decision, ResourcePermissionDecision::Allowed);
    }
}
