//! OpenAPI spec freshness test.
//!
//! This test keeps `docs/contracts/rustshare-api-openapi.json` in sync with the
//! codebase. It regenerates the spec from `rustshare_server::openapi::ApiDoc`
//! and asserts that the committed copy matches. Run it whenever you add or
//! change annotated handlers / schemas:
//!
//!     export $(grep -v '^#' backend/.env | xargs)
//!     cargo test --test openapi_export_test -p rustshare-server
//!
//! If the test fails because the spec legitimately changed, rerun with the
//! `RUSTSHARE_UPDATE_OPENAPI` environment variable set to overwrite the
//! committed copy:
//!
//!     RUSTSHARE_UPDATE_OPENAPI=1 cargo test --test openapi_export_test -p rustshare-server

use std::path::PathBuf;

fn spec_path() -> PathBuf {
    // CARGO_MANIFEST_DIR for rustshare-server is backend/server, so the
    // workspace root is two levels up.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("missing backend dir")
        .parent()
        .expect("missing workspace root")
        .join("docs/contracts/rustshare-api-openapi.json")
}

#[test]
fn openapi_spec_is_fresh() {
    let generated = rustshare_server::openapi::to_pretty_json()
        .expect("failed to serialize OpenAPI spec to JSON");

    let update = std::env::var("RUSTSHARE_UPDATE_OPENAPI").is_ok();

    let spec_path = spec_path();
    if update {
        rustshare_server::openapi::export_to_file(&spec_path)
            .expect("failed to write OpenAPI spec to file");
        return;
    }

    let committed = std::fs::read_to_string(&spec_path).unwrap_or_else(|e| {
        panic!(
            "failed to read committed spec at {}: {}",
            spec_path.display(),
            e
        )
    });

    assert_eq!(
        generated, committed,
        "OpenAPI spec is stale. Regenerate by running:\n  RUSTSHARE_UPDATE_OPENAPI=1 cargo test --test openapi_export_test -p rustshare-server\nCommitted: {}",
        spec_path.canonicalize().unwrap_or(spec_path).display()
    );
}
