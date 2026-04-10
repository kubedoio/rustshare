# RustShare

RustShare is a modern file-sharing and sync platform for technical teams and security-conscious organizations.

It is being designed as a serious alternative in the category of Nextcloud, Seafile, OpenCloud, and Dropbox-style systems, with a strong focus on:

- self-hosted and private-cloud deployment
- clear permissions and auditable sharing
- lightweight clients
- secure architecture
- clean integration with **RustChat**
- future-ready, permission-aware AI on top of authorized content

RustShare is not meant to be “just another sync tool.”
The product direction is to make files, sharing, governance, and collaboration work together without collapsing everything into one oversized platform.

---

## Project status

RustShare currently exists as an MVP and is being refined toward a stronger production design.

The current work focuses on:

- polishing the existing implementation instead of rewriting blindly
- identifying gaps between the MVP and the intended product direction
- enforcing clearer architecture and behavior contracts
- converging on a secure, multi-tenant, integration-friendly design

## Desktop client status

The current desktop client ships as a CLI plus background daemon under [apps/desktop](/Users/scolak/Projects/x/rustshare/apps/desktop).

What is real today:

- the live macOS path is the `rustshare-desktop` binary, currently version `0.3.0`
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

- [apps/desktop/docs/distribution/macos-client-installation.md](/Users/scolak/Projects/x/rustshare/apps/desktop/docs/distribution/macos-client-installation.md)
- [apps/desktop/docs/CLI_USAGE.md](/Users/scolak/Projects/x/rustshare/apps/desktop/docs/CLI_USAGE.md)
- [apps/desktop/docs/architecture/desktop-phase1-architecture.md](/Users/scolak/Projects/x/rustshare/apps/desktop/docs/architecture/desktop-phase1-architecture.md)

---

## Product direction

RustShare is being shaped around the following core ideas:

1. **Permission clarity beats feature count**  
   Sharing must be understandable, revocable, and testable.

2. **Auditability beats hidden magic**  
   Users and administrators must be able to understand what happened, who accessed what, and why.

3. **Object storage is for durable content**  
   Durable blobs and immutable versions belong in S3-compatible storage.

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

- **S3-compatible object storage** for durable file content and immutable versions
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

- `docs/adr/01-product-spec.md`  
  Product scope, target users, Phase 1 priorities, and product boundaries.

- `docs/adr/02-contract.md`  
  Behavioral contracts and acceptance rules that should be enforced through tests and validation.

- `docs/adr/03-design.md`  
  Architecture and system-design guidance for the intended RustShare direction.

- `docs/adr/0001-ADR.md`  
  Original ADR.

### Implementation and status docs

- `docs/STATUS.md` — Current project state and completion estimates
- `docs/FRONTEND_STATUS.md` — Frontend-specific maturity and capabilities
- `docs/PRODUCTION_READINESS.md` — Launch hardening checklist and remaining risks
- `docs/TESTING.md` — Deployment validation and manual browser testing guide
- `docs/TODOS.md` — Open and deferred engineering decisions
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

1. `03-design.md`
2. `01-product-spec.md`
3. `02-contract.md`
4. `0001-ADR.md`

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

RustShare deploys via Docker Compose. The frontend is built into the backend image and served by Axum, with nginx as the reverse proxy.

Quick start:

```bash
docker compose up -d
./test-deployment.sh
```

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for the full deployment guide, including environment variables, compose profiles, and the production checklist.

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
