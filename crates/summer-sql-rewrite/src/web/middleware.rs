use std::{future::Future, pin::Pin, sync::Arc};

#[cfg(feature = "summer-auth")]
use summer_auth::UserSession;
use summer_web::axum::{body::Body, extract::Request, response::Response};
use tower_layer::Layer;

use crate::{
    configurator::SqlRewriteRequestExtender, connection::RewriteConnection, extensions::Extensions,
    registry::PluginRegistry,
};

#[derive(Clone)]
pub struct SqlRewriteLayer {
    db: sea_orm::DatabaseConnection,
    registry: Arc<PluginRegistry>,
    request_extender: Option<SqlRewriteRequestExtender>,
}

impl SqlRewriteLayer {
    pub fn new(db: sea_orm::DatabaseConnection, registry: impl Into<Arc<PluginRegistry>>) -> Self {
        Self {
            db,
            registry: registry.into(),
            request_extender: None,
        }
    }

    pub fn with_request_extender(mut self, extender: SqlRewriteRequestExtender) -> Self {
        self.request_extender = Some(extender);
        self
    }
}

impl<S: Clone> Layer<S> for SqlRewriteLayer {
    type Service = SqlRewriteMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SqlRewriteMiddleware {
            inner,
            db: self.db.clone(),
            registry: self.registry.clone(),
            request_extender: self.request_extender.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SqlRewriteMiddleware<S> {
    inner: S,
    db: sea_orm::DatabaseConnection,
    registry: Arc<PluginRegistry>,
    request_extender: Option<SqlRewriteRequestExtender>,
}

impl<S> tower_service::Service<Request> for SqlRewriteMiddleware<S>
where
    S: tower_service::Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let mut extensions = Extensions::new();
        inject_builtin_request_extensions(req.extensions(), &mut extensions);
        if let Some(extender) = &self.request_extender {
            extender(req.extensions(), &mut extensions);
        }

        let conn = RewriteConnection::new(self.db.clone(), self.registry.clone(), extensions);
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            req.extensions_mut().insert(conn);
            inner.call(req).await
        })
    }
}

fn inject_builtin_request_extensions(req_ext: &http::Extensions, ext: &mut Extensions) {
    #[cfg(feature = "summer-auth")]
    if let Some(session) = req_ext.get::<UserSession>() {
        ext.insert(session.clone());
    }
}

#[cfg(all(test, feature = "summer-auth"))]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };

    use summer_auth::{DeviceType, LoginId, UserProfile, UserSession};
    use summer_web::axum::{
        body::Body,
        extract::Request,
        http::{Request as HttpRequest, Response as HttpResponse, StatusCode},
        response::Response,
    };
    use tower_layer::Layer;
    use tower_service::Service;

    use crate::{Extensions, RewriteConnection, web::SqlRewriteLayer};

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RequestMarker(&'static str);

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CapturedContext {
        tenant_id: Option<String>,
        marker: Option<&'static str>,
    }

    #[derive(Clone)]
    struct CaptureRewriteService {
        captured: Arc<Mutex<Option<CapturedContext>>>,
    }

    impl CaptureRewriteService {
        fn new(captured: Arc<Mutex<Option<CapturedContext>>>) -> Self {
            Self { captured }
        }
    }

    impl Service<Request> for CaptureRewriteService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let conn = req
                .extensions()
                .get::<RewriteConnection>()
                .cloned()
                .expect("rewrite connection should be injected");
            let marker = conn.extensions().get::<RequestMarker>().cloned();
            *self.captured.lock().expect("capture lock") = Some(CapturedContext {
                tenant_id: None,
                marker: marker.map(|value| value.0),
            });

            Box::pin(async move {
                Ok(HttpResponse::builder()
                    .status(StatusCode::OK)
                    .body(Body::empty())
                    .expect("response"))
            })
        }
    }

    struct ReadySensitiveService {
        id: usize,
        ready: bool,
        next_id: Arc<AtomicUsize>,
    }

    impl ReadySensitiveService {
        fn new() -> Self {
            Self {
                id: 1,
                ready: false,
                next_id: Arc::new(AtomicUsize::new(2)),
            }
        }
    }

    impl Clone for ReadySensitiveService {
        fn clone(&self) -> Self {
            Self {
                id: self.next_id.fetch_add(1, Ordering::Relaxed),
                ready: false,
                next_id: self.next_id.clone(),
            }
        }
    }

    impl Service<Request> for ReadySensitiveService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.ready = true;
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request) -> Self::Future {
            assert!(
                self.ready,
                "call must use the same service instance that was polled ready (id={})",
                self.id
            );

            Box::pin(async move {
                Ok(HttpResponse::builder()
                    .status(StatusCode::OK)
                    .body(Body::empty())
                    .expect("response"))
            })
        }
    }

    fn sample_session() -> UserSession {
        UserSession {
            login_id: LoginId::new(7),
            device: DeviceType::Web,
            profile: UserProfile {
                user_name: "admin".to_string(),
                nick_name: "Admin".to_string(),
                roles: vec!["admin".to_string()],
                permissions: vec!["sys:user:list".to_string()],
            },
        }
    }

    #[tokio::test]
    async fn sql_rewrite_layer_injects_user_session_and_custom_extensions() {
        let captured = Arc::new(Mutex::new(None));
        let db = sea_orm::MockDatabase::new(sea_orm::DbBackend::Postgres).into_connection();
        let layer = SqlRewriteLayer::new(db, crate::PluginRegistry::new()).with_request_extender(
            Arc::new(|_req, ext: &mut Extensions| {
                ext.insert(RequestMarker("marker-1"));
            }),
        );
        let mut service = layer.layer(CaptureRewriteService::new(captured.clone()));
        let mut req = HttpRequest::builder()
            .uri("/rewrite")
            .body(Body::empty())
            .expect("request");
        req.extensions_mut().insert(sample_session());

        let response = service.call(req).await.expect("service response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            *captured.lock().expect("capture lock"),
            Some(CapturedContext {
                tenant_id: Some("T-WEB-001".to_string()),
                marker: Some("marker-1"),
            })
        );
    }

    #[tokio::test]
    async fn sql_rewrite_layer_uses_polled_ready_service_instance() {
        let db = sea_orm::MockDatabase::new(sea_orm::DbBackend::Postgres).into_connection();
        let layer = SqlRewriteLayer::new(db, crate::PluginRegistry::new());
        let mut service = layer.layer(ReadySensitiveService::new());
        let req = HttpRequest::builder()
            .uri("/rewrite")
            .body(Body::empty())
            .expect("request");

        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        assert!(matches!(service.poll_ready(&mut cx), Poll::Ready(Ok(()))));

        let response = service.call(req).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }
}
