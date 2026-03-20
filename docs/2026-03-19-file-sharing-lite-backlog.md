# File Sharing Lite MVP Backlog

Date: 2026-03-19
Companion documents:

- `docs/2026-03-19-file-sharing-lite-mvp-spec.md`
- `docs/2026-03-19-file-sharing-lite-mvp-roadmap.md`
- `docs/2026-03-19-file-sharing-lite-architecture-spec.md`

Status: Draft Jira-style backlog with workstream ownership

## 1. Operating Model

This backlog assumes parallel execution across four main workstreams.

## Workstream A: Platform + Identity

Owner profile:

- senior backend/platform engineer

Mission:

- OIDC
- environments
- CI/CD
- operational guardrails

## Workstream B: Core Backend

Owner profile:

- senior backend engineer

Mission:

- data model
- uploads/downloads
- files/folders
- permissions
- sharing

## Workstream C: Web Frontend

Owner profile:

- frontend engineer

Mission:

- login UX
- file browser
- sharing UI
- admin policy UI

## Workstream D: Mobile

Owner profile:

- mobile engineer

Mission:

- OIDC mobile login
- upload/download
- offline files
- photo backup

## Shared lane: QA / Product Review

Owner profile:

- QA/product/shared founder review

Mission:

- acceptance validation
- scope discipline
- pilot readiness

## 2. Subagent Management Rules

To keep parallel work safe:

1. Platform and Backend define contracts first.
2. Web and Mobile never build against unstable, undocumented endpoints.
3. Every story must declare:
   - owner lane
   - dependencies
   - acceptance criteria
   - API/schema contract impact
4. No lane may introduce MVP scope expansion without explicit review.
5. Shared integration checkpoints happen at the end of every sprint.

## 3. Story Estimation Method

For planning, each story includes:

- `SP`: relative story points
- `Owner`: workstream owner
- `Depends on`: prerequisite story or epic

Sizing guide:

- `SP 1`: half day to one day
- `SP 2`: one to two days
- `SP 3`: two to three days
- `SP 5`: up to one week
- `SP 8`: one to two weeks or strong uncertainty

## 4. Epic List

- `EPIC-1`: Platform, environments, and OIDC
- `EPIC-2`: Core storage, uploads, and downloads
- `EPIC-3`: Files, folders, trash, and versioning
- `EPIC-4`: Sharing and permissions
- `EPIC-5`: Web application
- `EPIC-6`: Mobile application
- `EPIC-7`: Admin, audit, and notifications
- `EPIC-8`: Hardening, pilot, and release readiness

## 5. Detailed Backlog

## EPIC-1: Platform, Environments, and OIDC

### FS-101: Create environment strategy

- Owner: Workstream A
- SP: 3
- Depends on: none
- Description: Define local, staging, and production environment variables, secret handling, and service topology.
- Acceptance criteria:
  - environment config matrix exists
  - required secrets documented
  - local dev startup instructions work

### FS-102: Set up CI pipeline

- Owner: Workstream A
- SP: 3
- Depends on: FS-101
- Description: Add CI for backend tests, frontend checks, and mobile lint/test placeholders.
- Acceptance criteria:
  - backend pipeline runs on pull requests
  - frontend pipeline hooks exist even if app code is incomplete
  - failing checks block merge

### FS-103: Integrate OIDC for web

- Owner: Workstream A
- SP: 5
- Depends on: FS-101
- Description: Implement Authorization Code OIDC login and session creation for web.
- Acceptance criteria:
  - login redirects to IdP
  - callback creates session
  - user can log out
  - unauthorized routes redirect properly

### FS-104: Integrate OIDC for mobile

- Owner: Workstream D
- SP: 5
- Depends on: FS-103
- Description: Implement mobile login with Authorization Code + PKCE via system browser.
- Acceptance criteria:
  - login works on iOS and Android
  - refresh token stored securely
  - session restore works on app restart

### FS-105: First-login user provisioning

- Owner: Workstream B
- SP: 3
- Depends on: FS-103
- Description: Create local user on first successful OIDC login and update profile on subsequent logins.
- Acceptance criteria:
  - first login creates user
  - repeat login updates display name/email changes
  - suspended users cannot access system

