# File Sharing Lite MVP Roadmap

Date: 2026-03-19
Companion document: `docs/2026-03-19-file-sharing-lite-mvp-spec.md`
Status: Draft planning document

## 1. Planning Intent

This roadmap translates the MVP specification into a delivery plan that is honest about:

- what must ship
- what should not ship in MVP-1
- what order reduces risk
- what team shape is realistic

This plan assumes we are building a focused file-sharing product with:

- web
- backend/API
- mobile clients
- OIDC SSO
- internal/public sharing
- mobile photo backup

It does **not** assume desktop sync in MVP-1.

## 2. Recommended Delivery Model

Recommended release structure:

1. Foundation and architecture setup
2. Core file platform
3. Sharing and permissions
4. Mobile upload and offline basics
5. Hardening and pilot release

Recommended team:

- 2 backend engineers
- 1 frontend engineer
- 1 mobile engineer
- 0.5 QA/design
- 0.5 platform/DevOps

## 3. Timeline Overview

## Scenario A: Recommended MVP

Calendar estimate: 16 weeks

### Phase 0: Discovery and product shaping

- duration: 2 weeks
- outcome: signed-off MVP scope, architecture decisions, API contracts, design baseline

### Phase 1: Core platform foundation

- duration: 3 weeks
- outcome: auth, data model, storage integration, upload framework, CI/CD, environments

### Phase 2: Core file operations

- duration: 4 weeks
- outcome: folders, files, uploads, downloads, trash, versioning, web browser UI

### Phase 3: Sharing and permission system

- duration: 3 weeks
- outcome: internal shares, public links, permission inheritance, audit trail, admin controls

### Phase 4: Mobile MVP

- duration: 3 weeks
- outcome: mobile login, browse, upload, download, offline files, photo backup

### Phase 5: Stabilization and pilot

- duration: 1 week
- outcome: security review, performance fixes, pilot readiness

## Scenario B: Hardened v1

Calendar estimate: 20 to 28 weeks

Adds:

- deeper test coverage
- better observability
- stronger mobile retry behavior
- admin polish
- migration/upgrade safety
- operational runbooks

## 4. Exit Criteria by Milestone

### Milestone 1: Architecture locked

Exit criteria:

- PostgreSQL schema approved
- object storage approach approved
- auth model approved
- API style approved
- mobile auth flow approved
- final MVP non-goals approved

### Milestone 2: Core file service works

Exit criteria:

- user can log in with OIDC
- user can browse root and nested folders
- user can upload, download, rename, move, delete
- trash/restore works
- file replacement creates version history

### Milestone 3: Sharing works

Exit criteria:

- internal user share works
- group share works
- public file link works
- public folder link works
- upload-only link works
- expiration/password rules work
- audit log captures share events

### Milestone 4: Mobile works

Exit criteria:

- user can log in on iOS and Android
- user can browse files
- user can upload from picker/share sheet
- user can mark files offline
- photo backup works for selected albums/folders
- failed uploads remain visible and retryable

### Milestone 5: Pilot-ready

Exit criteria:

- load test baseline completed
- backup/restore tested
- security review completed
- production deployment guide written
- known limitations documented

## 5. Epic Breakdown

## Epic A: Identity and SSO

Goal:

- enable secure user onboarding and session handling through OIDC

Stories:

- configure OIDC provider integration
- implement Authorization Code + PKCE for web
- implement Authorization Code + PKCE for mobile
- first-login user creation
- group claims ingestion
- admin break-glass account
- logout and token/session invalidation

Dependencies:

- none

Risks:

- IdP claim mapping inconsistencies
- mobile redirect/callback edge cases

Estimate:

- 3 to 4 person-weeks

## Epic B: User, Group, and Admin Policy Model

Goal:

- define principals, group membership, quotas, and high-level controls

Stories:

- user entity
- group entity
- group membership management
- quota model
- admin controls for public sharing policies
- admin controls for link expiry defaults and requirements

Dependencies:

- Epic A

Estimate:

- 2 to 3 person-weeks

## Epic C: Storage and Metadata Platform

Goal:

- create the durable storage foundation for files and metadata

Stories:

- PostgreSQL schema
- object storage integration
- immutable file-version storage pattern
- checksum generation
- upload session tracking
- retention of file metadata

Dependencies:

- none

Risks:

- wrong early storage abstractions
- large-file behavior

Estimate:

- 4 to 5 person-weeks

## Epic D: Upload and Download Engine

Goal:

- provide reliable transfer behavior for large and mobile uploads

Stories:

- multipart/resumable upload API
- upload finalization
- checksum verification
- signed download URLs or streamed download endpoint
- folder archive download
- retry-safe upload state machine

Dependencies:

- Epic C

Risks:

- interrupted upload handling
- archive generation performance

Estimate:

- 4 to 6 person-weeks

## Epic E: File and Folder Operations

Goal:

- enable a basic but complete personal/team file browser

Stories:

- create folder
- rename file/folder
- move file/folder
- soft delete
- restore
- list contents
- sort/filter by name/date/size
- file metadata view
- version history list

Dependencies:

- Epic C
- Epic D

Estimate:

- 4 to 5 person-weeks

## Epic F: Sharing and Permission System

Goal:

