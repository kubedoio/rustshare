//! Request-scoped PostgreSQL Row-Level Security (RLS) context.
//!
//! This middleware sets the `app.current_tenant_id` and `app.current_user_id`
//! configuration variables on a connection taken from the pool for the
//! authenticated request. These variables can be referenced by PostgreSQL RLS
//! policies if they are defined.
//!
//! # Important caveat
//!
//! The connection used to set the context is acquired and returned to the pool
//! *before* the inner handler runs. The handler's queries will execute on
//! different connections checked out from the pool. Therefore this middleware
//! does **not** provide runtime RLS enforcement. Repository-level tenant
//! filtering remains the primary defense against cross-tenant access.
//!
//! The per-connection settings are reset to restrictive nil-UUID defaults by the
//! pool's `before_acquire` hook, so a recycled connection never leaks context
//! between requests.

use axum::{
    extract::{FromRequestParts, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    RequestPartsExt,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::handlers::extractors::AuthenticatedUser;

/// Middleware that stamps a pooled PostgreSQL connection with the authenticated
/// tenant/user context.
///
/// Authentication is resolved here so the middleware can run before the inner
/// handler. Requests without a valid authentication session are treated as
/// unauthenticated and pass through unchanged. This allows the layer to be
/// applied broadly while only affecting routes that actually require
/// authentication.
///
/// The state type `S` must provide access to the PostgreSQL pool and support
/// extracting [`AuthenticatedUser`]. It is generic so the middleware can be
/// unit-tested with a lightweight wrapper around a dummy pool.
pub async fn tenant_context_middleware<S>(
    State(state): State<S>,
    request: Request,
    next: Next,
) -> Response
where
    S: AsRef<PgPool> + Clone + Send + Sync + 'static,
    AuthenticatedUser: FromRequestParts<S>,
{
    let (mut parts, body) = request.into_parts();
    let user = parts
        .extract_with_state::<AuthenticatedUser, S>(&state)
        .await
        .ok();
    let request = Request::from_parts(parts, body);

    if let Some(user) = user {
        if let Err(e) = set_rls_context(state.as_ref(), user.tenant_id, user.user_id).await {
            tracing::error!(error = %e, "Failed to set per-request RLS context");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to set tenant context",
            )
                .into_response();
        }
    }

    next.run(request).await
}

/// Set `app.current_tenant_id` and `app.current_user_id` on a connection from
/// the pool.
///
/// The values are trusted `Uuid`s from the authentication layer, so formatting
/// them into the `SET` statement is safe. PostgreSQL `SET` does not support
/// parameter placeholders, so we use the literal form.
async fn set_rls_context(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;

    let tenant_sql = format!("SET app.current_tenant_id = '{}'", tenant_id);
    sqlx::query(&tenant_sql).execute(&mut *conn).await?;

    let user_sql = format!("SET app.current_user_id = '{}'", user_id);
    sqlx::query(&user_sql).execute(&mut *conn).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct TestState(PgPool);

    impl AsRef<PgPool> for TestState {
        fn as_ref(&self) -> &PgPool {
            &self.0
        }
    }

    impl FromRequestParts<TestState> for AuthenticatedUser {
        type Rejection = StatusCode;

        async fn from_request_parts(
            _parts: &mut axum::http::request::Parts,
            _state: &TestState,
        ) -> Result<Self, Self::Rejection> {
            // The test router does not provide real credentials, so treat every
            // request as unauthenticated for the pass-through test.
            Err(StatusCode::UNAUTHORIZED)
        }
    }

    fn dummy_pool() -> PgPool {
        // A lazily-initialised pool is enough for tests that do not perform a
        // real checkout (e.g. unauthenticated pass-through).
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost:1/dummy")
            .expect("lazy pool creation should succeed")
    }

    #[tokio::test]
    async fn passes_through_when_unauthenticated() {
        let state = TestState(dummy_pool());
        let app = Router::new()
            .route("/", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                tenant_context_middleware::<TestState>,
            ));

        let response = app.oneshot(Request::new(Body::empty())).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn set_rls_context_sql_formats_uuids_safely() {
        let tenant_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let user_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        let tenant_sql = format!("SET app.current_tenant_id = '{}'", tenant_id);
        let user_sql = format!("SET app.current_user_id = '{}'", user_id);

        assert_eq!(
            tenant_sql,
            "SET app.current_tenant_id = '11111111-1111-1111-1111-111111111111'"
        );
        assert_eq!(
            user_sql,
            "SET app.current_user_id = '22222222-2222-2222-2222-222222222222'"
        );
    }
}
