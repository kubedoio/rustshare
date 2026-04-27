# Review Findings Fix Implementation Plan

**Goal:** Fix the four review findings from the recent commits so backend dependencies compile with the intended Rust toolchain, file downloads do not buffer large objects in memory, and frontend dependency bumps are build-verified in CI.

**Architecture:** Keep dependency changes conservative unless the project intentionally raises its Rust MSRV. Restore backend compilation first because later tasks depend on a working baseline. Replace direct whole-object reads in download/preview handlers with S3 presigned URLs using the already-supported public endpoint path, then tighten CI so dependency updates exercise frontend type/build checks.

**Tech Stack:** Rust workspace under `backend/`, Cargo dependency resolution, Axum handlers, AWS SDK S3 presigning, GitHub Actions, SvelteKit/Vite frontend under `frontend/`.

---

### Task 1: Restore Backend Dependency Compatibility

**Files:**
- Modify: `backend/Cargo.toml`
- Modify: `backend/server/Cargo.toml`
- Modify: `backend/crates/storage/Cargo.toml`
- Modify: `backend/Cargo.lock`
- Inspect only: `rust-toolchain.toml`
- Inspect only: `backend/crates/crypto/src/password.rs`
- Inspect only: `backend/crates/crypto/src/webhook_signature.rs`

**Step 1: Confirm the current failure**

Run:

```bash
cd backend
cargo check --workspace
```

Expected: FAIL before compilation with `constant_time_eq@0.5.0 requires rustc 1.95.0`.

Then run:

```bash
cd backend
cargo check --workspace --ignore-rust-version
```

Expected: FAIL with `rand::rngs::OsRng` not satisfying `argon2::password_hash::rand_core::CryptoRngCore`, and `HmacSha256::new_from_slice` requiring `hmac::KeyInit`.

**Step 2: Choose the conservative dependency repair**

Do not raise `rust-toolchain.toml` or `backend/Cargo.toml` `rust-version` for this fix unless the project owner explicitly wants a Rust 1.95+ migration.

Edit dependency versions back to the latest compatible major families used by the current code:

```toml
# backend/Cargo.toml
jsonwebtoken = "9"
rand = "0.8"
sha2 = "0.10"
hmac = "0.12"
```

```toml
# backend/crates/storage/Cargo.toml
md5 = "0.8"
redis = { version = "1.2", features = ["tokio-comp", "connection-manager"], optional = true }
jsonwebtoken = "9"
sha2 = "0.10"
```

```toml
# backend/server/Cargo.toml
constant_time_eq = "0.3"
```

Keep low-risk compatible bumps that do not require source changes, such as `tokio = "1.52"`, `uuid = "1.23"`, `reqwest = "0.12"` only if `cargo check --workspace --all-features` passes. If any remaining bump breaks compilation, pin that crate back to the previous compatible major/minor and document why in the commit message.

**Step 3: Regenerate the lockfile**

Run:

```bash
cd backend
cargo update
```

Expected: `backend/Cargo.lock` updates and selects compatible versions for the pinned dependency families.

If Cargo selects `constant_time_eq 0.5.x`, force it down:

```bash
cd backend
cargo update -p constant_time_eq --precise 0.3.1
```

If Cargo selects `rand 0.9.x`, force it down:

```bash
cd backend
cargo update -p rand --precise 0.8.5
```

**Step 4: Verify backend compilation**

Run:

```bash
cd backend
cargo check --workspace
cargo check --workspace --all-features
cargo test --workspace --lib
cargo test --workspace --all-features --lib
```

Expected: all commands PASS.

**Step 5: Commit**

```bash
git add backend/Cargo.toml backend/crates/storage/Cargo.toml backend/server/Cargo.toml backend/Cargo.lock
git commit -m "fix(deps): restore backend dependency compatibility"
```

---

### Task 2: Add Presigned URL Tests for Public Endpoint Downloads

**Files:**
- Modify: `backend/crates/storage/src/object_store.rs`
- Test: `backend/crates/storage/src/object_store.rs`

**Step 1: Add a unit-testable helper for selecting the presign endpoint**

Introduce a small pure helper near `ObjectStore::new`:

```rust
fn presign_endpoint(internal_endpoint: &str, public_endpoint: Option<String>) -> String {
    public_endpoint.unwrap_or_else(|| internal_endpoint.to_string())
}
```

Use it in `ObjectStore::new`:

```rust
let public_endpoint = std::env::var("RUSTFS_PUBLIC_ENDPOINT").ok();
let presign_endpoint = presign_endpoint(&endpoint, public_endpoint);
```

**Step 2: Add tests for endpoint selection**

