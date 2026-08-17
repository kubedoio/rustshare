# RustShare Release Process

This document defines how RustShare maintainers cut, publish, and manage releases.

> **Maintainers:** @senolcolak, @zoorpha  
> **Registry:** `ghcr.io/kubedoio/rustshare-backend`  
> **CI Workflow:** `.github/workflows/release.yml` (stable + prerelease tags), `.github/workflows/pilot-release.yml` (main/edge)

---

## SemVer Policy

RustShare follows **strict SemVer 2.0.0**.

- **Tag format:** `vMAJOR.MINOR.PATCH` (e.g., `v0.3.1`, `v1.0.0`), optionally
  with a SemVer prerelease segment (e.g., `v0.8.0-alpha.1`, `v1.0.0-rc.1`)
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
- **`.github/workflows/release.yml`** (stable tags): `X.Y.Z`, `X.Y`, `X`, `latest`, `sha-<gitsha>`; prerelease tags: `X.Y.Z-<pre>` only (no `latest`/aliases)

> **Note:** `release.yml` accepts `vX.Y.Z` stable tags and SemVer prerelease
> tags (`vX.Y.Z-alpha.N`, `vX.Y.Z-rc.N`, …) — the grammar lives in
> `scripts/release-tag.sh` (single source of truth). Prerelease tags publish a
> version-only Docker tag and a GitHub **prerelease**; `latest` and the rolling
> aliases are moved by stable tags only (see [Pre-release Validation](#pre-release-validation)).
> Release naming is a deliberate rule: prerelease releases are branded
> **Elembra v<version>** (the Elembra preview line), while stable releases
> keep the **RustShare v<version>** name.

Combined tag matrix:

| Tag | Source | Stability | Use case |
|-----|--------|-----------|----------|
| `edge` | Latest `main` push | Unstable | Development, CI validation |
| `nightly-YYYY-MM-DD` | Dated `main` push | Unstable | Snapshot testing, bisecting |
| `sha-<gitsha>` | Every build | Unstable | Exact reproducibility |
| `X.Y.Z` | Stable tag (`vX.Y.Z`) | Stable | Production deployments |
| `X.Y.Z-<prerelease>` | Prerelease tag (`vX.Y.Z-alpha.N` / `-rc.N`) | Unstable preview | Version-only image tag + GitHub prerelease; never `latest` |
| `X.Y` | Rolling minor alias | Stable | Automatic patch uptake |
| `X` | Rolling major alias | Stable | Automatic minor uptake (post-1.0) |
| `latest` | Latest stable tag only | Stable | Quick start, never `main` |

**Rules:**

- `latest` is **only** moved by stable version tags. A `main` push or a
  prerelease tag never overwrites `latest`.
- Rolling aliases (`X.Y`, `X`) are updated on every stable release so users can choose their uptake cadence; prerelease tags never update them.
- Prerelease tags publish a **version-only** image tag plus the `sha-<gitsha>` tag, and create a GitHub prerelease (named `Elembra vX.Y.Z-<pre>`).

---

## Release Immutability

Release-channel immutability is **policy backed by pipeline enforcement and
repository controls**: the workflow enforces what it can, and the remainder is
guarded by tag protection rules and maintainer discipline.

- **Git version tags are never force-moved or deleted — policy + mitigation,
  not enforced by the workflow.** The release workflow itself never creates,
  moves, or deletes tags; tags are pushed by maintainers and must stay pinned
  to their original commit forever. The pipeline enforces what it can — a
  release run only ever accepts a tag whose commit it can verify against the
  commit it runs on, and a tag push against an already-released version is
  rejected by `validate-tag` regardless. The operational control is
  **repo-level tag protection rules on `v*`** (recommend enabling: no
  force-push, no deletion) plus org-admin discipline.
- **Docker version tags are never republished — enforced.** The version image
  tags (`X.Y.Z`, `X.Y.Z-<pre>`) are published only after the boot gate, and
  `validate-tag` refuses to release a version whose GitHub release already
  exists — via tag push, and via an honest `workflow_dispatch` (a dispatch
  whose tag does not point at the run commit is rejected). The rolling aliases
  (`X.Y`, `X`, `latest`) are moved by CI only as part of a successful stable
  release — never by a prerelease and never by a failed run (the manual alias
  retag in the [Rollback Procedure](#rollback-procedure) below remains an
  explicit operator exception).
- **Every release identifies exactly one source commit and one image digest.**
  The source commit is the Git tag, verified by the workflow against the
  commit it runs on; the image digest is the digest of the multi-arch
  candidate image built from that commit, and all released Docker tags point
  at it. Promotion re-tags the verified digest — it never rebuilds (a repair
  run may move the version's Docker tags to a rebuilt digest of the SAME
  commit).
- **A defective release is fixed by the next version of the same class, never
  by re-publishing the defective one.** A defective stable release is fixed by
  the next stable patch (`v0.7.0` → `v0.7.1`); a defective prerelease by the
  next prerelease version (`v0.8.0-alpha.1` → `v0.8.0-alpha.2`). The defective
  version's tags and digest stay as they were.

**Residual risk — force-moved tags.** The one path that can bypass the
pipeline checks is a force-moved tag: if someone with push access force-pushes
`v0.7.0` to a new commit and then dispatches that version, the dispatch guard
passes (the tag does point at the run commit) and the immutability check
passes (a release exists, so the run is treated as a repair). This requires
**two elevated actions** and is exactly what **repo-level tag protection rules
on `v*` (no force-push, no deletion)** prevent; org-admin discipline is the
last line of defense. Even then the release record is not silently rewritten:
the GitHub Release and its binary assets always remain those of the commit
they were originally published from.

**The only exception — remediation/repair runs.** A release that was published
from the correct commit but produced a broken artifact (e.g. a bad image or a
failed build) may be re-run via **`workflow_dispatch` only**: re-running the
SAME version at ITS OWN commit rebuilds the artifact from the same code and
re-runs the gate. This is the explicit repair mode (the v0.7.0 remediation
path). A repair run requires all of:

1. The version tag exists and points at the exact commit being dispatched
   (existing dispatch guard, issue #256), **and**
2. A GitHub release already exists for that version (repair is only meaningful
   for an already-released version), **and**
3. The trigger is `workflow_dispatch` — a tag push against an already-released
   version is a force-move/duplicate attempt and is rejected.

The decision is made in `validate-tag`, before anything is built, by the pure
`immutability_decision` function in `scripts/release-tag.sh`:

| Release exists | Event | Tag commit vs run commit | Decision |
|----------------|-------|--------------------------|----------|
| no | any | — | ALLOW |
| yes | `workflow_dispatch` | same | ALLOW (repair mode) |
| yes | `workflow_dispatch` | different | FAIL |
| yes | tag push | — | FAIL |

---

## Pre-release Validation

The automated pipeline accepts stable `vX.Y.Z` tags and SemVer prerelease
tags (`vX.Y.Z-alpha.N`, `vX.Y.Z-rc.N`, …): pushing either triggers
`release.yml`, which builds binaries, builds and gates a candidate image,
promotes the released Docker tags, and creates the GitHub Release (stable or
prerelease, per `scripts/release-tag.sh`). See the
[Promotion Pipeline](#promotion-pipeline) below for the ordering guarantees.
Prerelease image tags are version-only (`latest` and rolling aliases are never
moved); run the same validation checklist below against the exact commit to be
tagged before pushing any tag.

Before pushing a stable tag, the maintainer must complete the validation
checklist against the exact commit to be tagged:

1. **Backend gates**: `cargo fmt --all --check`, clippy (`-D warnings`),
   `cargo test --workspace --all-features --lib`, release build,
   `cargo sqlx prepare --workspace --check`, `cargo deny ... check`.
2. **Frontend gates**: `npm ci`, `npm run check`, `npm run lint`,
   `npm run test`, `npm run build`, `npm audit --audit-level=high`.
3. **Live-DB integration suite**: PostgreSQL (pgvector) + RustFS, migrations
   applied, `cargo test --workspace --all-features -- --ignored`.
4. **Clean deployment smoke**: boot the documented Compose stack from a clean
   state and run `./scripts/final-launch-smoke.sh`.
5. **Upgrade drill**: upgrade from the previous stable release against
   representative data; verify data survival and restart after migrations.
6. **Backup/restore drill**: `./scripts/backup-stack.sh` →
   `./scripts/verify-backup-bundle.sh` → `./scripts/run-restore-drill.sh` /
   `post-restore-smoke.sh`.
7. **Security suite**: tenant isolation, share authorization/revocation, CSRF,
   authentication, SSRF, Mail privacy/sanitization, object-storage credential
   isolation.

When a staging environment is available, also deploy the candidate to staging,
run the integration suites there, and verify migration behavior against a copy
of production data before tagging. If issues are found, fix them on `main`,
merge, and re-run the affected gates against the new candidate commit — then
tag the stable version.

---

## Promotion Pipeline

`release.yml` publishes images in strict order — the released tags are created
only after the candidate image has passed the boot gate:

1. **Build candidate** — the image is built (multi-arch, SBOM/provenance
   attestations, OCI labels) and pushed under the unadvertised candidate tag
   `candidate-<run_id>` only. The released tags are NOT attached during the
   build.
2. **Boot smoke test** — the candidate image is booted per platform (exec bit
   + dynamic linker checks, full exit-code taxonomy and glibc-marker scan).
3. **Promote** — only when the boot gate passed, `docker buildx imagetools
   create` re-tags the verified digest with the released tags (version, and
   for stable: `X.Y`, `X`, `latest`) plus `sha-<short>`. No rebuild happens:
   promotion only creates references to the digest the gate tested.
4. **SBOM / provenance** — SBOMs for the binaries and the image, and build
   provenance attestations, are generated from the same digest.
5. **Release** — the GitHub Release (stable or prerelease) is created from the
   tag.

A failed gate fails the job before step 3: **no released tags are created, no
`latest`/alias mutation happens, and no release is created.** The `candidate-*`
tags are transient audit artifacts (they record what was gated); they are not
advertised and may be deleted at any time.

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
| `x86_64-unknown-linux-gnu` | `rustshare-server-x86_64-unknown-linux-gnu` |
| `aarch64-unknown-linux-gnu` | `rustshare-server-aarch64-unknown-linux-gnu` |

These binaries are built by `release.yml` (job `build-binaries`) with the pinned
Rust toolchain (1.97.1) and cross-compilation toolchain for `aarch64`, then
consumed by the container-image build. Operators who prefer binaries over Docker
can download them directly from the release page.