### FS-106: Group claim mapping

- Owner: Workstream A
- SP: 3
- Depends on: FS-105
- Description: Map configured IdP group claims into local groups.
- Acceptance criteria:
  - claim mapping configurable
  - group membership updated on login
  - unknown groups handled safely

### FS-107: Break-glass admin account

- Owner: Workstream A
- SP: 2
- Depends on: FS-101
- Description: Provide emergency local admin login path for IdP outage scenarios.
- Acceptance criteria:
  - bootstrap admin can be created
  - usage is auditable
  - can be disabled in production if desired

## EPIC-2: Core Storage, Uploads, and Downloads

### FS-201: Create core database schema

- Owner: Workstream B
- SP: 5
- Depends on: none
- Description: Create schema for users, groups, folders, files, versions, shares, public links, upload sessions, audit events.
- Acceptance criteria:
  - migration applies cleanly
  - rollback strategy documented
  - key indexes included

### FS-202: Object storage integration

- Owner: Workstream B
- SP: 5
- Depends on: FS-201
- Description: Integrate S3-compatible object storage for immutable file blobs.
- Acceptance criteria:
  - object write/read/delete works
  - storage credentials are not hardcoded
  - integration tested locally

### FS-203: Upload session API

- Owner: Workstream B
- SP: 5
- Depends on: FS-202
- Description: Create resumable upload session initiation and completion flow.
- Acceptance criteria:
  - client can create session
  - upload session expires safely
  - completion validates metadata

### FS-204: Large-file resumable upload path

- Owner: Workstream B
- SP: 8
- Depends on: FS-203
- Description: Support resumable upload for large files using chosen protocol/strategy.
- Acceptance criteria:
  - interrupted upload can resume
  - duplicate finalize is safe
  - upload corruption is detected

### FS-205: File download endpoint

- Owner: Workstream B
- SP: 3
- Depends on: FS-202
- Description: Add authenticated single-file download behavior.
- Acceptance criteria:
  - access control enforced
  - content type correct
  - large file download works

### FS-206: Folder archive download

- Owner: Workstream B
- SP: 5
- Depends on: FS-205
- Description: Generate downloadable archive for folder contents asynchronously.
- Acceptance criteria:
  - archive job created
  - client can poll status
  - archive download expires safely

## EPIC-3: Files, Folders, Trash, and Versioning

### FS-301: Folder CRUD

- Owner: Workstream B
- SP: 5
- Depends on: FS-201
- Description: Create, rename, move, and delete folders.
- Acceptance criteria:
  - nested folder creation works
  - rename and move preserve metadata
  - delete respects trash model

### FS-302: File metadata CRUD

- Owner: Workstream B
- SP: 5
- Depends on: FS-203
- Description: Create and manage file metadata linked to stored blobs.
- Acceptance criteria:
  - file appears in folder listing after upload finalize
  - rename works
  - move works

### FS-303: Folder contents listing

- Owner: Workstream B
- SP: 3
- Depends on: FS-301, FS-302
- Description: List files and folders for any accessible folder.
- Acceptance criteria:
  - pagination supported
  - sort fields documented
  - permissions filter results correctly

### FS-304: Trash and restore

- Owner: Workstream B
- SP: 5
- Depends on: FS-301, FS-302
- Description: Implement soft delete, trash view, and restore for files/folders.
- Acceptance criteria:
  - deleted content appears in trash
  - restore returns to valid location
  - purge path documented

### FS-305: File versioning

- Owner: Workstream B
- SP: 5
- Depends on: FS-302
- Description: Store new version on replacement and expose version history.
- Acceptance criteria:
  - replace creates new version
  - version list available
  - restore previous version works

### FS-306: Filename search

- Owner: Workstream B
- SP: 3
- Depends on: FS-303
- Description: Implement filename/folder-name search over accessible content.
- Acceptance criteria:
  - search respects permissions
  - response time acceptable for MVP target

## EPIC-4: Sharing and Permissions

### FS-401: Define permission matrix

- Owner: Workstream B
- SP: 2
- Depends on: none
- Description: Formalize viewer/contributor/editor/manager actions.
- Acceptance criteria:
  - matrix documented
  - backend enforcement rules derive from matrix
  - product sign-off recorded

