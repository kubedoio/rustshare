# RustShare Release Process

This document defines how RustShare maintainers cut, publish, and manage releases.

> **Maintainers:** @senolcolak, @zoorpha  
> **Registry:** `ghcr.io/kubedoio/rustshare-backend`  
> **CI Workflow:** `.github/workflows/release.yml` (stable), `.github/workflows/pilot-release.yml` (main/edge)

---

## SemVer Policy

RustShare follows **strict SemVer 2.0.0**.

- **Tag format:** `vMAJOR.MINOR.PATCH` (e.g., `v0.3.1`, `v1.0.0`)
- `MAJOR` — incompatible API or behavioral changes
- `MINOR` — backward-compatible functionality additions
- `PATCH` — backward-compatible bug fixes

Only annotated, signed Git tags are valid release triggers.

---

## Pre-1.0 Policy

While the project is `< 1.0.0`:

- **MINOR bumps may contain breaking changes.** A move from `0.2.0` → `0.3.0` can require operator action.
- **PATCH is for fixes and safe additions.** A move from `0.2.0` → `0.2.1` should be safe to apply without reading the changelog.

**Action for users:**

- Pin to exact versions (`0.3.1`, not `0.3` or `0`) in production.
- Always read [`CHANGELOG.md`](../CHANGELOG.md) and [`docs/upgrading.md`](upgrading.md) before upgrading across MINOR boundaries.

---

## Release Channels

The release workflows publish the following Docker tags:

- **`.github/workflows/pilot-release.yml`** (main branch): `edge`, `nightly-YYYY-MM-DD`, `sha-<gitsha>`
- **`.github/workflows/release.yml`** (stable / RC tags): `X.Y.Z`, `X.Y`, `X`, `latest`, `sha-<gitsha>` (stable); `X.Y.Z-rc.N`, `sha-<gitsha>` (RC)

Combined tag matrix:

| Tag | Source | Stability | Use case |
|-----|--------|-----------|----------|
| `edge` | Latest `main` push | Unstable | Development, CI validation |
| `nightly-YYYY-MM-DD` | Dated `main` push | Unstable | Snapshot testing, bisecting |
| `sha-<gitsha>` | Every build | Unstable | Exact reproducibility |
| `X.Y.Z-rc.N` | Release candidate tag (`vX.Y.Z-rc.N`) | Pre-release | Staging validation |
| `X.Y.Z` | Stable tag (`vX.Y.Z`) | Stable | Production deployments |
| `X.Y` | Rolling minor alias | Stable | Automatic patch uptake |
| `X` | Rolling major alias | Stable | Automatic minor uptake (post-1.0) |
| `latest` | Latest stable tag only | Stable | Quick start, never `main` |

**Rules:**

- `latest` is **only** moved by stable version tags. A `main` push never overwrites `latest`.
- Rolling aliases (`X.Y`, `X`) are updated on every stable release so users can choose their uptake cadence.

---

## Release Candidate Flow

1. **Decide the target version.** Ensure `CHANGELOG.md` has an "Unreleased" section with all changes since the last stable release.
2. **Create an RC tag:**
   ```bash
   git tag -s v0.4.0-rc.1 -m "Release candidate 0.4.0-rc.1"
   git push origin v0.4.0-rc.1
   ```
3. **CI actions:**
   - Runs the pilot compose smoke test.
   - Builds and scans the image with Trivy.
   - Publishes tags: `0.4.0-rc.1`, `sha-<gitsha>`.
   - Does **not** update `latest`, `0.4`, or `0`.
4. **Validate the RC:**
   - Deploy to staging.
   - Run integration tests.
   - Verify migration behavior against a copy of production data.
5. **Promote to stable:**
   - If the RC passes, create the stable tag (`v0.4.0`).
   - If issues are found, fix on `main`, cut `v0.4.0-rc.2`, and repeat.

---

## Stable Release Checklist

Before pushing a stable tag, complete every step:

1. **Update `CHANGELOG.md`**
   - Move items from "Unreleased" to a new `## [X.Y.Z] - YYYY-MM-DD` section.
   - Link the diff: `https://github.com/kubedoio/rustshare/compare/vA.B.C...vX.Y.Z`

2. **Update `docs/upgrading.md`**
   - Add version-specific notes for this release.
   - Document any breaking changes or new environment variables.

3. **Run the full test suite locally**
   ```bash
   cargo test --workspace
   cd frontend && npm run test
   ./scripts/final-launch-smoke.sh
   ```