- implement the core collaborative value of the product

Stories:

- share file to user
- share folder to user
- share folder to group
- revoke share
- edit share permissions
- inherited permissions
- public file link
- public folder link
- password-protected link
- expiring link
- upload-only file drop link

Dependencies:

- Epic B
- Epic E

Risks:

- inheritance correctness
- edge cases around delete/move with inherited access

Estimate:

- 6 to 8 person-weeks

## Epic G: Audit and Notification Basics

Goal:

- provide operational traceability and minimum user awareness

Stories:

- audit events for auth, upload, delete, share create/revoke
- activity list in UI
- email notification for incoming share
- admin audit search/filter

Dependencies:

- Epic A
- Epic F

Estimate:

- 2 to 3 person-weeks

## Epic H: Web Application

Goal:

- deliver a focused browser-based file experience

Stories:

- app shell
- login
- file browser
- folder navigation
- upload UI
- drag-and-drop
- preview for image/pdf/text
- share dialog
- shared-with-me page
- settings page
- admin policy page

Dependencies:

- backend APIs from Epics A-F

Estimate:

- 6 to 8 person-weeks

## Epic I: Mobile Clients

Goal:

- deliver light native mobile access with backup behavior

Stories:

- OIDC login
- file browser
- upload from picker
- upload from share sheet
- file download/open-in
- offline file pinning
- upload queue
- photo/video backup selection
- Wi-Fi only / charging-only options
- background retries where OS permits

Dependencies:

- Epic A
- Epic D
- Epic E
- Epic F

Risks:

- iOS background constraints
- Android vendor-specific behavior

Estimate:

- 10 to 14 person-weeks

## Epic J: Platform and Operations

Goal:

- make the system deployable and supportable

Stories:

- CI/CD
- staging/prod environments
- metrics/logging/tracing baseline
- secret management
- database backup
- object storage backup strategy
- deployment guide
- incident runbook

Dependencies:

- parallel to most epics

Estimate:

- 3 to 5 person-weeks

## 6. Sequencing Plan

### Weeks 1-2

- product decisions
- architecture decisions
- API contracts
- data model
- UX wireframes

### Weeks 3-5

- OIDC integration
- user/group model
- object storage
- metadata schema
- upload session framework
- environment/CI setup

### Weeks 6-9

- file browser APIs
- file/folder CRUD
- upload/download
- trash/restore
- versioning
- first complete web vertical slice

### Weeks 10-12

- internal shares
- public links
- upload-only links
- permission inheritance
- audit events
- shared-with-me UI

### Weeks 13-15

- mobile app vertical slice
- photo backup
- offline access
- upload queue and retry behavior

### Week 16

- stabilization
- bug fixing
- pilot preparation

## 7. Testing Strategy

## Backend

Required:

- unit tests for permissions and sharing rules
- API integration tests
- upload/download tests
- auth/session tests
- migration tests

## Web

Required:

- component tests for core interactions
- e2e tests for login, upload, share, revoke, restore

## Mobile

Required:

- auth flow tests
- upload queue tests
- offline file tests
- photo backup scenario tests

## Cross-cutting

Required:

- performance tests for large uploads
- soak tests for repeated upload retries
- security review of public links

## 8. Pilot Plan

Recommended pilot size:

- 20 to 50 users
- one real IdP
- one mobile-heavy user group
- one admin team

Pilot success criteria:

- no critical data-loss bugs
- no share-permission escalation bugs
- photo backup works for majority of pilot users
- large upload failure rate remains acceptably low
- support burden is manageable

## 9. Honest Resource Warnings

### If the team is smaller

With only:

- 1 backend/full-stack engineer
- 1 mobile engineer

expect:

- 24+ weeks for MVP
- higher integration risk
- less hardening

### If desktop sync is added

Do not "sneak it in".

It should become a named program with its own milestones:

- local indexing
- filesystem watchers
- conflict handling
- selective sync
- client updater
- recovery workflows

That is a separate major track.

## 10. Staffing Recommendation by Phase

### Early phase

- backend lead
- product-minded frontend/full-stack
- platform support part-time

### Mid phase

- add dedicated mobile engineer
- add QA support

### Late phase

- add operational hardening focus
- add security review if handling external users/public links

## 11. Definition of Done for MVP-1

MVP-1 is done only when:

- OIDC SSO works on web and mobile
- users can upload and download reliably
- users can share files/folders internally
- users can create expiring/password-protected public links
- admins can control sharing policy
- mobile photo backup works in real usage
- audit trail exists
- docs and operational runbooks exist

MVP-1 is **not** done when:

- only happy-path demos work
- mobile uploads fail silently
- sharing rules are ambiguous
- public links lack strong policy control
- production operations are undocumented

## 12. Recommended Post-MVP Backlog

Top post-MVP candidates:

1. group-sharing polish
2. better preview coverage
3. admin reporting
4. push notifications
5. WebDAV compatibility
6. desktop sync client
7. SAML

## 13. Final Recommendation

If speed and probability of success matter most:

- keep MVP to web + mobile + OIDC + core sharing
- do not add desktop sync yet
- do not add SAML yet
- do not add full-text search

That is the version most likely to reach a real pilot without collapsing under hidden complexity.