### FS-402: Internal user share API

- Owner: Workstream B
- SP: 5
- Depends on: FS-401, FS-303
- Description: Share file/folder with specific users.
- Acceptance criteria:
  - create share works
  - revoke works
  - update permission works

### FS-403: Group share API

- Owner: Workstream B
- SP: 5
- Depends on: FS-106, FS-401
- Description: Share folder with groups from IdP/local mapping.
- Acceptance criteria:
  - group share grants access to members
  - membership changes reflected safely

### FS-404: Permission inheritance engine

- Owner: Workstream B
- SP: 8
- Depends on: FS-402, FS-403
- Description: Resolve direct and inherited permissions with deterministic precedence.
- Acceptance criteria:
  - user share overrides group share
  - child override precedence documented
  - automated tests cover inheritance edge cases

### FS-405: Public link creation

- Owner: Workstream B
- SP: 5
- Depends on: FS-401, FS-302
- Description: Create public file/folder links with token hashing.
- Acceptance criteria:
  - link token not stored in plaintext
  - optional expiry works
  - optional password works

### FS-406: Upload-only public link

- Owner: Workstream B
- SP: 5
- Depends on: FS-405
- Description: Add folder link that accepts uploads without exposing read access.
- Acceptance criteria:
  - upload-only role enforced
  - uploaded files land in target folder
  - audit trail records source link usage

### FS-407: Share auditing

- Owner: Workstream G / Workstream B
- SP: 3
- Depends on: FS-402, FS-405
- Description: Record create/revoke/access share events.
- Acceptance criteria:
  - audit events written for internal shares
  - audit events written for public links
  - admin can query events later

## EPIC-5: Web Application

### FS-501: App shell and authenticated layout

- Owner: Workstream C
- SP: 3
- Depends on: FS-103
- Description: Build core app frame, navigation, auth-guarded routes.
- Acceptance criteria:
  - unauthenticated access redirects to login
  - app layout loads after sign-in

### FS-502: File browser page

- Owner: Workstream C
- SP: 5
- Depends on: FS-303
- Description: Build folder-aware file browser with list/grid support.
- Acceptance criteria:
  - navigate folders
  - file/folder listings render correctly
  - empty states exist

### FS-503: Upload UX

- Owner: Workstream C
- SP: 5
- Depends on: FS-203, FS-204
- Description: Multi-file upload UI with progress and retry/error states.
- Acceptance criteria:
  - progress visible
  - errors visible
  - multiple uploads supported

### FS-504: File actions UX

- Owner: Workstream C
- SP: 5
- Depends on: FS-301, FS-302, FS-304, FS-305
- Description: Rename, move, delete, restore, version history UI.
- Acceptance criteria:
  - all major actions available in UI
  - destructive actions confirmed
  - restore path usable

### FS-505: Share dialog

- Owner: Workstream C
- SP: 5
- Depends on: FS-402, FS-405, FS-406
- Description: Build internal share and public link management UI.
- Acceptance criteria:
  - user/group sharing works from dialog
  - public links can be created and revoked
  - expiry/password controls visible

### FS-506: Shared-with-me page

- Owner: Workstream C
- SP: 3
- Depends on: FS-402, FS-403
- Description: Show content shared to current user.
- Acceptance criteria:
  - shares visible by source and role
  - navigation into shared folders works

### FS-507: Admin sharing policy UI

- Owner: Workstream C
- SP: 3
- Depends on: FS-701
- Description: Configure share-link defaults and constraints.
- Acceptance criteria:
  - admin can set default expiry
  - admin can require passwords
  - non-admins cannot access page

### FS-508: Preview support

- Owner: Workstream C
- SP: 3
- Depends on: FS-205, FS-702
- Description: Preview image/pdf/text files in browser.
- Acceptance criteria:
  - image preview works
  - PDF preview works
  - text preview works

## EPIC-6: Mobile Application

### FS-601: Mobile app shell

- Owner: Workstream D
- SP: 3
- Depends on: FS-104
- Description: Create authenticated mobile shell and navigation.
- Acceptance criteria:
  - user can sign in and open app
  - session survives restart

### FS-602: Mobile file browser