Add tests in `backend/crates/storage/src/object_store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::presign_endpoint;

    #[test]
    fn presign_endpoint_uses_public_endpoint_when_configured() {
        let endpoint = presign_endpoint(
            "http://rustfs:9000",
            Some("https://files.example.com".to_string()),
        );

        assert_eq!(endpoint, "https://files.example.com");
    }

    #[test]
    fn presign_endpoint_falls_back_to_internal_endpoint() {
        let endpoint = presign_endpoint("http://rustfs:9000", None);

        assert_eq!(endpoint, "http://rustfs:9000");
    }
}
```

**Step 3: Run the focused tests**

Run:

```bash
cd backend
cargo test -p rustshare-storage object_store::tests
```

Expected: PASS.

**Step 4: Commit**

```bash
git add backend/crates/storage/src/object_store.rs
git commit -m "test(storage): cover public presign endpoint selection"
```

---

### Task 3: Stop Buffering Preview and Download Responses

**Files:**
- Modify: `backend/server/src/handlers/files.rs`

**Step 1: Replace whole-object reads with redirects to public presigned URLs**

In `download_file_content`, remove:

```rust
let bytes = match state.object_store.get(&storage_key).await {
    Ok(bytes) => bytes,
    Err(e) => {
        tracing::error!("Failed to read file content: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to read file content")),
        )
            .into_response();
    }
};
```

Generate a presigned URL with attachment disposition:

```rust
let presigned_url = match state
    .object_store
    .get_presigned_url_with_disposition(&storage_key, 3600, Some(&content_disposition))
    .await
{
    Ok(url) => url,
    Err(e) => {
        tracing::error!("Failed to generate download URL: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to generate download URL")),
        )
            .into_response();
    }
};

let mut headers = HeaderMap::new();
headers.insert(
    header::LOCATION,
    HeaderValue::from_str(&presigned_url).unwrap_or_else(|_| HeaderValue::from_static("/")),
);

(StatusCode::FOUND, headers).into_response()
```

In `preview_file`, remove the `object_store.get()` call and generate a presigned URL with inline disposition:

```rust
let presigned_url = match state
    .object_store
    .get_presigned_url_with_disposition(&storage_key, 3600, Some("inline"))
    .await
{
    Ok(url) => url,
    Err(e) => {
        tracing::error!("Failed to generate preview URL: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to generate preview URL")),
        )
            .into_response();
    }
};

let mut headers = HeaderMap::new();
headers.insert(
    header::LOCATION,
    HeaderValue::from_str(&presigned_url).unwrap_or_else(|_| HeaderValue::from_static("/")),
);

(StatusCode::FOUND, headers).into_response()
```

**Step 2: Remove now-unused imports**

After switching back to redirects, remove any imports in `backend/server/src/handlers/files.rs` that become unused. Run compiler checks before guessing.

**Step 3: Run backend checks**

Run:

```bash
cd backend
cargo check --workspace
cargo test --workspace --lib
```

Expected: PASS.

**Step 4: Commit**

```bash
git add backend/server/src/handlers/files.rs
git commit -m "fix(files): avoid buffering downloads in backend memory"
```

---

### Task 4: Add Frontend Verification to Dependency CI

**Files:**
- Modify: `.github/workflows/dependencies.yml`

**Step 1: Add frontend check and build steps**

After the existing `Install dependencies` step in `check-frontend-dependencies`, add:

```yaml
      - name: Type check frontend
        working-directory: ./frontend
        run: npm run check

      - name: Build frontend
        working-directory: ./frontend
        run: npm run build
```

Keep `npm audit --audit-level=high || true` as non-blocking if the current project policy is advisory-only, but make type check and build blocking.

**Step 2: Validate YAML shape**

Run:

```bash
git diff --check .github/workflows/dependencies.yml
```

Expected: no whitespace errors.

If `actionlint` is installed, also run:

```bash
actionlint .github/workflows/dependencies.yml
```

Expected: PASS. If `actionlint` is not installed, note that in the final verification.

**Step 3: Verify frontend locally**

Run:

```bash
cd frontend
npm ci
npm run check
npm run build
```

Expected: all commands PASS.

**Step 4: Commit**

```bash
git add .github/workflows/dependencies.yml
git commit -m "ci: build-check frontend dependency updates"
```

---

### Task 5: Full Final Verification

**Files:**
- Inspect: `git status`

**Step 1: Run backend verification**

```bash
cd backend
cargo check --workspace
cargo check --workspace --all-features
cargo test --workspace --lib
cargo test --workspace --all-features --lib
```

Expected: PASS.

**Step 2: Run frontend verification**

```bash
cd frontend
npm run check
npm run build
```

Expected: PASS.

**Step 3: Review the final diff**

Run:

```bash
git diff --stat HEAD~4..HEAD
git log --oneline -n 6
git status -sb
```

Expected: four focused commits plus a clean worktree.

**Step 4: Open follow-up only if needed**

If the project still needs true backend proxy streaming instead of public presigned URLs, open a separate follow-up issue/plan for adding an object-store streaming API. That should be a separate change because it needs a trait design, response-body integration, and tests for range/large-object behavior.
