# Elembra

**Open-source sovereign business memory workspace for durable team knowledge, files, notes, Chat, and permission-aware AI workflows.**

> The GitHub repository is still named `rustshare`; **Elembra** is the current product and architecture name.

> [!NOTE]
> RustShare is currently in **Public Preview**: early, actively evolving, and not yet intended for production use. Feedback, testing, and first contributions are welcome.

---

[Getting Started](#getting-started) • [Community](#community) • [Deployment](#deployment)

## Public Preview Status

RustShare is currently in **Public Preview**. The codebase is early, actively evolving, and open for feedback, testing, beta users, and first contributors.

It should not be treated as finished, production-ready, or enterprise-ready yet.

### Current focus

RustShare currently exists as an MVP and is being refined toward a stronger production design. The current work focuses on:

- polishing the existing implementation instead of rewriting blindly
- identifying gaps between the MVP and the intended product direction
- enforcing clearer architecture and behavior contracts
- converging on a secure, multi-tenant, integration-friendly design

## How to Report Useful Feedback

The fastest way to help us improve RustShare is to report what you actually tried and what happened.

A useful bug report or feedback item usually includes:

- **What you were trying to do** — the task, feature, or workflow you were exploring
- **What happened instead** — the error, unexpected behavior, or missing capability you saw
- **What you expected to happen** — the behavior or result you were hoping for
- **Operating system and setup method** — for example, Docker Compose, local dev build, or the macOS desktop CLI
- **Safe error messages** — logs, stack traces, or screenshots are welcome, but please remove passwords, tokens, private URLs, customer data, and other sensitive values first

For questions and general support, see [SUPPORT.md](SUPPORT.md). For development setup and contribution guidelines, see [CONTRIBUTING.md](CONTRIBUTING.md). For contributor and AI-agent workflow guidance, see [`AGENTS.md`](AGENTS.md) and [`docs/agent-guides/`](docs/agent-guides/).

## Security and Sensitive Data

Please do not post passwords, access tokens, private URLs, customer data, confidential logs, or other sensitive information in public GitHub issues.

If you believe you found a security problem, please see [SECURITY.md](SECURITY.md) to report it privately.

## What this is

RustShare is a durable memory and artifact layer for technical teams that need control over their documents, files, notes, diagrams, and long-lived operational knowledge.

It focuses on:

- durable company memory
- files, notes, diagrams, and technical artifacts
- markdown-based knowledge work
- meeting notes and decision records
- self-hosted deployment
- audit-friendly artifact storage
- API-driven integrations
- future permission-aware AI memory foundations

## What this is not

RustShare is not positioned as a Dropbox clone, a Nextcloud clone, or a generic file-sharing app.

It is not presented as a fully mature enterprise content platform yet. It is a public-preview infrastructure product for teams that are comfortable evaluating and operating self-hosted software.

![CI](https://github.com/kubedoio/rustshare/actions/workflows/ci.yml/badge.svg)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.97.1-orange.svg)](rust-toolchain.toml)

---

## Getting Started

### Quick Start (Docker Compose)

```bash
git clone https://github.com/kubedoio/rustshare.git
cd rustshare
cp .env.example .env
# REQUIRED: .env.example ships with empty secrets and the backend refuses to
# start without them. Generate strong secrets (this edits .env in place):
./scripts/pre-flight.sh
docker compose up -d
```

The first build compiles both the frontend and the backend, so it can take several minutes. Wait for the backend container to become healthy (`docker compose ps`), then visit `http://localhost`.

This Docker Compose quickstart has been validated on Ubuntu 22.04 LTS, Ubuntu
24.04 LTS, and Debian 12.

> **Admin password — record it immediately.** Unless you set `RUSTSHARE_ADMIN_PASSWORD` in `.env` before first start, the backend generates a random admin password ONCE at first boot and writes it to a bootstrap file inside the backend container. Retrieve it right away:
>
> ```bash
> scripts/read-bootstrap-password.sh "$(docker compose ps -q backend)"
> ```
>
> The bootstrap file lives in container-local storage and does **not** survive container recreation — after `docker compose down` / `--force-recreate`, an unrecorded auto-generated password is unrecoverable. For a durable credential, set `RUSTSHARE_ADMIN_PASSWORD` in `.env` before the first `docker compose up -d` (an empty value is treated as unset and triggers auto-generation).

> For validation, run `./scripts/final-launch-smoke.sh`. For production deployment details and first-start troubleshooting, see [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

### Development Setup

See [CONTRIBUTING.md](CONTRIBUTING.md) for full development setup, test commands, and contribution guidelines.

## Desktop client status

The current desktop client ships as a CLI plus background daemon under [apps/desktop](apps/desktop).

What is real today:

- the live macOS path is the `rustshare-desktop` binary, currently version `0.4.0`
- login is pairing-first, with an explicit `--token` fallback for admin and debugging workflows
- sync roots mirror their configured remote subtree, including directory structure and empty directories
- root `/` is supported as a full-account mirror
- broken remote downloads are quarantined per path so one stale server record does not stall the whole root
- stale remote metadata can be inspected with `sync doctor` and cleaned with `sync cleanup-remote`
- zero-byte files are synced as normal files, they are no longer skipped and re-uploaded forever

What is not shipped yet:

- a polished `.app` bundle
- notarized drag-and-drop macOS distribution
- a finished GUI shell on top of the sync daemon

If you want to build and run the current client, start here:

- [apps/desktop/docs/distribution/macos-client-installation.md](apps/desktop/docs/distribution/macos-client-installation.md)
- [apps/desktop/docs/CLI_USAGE.md](apps/desktop/docs/CLI_USAGE.md)
- [apps/desktop/docs/architecture/desktop-phase1-architecture.md](apps/desktop/docs/architecture/desktop-phase1-architecture.md)

---

## Product direction

RustShare is being shaped around the following core ideas:

1. **Permission clarity beats feature count**  
   Sharing must be understandable, revocable, and testable.

2. **Auditability beats hidden magic**  
   Users and administrators must be able to understand what happened, who accessed what, and why.

3. **Object storage is for durable, verifiable content**
   Durable blobs and immutable versions belong in S3-compatible storage under content-addressed `blobs/{sha256}` keys, with backend-mediated integrity checks.

4. **Metadata and permissions stay explicit**  
   Namespace, grants, shares, public links, and effective access cannot be left implicit.

5. **Chat integration must feel native, but remain bounded**  
   RustChat should integrate deeply with RustShare without becoming an authorization shortcut.

6. **AI must be permission-aware**  
   AI features should help users search, summarize, and query files they already have access to — not create a second unauthorized access path.

---

## Intended target users

RustShare is primarily aimed at:

- technical SMBs
- platform teams
- MSPs
- internal IT departments
- regulated or security-conscious mid-market organizations
- teams that want operational control and self-hosting options

Phase 1 is **not** optimized for:

- generic consumer cloud storage
- live office-suite replacement
- broad content publishing workflows
- highly autonomous AI actions over file estates

---

## High-level architecture

RustShare is moving toward a **hybrid, object-store-centered, service-oriented architecture**.

### Core model

- **S3-compatible object storage** for content-addressed durable file content and immutable versions
- **explicit metadata and permission authority** for namespace, shares, and access decisions
- **event-driven projections** for search, previews, notifications, and AI indexing
- **shared identity + bounded integration** with RustChat
- **Rust** for core backend services

### Why this direction

This model keeps the platform practical:

- scalable blob durability without forcing a filesystem model onto users
- explicit permission behavior
- clear multi-tenant reasoning
- a safer integration model for chat and future ecosystem features
- AI as an additive layer, not a core dependency

---

## Phase 1 scope

RustShare Phase 1 is intended to deliver a file platform people can actually use every day.

### Required capabilities

- tenant/workspace model
- OIDC-based identity
- user and group permissions
- file and folder CRUD
- upload and download
- rename, move, delete, restore
- version history
- internal shares
- public links with clear capability modes
- “shared with me”
- markdown notes with editor, autosave, and public sharing
- desktop-usable web UI
- lightweight sync/client flows
- device onboarding and pairing
- audit visibility for critical actions
- backup and restore path
- baseline RustChat integration

### Explicit non-goals for Phase 1

- office suite replacement
- plugin marketplace
- autonomous AI write/delete workflows
- over-engineered multi-region complexity
- speculative roadmap features that do not improve daily usability

---

## RustChat integration

RustShare is designed to integrate with RustChat as a native-feeling file layer.

That means:

- shared identity
- consistent user and group semantics
- permission-checked file references in chat
- previews/unfurls that respect access control
- no shared database between the systems
- no bypass of RustShare authorization through chat

RustChat should benefit from RustShare, but RustShare must remain installable and valuable on its own.

---

## AI direction

AI is part of the product direction, but it is not the product core.

The early AI scope is intentionally practical:

- ask this file
- ask this folder
- permission-aware semantic search
- summaries
- metadata extraction
- related-file discovery

Deferred AI areas include:

- autonomous permission changes
- autonomous deletion or retention actions
- unconstrained “chat with everything” behavior
- AI as a requirement for core usability

---

## Repository documentation map

This repository includes design and planning documents that define the target shape of the product.

### Core documents

- `docs/adr/0001-ADR.md`  
  Architecture Decision Records index and original ADR.

### Implementation and status docs

- `docs/STATUS.md` — Current project state and completion estimates
- `docs/FRONTEND_STATUS.md` — Frontend-specific maturity and capabilities
- `docs/PRODUCTION_READINESS.md` — Launch hardening checklist and remaining risks
- `docs/TESTING.md` — Deployment validation and manual browser testing guide
- `docs/DESIGN.md` — Design system tokens, typography, colors, and UX rules
- `docs/SPEC.md` — Notes MVP-1 implementation specification
- `docs/ARCHITECTURE_NOTES.md` — Notes MVP-1 key architectural decisions

### Desktop client docs

- `apps/desktop/docs/distribution/macos-client-installation.md` — build, install, pairing, daemon lifecycle, and troubleshooting for the current macOS CLI client
- `apps/desktop/docs/CLI_USAGE.md` — command reference for `rustshare-desktop`
- `apps/desktop/docs/architecture/desktop-phase1-architecture.md` — component map for the CLI, daemon, and shared sync engine
- `apps/desktop/docs/architecture/desktop-phase1-runtime-view.md` — what the client actually does at startup, during steady-state sync, and during recovery
- `apps/desktop/docs/distribution/build-and-package.md` — current internal packaging flow for versioned desktop artifacts

---

## How to use these docs with an LLM

Recommended order:

1. `docs/DESIGN.md`
2. `docs/SPEC.md`
3. `docs/adr/0001-ADR.md`

Then instruct the LLM to:

- inspect the current implementation
- compare it against the docs
- produce a gap analysis
- classify each important requirement as:
  - implemented
  - partial
  - missing
  - conflicting
- implement the missing pieces incrementally
- add or update tests for behavioral contracts
- avoid rewriting valid working parts without cause

---

## Engineering guidance

When working on RustShare, prefer the following mindset:

- do not overbuild
- do not chase generic enterprise bloat
- do not mistake architectural purity for product progress
- keep the client experience lightweight
- preserve valid MVP behavior where it already fits the design
- close the most important user-facing and architectural gaps first
- make permission behavior explicit and testable
- make integrations clean rather than clever

---

## What success looks like

RustShare succeeds when it becomes:

- easy enough for daily use
- strict enough for serious teams
- clear enough to trust
- modular enough to integrate
- lightweight enough to deploy and operate
- extensible enough to grow into AI-assisted, chat-connected workflows later

---

## Deployment

For production deployment guides, TLS setup, backup/restore procedures, and troubleshooting, see [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

---

## Current priority

The current priority is not to add every possible feature.

The priority is to make the existing MVP converge toward:

- a coherent product shape
- clean sharing behavior
- solid tenant and permission boundaries
- a dependable file workflow
- a maintainable architecture
- an implementation that can be safely improved by human engineers and LLM-assisted development

## Application Architecture

Elembra is composed of first-party **Applications** with explicit ownership and
integration contracts. The former product-level Module abstraction and
`/modules/...` routes were removed in the one-time Module → Application cutover.

### Current product rules

- Application manifests and the registry determine enabled product capabilities.
- Product routes use `/apps/...`; registry/configuration APIs use
  `/api/v1/applications/...`.
- Files, Notes, Chat, Memory and other Applications keep explicit data and
  authorization ownership.
- Cross-Application references use `ResourceRef` and are reauthorized at the
  owning Application before content is materialized.
- Durable Integration Events carry asynchronous cross-Application effects.
- Buzz remains the independent signed communication engine behind Elembra Chat;
  Elembra does not read Buzz private database tables or maintain a second Chat ACL.
- Templates remain reusable creation patterns where applicable, but they are not
  a replacement for the Application ownership model.
- Disabling an Application must not delete its user data.

### Canonical architecture references

- `docs/architecture/elembra-platform.md` — current platform architecture.
- `docs/specs/application-manifest-v1alpha1.md` — Application manifest contract.
- `docs/implementation/resource-ref-source-authorization.md` — ResourceRef and
  source-authorization implementation.
- `docs/runbooks/customer-alpha.md` — current controlled Alpha operational contract.
- `docs/releases/customer-alpha-gate.yaml` — machine-readable release evidence and
  GO/NO-GO state.

Older Module ADRs/specifications are retained only as pre-cutover historical
records and are explicitly marked superseded.

## Community

- [Contributing](CONTRIBUTING.md) — How to set up your dev environment, run tests, and submit PRs
- [Support](SUPPORT.md) — Where to ask questions and report bugs
- [Security Policy](SECURITY.md) — How to report vulnerabilities
- [Code of Conduct](CODE_OF_CONDUCT.md) — Expected behavior in the community
- [Governance](GOVERNANCE.md) — How the project is run
- [Roadmap](ROADMAP.md) — What's planned and when
- [Changelog](CHANGELOG.md) — Release history and what's new
- [Release Process](docs/release-process.md) — How releases are cut and published
- [Upgrading](docs/upgrading.md) — How to upgrade a running deployment
- [License](LICENSE) — Apache 2.0