- Owner: Workstream D
- SP: 5
- Depends on: FS-303
- Description: Browse folders and file metadata on mobile.
- Acceptance criteria:
  - navigation works
  - pull-to-refresh works
  - empty/error states exist

### FS-603: Upload from picker/share sheet

- Owner: Workstream D
- SP: 5
- Depends on: FS-203, FS-204
- Description: Upload files from device picker and OS share sheet.
- Acceptance criteria:
  - file picker upload works
  - share extension or equivalent path works
  - errors and retries visible

### FS-604: Download and offline files

- Owner: Workstream D
- SP: 5
- Depends on: FS-205
- Description: Download files and pin selected files/folders for offline use.
- Acceptance criteria:
  - download works
  - open-in works
  - offline items survive restart

### FS-605: Photo backup queue

- Owner: Workstream D
- SP: 8
- Depends on: FS-203, FS-204
- Description: Build one-way photo/video backup queue for selected albums/folders.
- Acceptance criteria:
  - user selects source album/folder
  - uploads continue when OS allows
  - retries are visible
  - duplicates skipped where possible

### FS-606: Photo backup settings

- Owner: Workstream D
- SP: 3
- Depends on: FS-605
- Description: Wi-Fi only, charging only, background behavior options.
- Acceptance criteria:
  - settings persist
  - queue respects settings

### FS-607: Mobile share-link access

- Owner: Workstream D
- SP: 3
- Depends on: FS-405
- Description: Open public links in mobile app/browser handoff cleanly.
- Acceptance criteria:
  - public links open correctly
  - password-protected links handled

## EPIC-7: Admin, Audit, and Notifications

### FS-701: Sharing policy API

- Owner: Workstream B
- SP: 3
- Depends on: FS-201
- Description: API for admin-configurable share defaults and constraints.
- Acceptance criteria:
  - public link rules configurable
  - policy enforced server-side

### FS-702: Preview generation worker

- Owner: Workstream A / Workstream B
- SP: 5
- Depends on: FS-203, FS-205
- Description: Background preview/thumbnail generation for supported file types.
- Acceptance criteria:
  - worker jobs created after upload
  - failures visible in logs
  - preview metadata stored

### FS-703: Audit event system

- Owner: Workstream B
- SP: 5
- Depends on: FS-201
- Description: Central event writer for auth, uploads, deletes, shares.
- Acceptance criteria:
  - common actions produce audit events
  - event schema documented

### FS-704: Notification records and email

- Owner: Workstream A / Workstream B
- SP: 5
- Depends on: FS-402, FS-405, FS-703
- Description: Create notification storage and email notification path.
- Acceptance criteria:
  - incoming share generates notification record
  - email send is asynchronous
  - email failure does not break share creation

### FS-705: Notification UI

- Owner: Workstream C
- SP: 3
- Depends on: FS-704
- Description: Show recent notifications in web app.
- Acceptance criteria:
  - read/unread state works
  - share notifications visible

## EPIC-8: Hardening, Pilot, and Release Readiness

### FS-801: End-to-end test suite

- Owner: Shared lane
- SP: 5
- Depends on: core vertical slices from Epics 1-7
- Description: Add E2E coverage for login, upload, share, restore, mobile smoke paths.
- Acceptance criteria:
  - critical web paths covered
  - backend API smoke tests exist
  - mobile smoke checklist documented

### FS-802: Rate limiting and abuse controls

- Owner: Workstream A / Workstream B
- SP: 3
- Depends on: FS-103, FS-405
- Description: Protect auth and public-link endpoints from abuse.
- Acceptance criteria:
  - auth throttling exists
  - public-link password attempts throttled

### FS-803: Backup and restore validation

- Owner: Workstream A
- SP: 3
- Depends on: FS-202, FS-703
- Description: Verify database and blob restore process.
- Acceptance criteria:
  - restore drill documented
  - data consistency check performed

### FS-804: Performance baseline

- Owner: Shared lane
- SP: 3
- Depends on: FS-204, FS-303, FS-405
- Description: Measure upload, list, and share-link flows against MVP baseline.
- Acceptance criteria:
  - baseline report exists
  - major bottlenecks identified

### FS-805: Security review

