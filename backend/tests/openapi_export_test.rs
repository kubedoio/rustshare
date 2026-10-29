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
fn openapi_spec_version_is_2_0_0() {
    let spec = rustshare_server::openapi::to_pretty_json()
        .expect("failed to serialize OpenAPI spec to JSON");
    let value: serde_json::Value =
        serde_json::from_str(&spec).expect("failed to parse generated OpenAPI spec as JSON");
    let version = value
        .get("info")
        .and_then(|info| info.get("version"))
        .and_then(|v| v.as_str())
        .expect("missing info.version in OpenAPI spec");
    assert_eq!(
        version, "2.0.0",
        "OpenAPI spec version must be 2.0.0 to signal breaking contract changes"
    );
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

#[test]
fn account_and_vault_device_delete_routes_are_distinct() {
    let spec = rustshare_server::openapi::to_pretty_json()
        .expect("failed to serialize OpenAPI spec to JSON");
    let value: serde_json::Value =
        serde_json::from_str(&spec).expect("failed to parse generated OpenAPI spec as JSON");
    for (path, tag, operation_id, parameter) in [
        (
            "/api/v1/user/devices/{id}",
            "Devices",
            "revoke_user_device",
            "id",
        ),
        (
            "/api/vault-sync/v1/devices/{device_id}",
            "Vault Sync",
            "revoke_device",
            "device_id",
        ),
    ] {
        let operation = value["paths"]
            .get(path)
            .and_then(|path_item| path_item.get("delete"))
            .unwrap_or_else(|| panic!("missing DELETE operation for {path}"));
        assert_eq!(operation["tags"][0], tag, "wrong tag for {path}");
        assert_eq!(operation["operationId"], operation_id);
        let parameter = operation["parameters"]
            .as_array()
            .and_then(|parameters| {
                parameters
                    .iter()
                    .find(|candidate| candidate["name"] == parameter)
            })
            .unwrap_or_else(|| panic!("missing {parameter} path parameter for {path}"));
        assert_eq!(parameter["in"], "path");
        assert_eq!(parameter["required"], true);
    }
}
