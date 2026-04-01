use std::ops::Deref;

use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::{IntoResponse, Response};

use crate::connection::RewriteConnection;

#[derive(Debug, Clone)]
pub struct RewriteDbConn(pub RewriteConnection);

impl Deref for RewriteDbConn {
    type Target = RewriteConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RewriteDbConn {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let conn = parts
            .extensions
            .get::<RewriteConnection>()
            .cloned()
            .ok_or_else(missing_rewrite_connection)?;
        Ok(Self(conn))
    }
}

impl summer_web::aide::OperationInput for RewriteDbConn {}

fn missing_rewrite_connection() -> Response {
    summer_web::problem_details::ProblemDetails::new(
        "rewrite-connection-missing",
        "Internal Server Error",
        500,
    )
    .with_detail("SQL 改写连接未初始化，请确认已添加 SqlRewriteLayer 中间件")
    .into_response()
}

#[cfg(all(test, feature = "summer-auth"))]
mod tests {
    use sea_orm::{ConnectionTrait, DbBackend, MockDatabase};
    use summer_web::axum::{extract::FromRequestParts, http::StatusCode};

    use crate::{Extensions, PluginRegistry, RewriteConnection, web::RewriteDbConn};

    fn build_parts() -> summer_web::axum::http::request::Parts {
        summer_web::axum::http::Request::builder()
            .uri("/rewrite")
            .body(())
            .expect("request")
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn rewrite_db_conn_extractor_reads_inserted_connection() {
        let mut parts = build_parts();
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        parts.extensions.insert(RewriteConnection::new(
            db,
            PluginRegistry::new(),
            Extensions::new(),
        ));

        let conn = RewriteDbConn::from_request_parts(&mut parts, &())
            .await
            .expect("rewrite db conn");

        assert_eq!(conn.get_database_backend(), DbBackend::Postgres);
    }

    #[tokio::test]
    async fn rewrite_db_conn_extractor_rejects_when_absent() {
        let mut parts = build_parts();

        let rejection = RewriteDbConn::from_request_parts(&mut parts, &())
            .await
            .expect_err("missing rewrite db conn should reject");

        assert_eq!(rejection.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