- Owner: Shared lane
- SP: 5
- Depends on: FS-405, FS-406, FS-802
- Description: Review token handling, permissions, uploads, and public-link behavior.
- Acceptance criteria:
  - review checklist completed
  - critical issues fixed or documented

### FS-806: Pilot deployment and support docs

- Owner: Workstream A / Shared lane
- SP: 3
- Depends on: all pilot-critical stories
- Description: Prepare runbook, support guide, known limitations, and pilot onboarding materials.
- Acceptance criteria:
  - admin guide exists
  - pilot user guide exists
  - known limitations documented honestly

## 6. Sprint-Oriented Suggested Execution

## Sprint 1

Primary focus:

- FS-101
- FS-102
- FS-201
- FS-401
- FS-501

Checkpoint:

- skeleton app and backend running

## Sprint 2

Primary focus:

- FS-103
- FS-105
- FS-202
- FS-301
- FS-302

Checkpoint:

- authenticated upload/download vertical slice possible

## Sprint 3

Primary focus:

- FS-203
- FS-205
- FS-303
- FS-502
- FS-503
- FS-601

Checkpoint:

- web file browser usable
- mobile login shell working

## Sprint 4

Primary focus:

- FS-204
- FS-304
- FS-305
- FS-402
- FS-405

Checkpoint:

- files usable end-to-end with public-link basics

## Sprint 5

Primary focus:

- FS-403
- FS-404
- FS-505
- FS-506
- FS-603
- FS-604

Checkpoint:

- internal sharing works
- mobile upload/download works

## Sprint 6

Primary focus:

- FS-406
- FS-703
- FS-704
- FS-605
- FS-606
- FS-507

Checkpoint:

- file-drop links
- audit trail
- photo backup MVP

## Sprint 7

Primary focus:

- FS-702
- FS-705
- FS-508
- FS-801
- FS-802

Checkpoint:

- preview support
- notifications
- hardening begins

## Sprint 8

Primary focus:

- FS-803
- FS-804
- FS-805
- FS-806

Checkpoint:

- pilot-ready release candidate

## 7. Dependency Rules Between Workstreams

### Platform -> Web/Mobile

Platform must deliver before parallel UI completion:

- auth callback contract
- token/session contract
- environment config

### Backend -> Web/Mobile

Backend must deliver before UI completion:

- folder contents contract
- upload session contract
- share APIs
- error model

### Shared acceptance rules

No frontend/mobile story is done until:

- API contract exists
- mock payload exists
- error states are handled

## 8. What I Would Tell Each Subagent to Work On

### Subagent A: Platform + Identity

Start with:

- FS-101
- FS-102
- FS-103
- FS-107

Then:

- FS-106
- FS-702
- FS-802
- FS-803

### Subagent B: Core Backend

Start with:

- FS-201
- FS-202
- FS-301
- FS-302

Then:

- FS-203
- FS-204
- FS-303
- FS-304
- FS-305
- FS-402
- FS-403
- FS-404
- FS-405
- FS-406
- FS-701
- FS-703

### Subagent C: Web

Start with:

- FS-501

Then after contracts:

- FS-502
- FS-503
- FS-504
- FS-505
- FS-506
- FS-507
- FS-508
- FS-705

### Subagent D: Mobile

Start with:

- FS-104
- FS-601

Then after upload/download contracts:

- FS-602
- FS-603
- FS-604
- FS-605
- FS-606
- FS-607

## 9. Scope-Protection Rules

Reject from MVP unless explicitly approved:

- desktop sync client
- WebDAV
- SAML
- collaborative docs
- full-text search
- antivirus product features
- multi-tenant admin

## 10. Definition of Ready

A story is ready only if:

- scope is clear
- owner is clear
- dependencies are clear
- acceptance criteria exist
- API/schema impact is known

## 11. Definition of Done

A story is done only if:

- implementation complete
- automated tests added where appropriate
- docs updated
- failure cases handled
- acceptance criteria passed

## 12. Final Recommendation

If I were directly managing the subagents, I would run:

- Platform + Backend in front
- Web and Mobile one sprint behind contracts
- QA/Hardening continuously from Sprint 4 onward

That gives the fastest path to a real MVP without chaos.