4. **Commit documentation changes**
   ```bash
   git add CHANGELOG.md docs/upgrading.md
   git commit -m "docs: prepare release vX.Y.Z"
   git push origin main
   ```

5. **Create a signed Git tag**
   ```bash
   git tag -s vX.Y.Z -m "Release vX.Y.Z"
   ```
   > All stable tags **must** be GPG-signed. See [Git Tag Signing](#git-tag-signing) below.

6. **Push the tag**
   ```bash
   git push origin vX.Y.Z
   ```

7. **Wait for the release workflow**
   - Monitor `.github/workflows/release.yml` on the Actions tab.
   - Confirm binaries, SBOMs, and Docker images are built and attested successfully.

8. **Verify the published artifacts**
   - **GitHub Release:** Auto-generated from the tag.
   - **Docker tags:** `X.Y.Z`, `X.Y`, `X`, `latest` on `ghcr.io/kubedoio/rustshare-backend`.
   - **SBOM & attestation:** Verify attestation is available in the package registry.
   - **Binary artifacts:** Download and sanity-check the Linux `x86_64` and `aarch64` binaries attached to the GitHub Release.

9. **Announce (if applicable)**
   - Post to relevant channels (GitHub Discussions, Discord, Mastodon, etc.).
   - Include a link to the release notes and `docs/upgrading.md`.

---

## Emergency Release / Hotfix

For security fixes or critical regressions requiring immediate shipment:

1. **Create a hotfix branch from the last stable tag**
   ```bash
   git checkout -b hotfix/vX.Y.Z+1 vX.Y.Z
   ```

2. **Apply the minimal fix.** Do not bundle unrelated changes.

3. **Fast-track validation**
   - Run the affected tests.
   - Run `./scripts/final-launch-smoke.sh`.
   - If the fix is in a critical path, run `./scripts/run-restore-drill.sh` against a backup to confirm data integrity.

4. **Update `CHANGELOG.md` and `docs/upgrading.md`** with the hotfix description.

5. **Tag and push**
   ```bash
   git tag -s vX.Y.Z+1 -m "Hotfix vX.Y.Z+1"
   git push origin vX.Y.Z+1
   ```

6. **Merge the hotfix back to `main`** immediately after the release succeeds.

---

## Rollback Procedure

If a stable release is found to be defective after publication:

1. **Do not delete the tag or the GitHub Release.** Deletion breaks downstream caches and mirrors.
2. **Edit the GitHub Release notes** to mark it as deprecated:
   - Add a prominent `## ⚠️ Deprecated` banner at the top.
   - Explain why it is deprecated and which version to use instead.
3. **Retag `latest` (and rolling aliases) to the previous stable version**
   ```bash
   # Pull the last known-good manifest
   docker pull ghcr.io/kubedoio/rustshare-backend:X.Y.Z-1
   # Retag as latest
   docker tag ghcr.io/kubedoio/rustshare-backend:X.Y.Z-1 \
              ghcr.io/kubedoio/rustshare-backend:latest
   docker push ghcr.io/kubedoio/rustshare-backend:latest
   ```
   > In practice, this is done by CI: push a new PATCH release (`vX.Y.Z+1`) that reverts the defect, or manually update the rolling aliases via the registry UI/API.
4. **Notify users** via the same channels used for the release announcement.
5. **Document the incident** in `CHANGELOG.md` under the rolled-back version.

---

## Git Tag Signing

All stable release tags (`v*`) must be GPG-signed. Release candidates are encouraged but not strictly required to be signed.

**Requirement:**

```bash
git tag -s vX.Y.Z -m "Release vX.Y.Z"
```

**Verify a tag before trusting it:**

```bash
git tag -v vX.Y.Z
```

**Tips:**

- Configure your signing key globally:
  ```bash
  git config --global user.signingkey YOUR_KEY_ID
  git config --global tag.gpgSign true
  ```
- Upload your public key to GitHub so tags show "Verified."
- Ensure the release workflow does not push tags; maintainers push signed tags from local machines.

---

## Binary Artifacts

The release workflow builds and attaches static Linux binaries to each GitHub Release:

| Target | Expected artifact |
|--------|-------------------|
| `x86_64-unknown-linux-gnu` | `rustshare-backend-x86_64-linux` |
| `aarch64-unknown-linux-gnu` | `rustshare-backend-aarch64-linux` |

These binaries are built in the `docker/backend.Dockerfile` multi-stage build and extracted by the release workflow. Operators who prefer binaries over Docker can download them directly from the release page.
