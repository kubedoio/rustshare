# File Sharing Lite MVP Specification

Date: 2026-03-19
Status: Draft specification based on current official-product research

## 1. Executive Summary

This specification defines a first MVP for a self-hosted file sharing product in the same problem space as Seafile and Nextcloud, but deliberately narrower:

- file storage
- upload/download
- internal and public sharing
- permissions
- SSO
- light mobile clients with photo backup and offline access

This MVP explicitly does **not** attempt to match the full scope of mature platforms that have been evolving for more than a decade.

The recommended first release is:

- web application
- API/backend
- Android and iOS mobile clients
- OIDC-based SSO
- RustFS-backed primary file service

The recommended first release is **not**:

- a full groupware suite
- an Office/Docs platform
- a federated cloud
- a virtual drive product
- a Seafile-class desktop sync engine on day one

## 2. Research Summary

### What mature tools actually include

Official documentation shows that Nextcloud and Seafile both cover much more than "basic file sharing".

Nextcloud includes:

- internal user/group shares
- public links
- upload-only "file drop" shares
- expiration and password controls
- desktop and mobile sync clients
- OIDC support
- WebDAV access
- broader ecosystem features outside file sharing

Seafile includes:

- desktop sync
- selective sync
- mobile apps with photo backup
- public links
- granular folder permissions
- OAuth support in community docs
- SAML/Shibboleth options
- many advanced enterprise features outside the narrow MVP

### Research-based product conclusion

If we want a "Seafile-lite / Nextcloud-lite" product, the safe path is to keep only the part that is repeatedly central in both products:

1. identity
2. personal/team files
3. web upload/download/share
4. permissioned folder sharing
5. public links
6. mobile browsing + photo backup

Everything else should be treated as phase 2+.

## 3. Product Goal

Build a focused file-sharing system for teams and small organizations that need:

- secure storage
- simple sharing
- basic role-based permissions
- SSO
- mobile photo/file upload
- dependable download/sync of selected content

## 4. Product Positioning

### In scope for MVP

- file and folder storage
- sharing inside the organization
- public share links
- light mobile sync behavior
- admin controls for sharing and identity

### Out of scope for MVP

- chat, calendar, contacts, mail
- Office editing and collaborative documents
- wiki / knowledge base
- federation between servers
- external storage connectors
- client-side encrypted libraries
- advanced DLP / classification / legal hold
- virtual drive / on-demand desktop filesystem integration
- Seafile-style fine-grained custom permission builders
- Nextcloud-style app ecosystem

## 5. Target Users

### Primary

- small and medium teams that need Dropbox-like internal file sharing with self-hosting or private-cloud deployment
- organizations that already use an IdP such as Keycloak, Okta, Entra ID, Authentik, or Google Workspace via OIDC

### Secondary

- departments that mainly need mobile photo upload, field uploads, and secure file delivery

## 6. Honest MVP Recommendation

### Recommended MVP boundary

The first MVP should include:

- web app
- backend/API
- iOS + Android clients
- OIDC SSO
- internal shares
- public links
- upload-only link shares for folders
- basic audit trail

### Important cut

Do **not** build a native desktop sync client in MVP-1 unless it is an absolute business requirement.

Reason:

- desktop sync, selective sync, conflict handling, local indexing, background transfer, virtual drive semantics, OS integration, and recovery flows are a separate product area
- Seafile and Nextcloud both have years of client behavior and edge-case handling behind those features

If desktop sync is mandatory for launch, add significant time and staffing. See estimates below.

## 7. Core Functional Requirements

### 7.1 Identity and Authentication

Required:

- OIDC login for web
- OIDC login for mobile using Authorization Code + PKCE
- IdP-driven user provisioning on first login
- group claim ingestion from IdP
- session/token revocation on logout
- optional local bootstrap admin account for break-glass administration

Recommended MVP rule:

- support **OIDC only** in MVP-1

Why:

- Nextcloud officially supports OIDC via the `user_oidc` app and can validate bearer tokens for API requests
- Seafile officially documents OAuth/OAuth2-style login in community docs, while SAML support is more configuration-heavy and partly positioned in Pro docs
- OIDC is much better aligned with modern web and mobile login flows than SAML

Inference:

- SAML should be phase 2 unless a signed enterprise deal requires it
- many enterprise IdPs can bridge SAML upstream and expose OIDC to apps, so supporting OIDC first is the cleanest MVP decision

### 7.2 User and Group Model

Required:

- user
- group
- organization-wide admin
- optional group manager role later

User attributes:

- id
- email
- display name
- avatar URL optional
- active/suspended status
- groups
- storage quota

### 7.3 File and Folder Model

Required:

- hierarchical folders
- files with metadata
- object-storage-backed content
- soft delete / trash
- restore from trash
- basic file versioning for replacements/overwrites

File metadata:

- id
- parent folder id
- owner id
- original filename
- normalized filename
- mime type
- size
- checksum
- storage key
- replication state
- created at
- updated at
- current version

### 7.4 Upload and Download

Required:

- single-file upload
- multi-file upload
- folder upload in web where browser supports it
- resumable large uploads
- download single file
- download folder as archive
- success response immediately after primary RustFS write

Required storage rule:

- RustFS is the primary blob store
- the backend writes to RustFS synchronously during upload/finalize
- cross-location replication is asynchronous and must not block the user response
- replication progress is tracked as metadata, separate from upload success

Recommendation:

- use resumable upload protocol support such as TUS or an equivalent multipart-upload abstraction

Honest note:

- large-file upload reliability is one of the highest-risk areas of this product
- "basic upload" is easy; "resume correctly across flaky mobile networks" is not

### 7.5 Sharing

#### Internal shares

Required:

- share file to user
- share folder to user
- share folder to group
- revoke share
- update share permission
- show inherited access

#### Public shares

Required:

- public link for file or folder
- optional password
- optional expiration date
- optional download disable flag for preview-only experiences only if previews exist
- upload-only folder link ("file drop")

### 7.6 Permission Model

Use a simple permission model for MVP.

Recommended internal roles:

- `viewer`: list, preview, download
- `contributor`: viewer + upload into shared folder
- `editor`: contributor + rename/move/delete within granted scope
- `manager`: editor + manage shares on that resource

Recommended public-link roles:

- `public_viewer`
- `public_uploader`

Do **not** implement Seafile-style atomic custom permission composition in MVP.

Inheritance rules:

- permissions inherited from parent folder
- explicit child permission overrides parent permission
- deny is not needed in MVP-1
- no invisible sub-folder permission in MVP-1

### 7.7 Search

Recommended MVP decision:

- filename and folder-name search only

Do not build:

- full-text document indexing
- OCR search
- metadata search engines

### 7.8 Web Application

Required screens:

- login / SSO
- file browser
- folder contents
- upload flow
- share dialog
- shared-with-me
- recent activity
- admin sharing policy screen
- user profile/settings

Required actions:

- create folder
- rename
- move
- delete
- restore from trash
- preview common file types
- copy link
- revoke link

Recommended previews for MVP:

- images
- PDF
- plain text
- audio/video if browser-native handling is easy

### 7.9 Mobile Clients

The mobile clients should be intentionally light.

Required:

- OIDC login via system browser + PKCE
- file browser
- upload from share sheet
- upload from camera/file picker
- download and open in other apps
- mark file or folder for offline access
- camera/photo/video backup
- retry queue for failed uploads
- upload status screen

Recommended MVP mobile sync behavior:

- **one-way** auto-upload from selected local sources to server
- **server-to-device** download only for explicitly offline-marked content

Do **not** promise full bi-directional arbitrary filesystem sync on mobile in MVP.

Reason:

- iOS and Android background execution rules make broad filesystem sync much harder than desktop sync
- photo backup and selected uploads are the safer MVP promise

Photo backup requirements:

- select one or more device folders/albums
- upload photos and videos in background where OS permits
- upload on Wi-Fi only toggle
- upload while charging toggle
- skip duplicates by checksum + size where possible
- preserve capture timestamp in metadata
- user-visible retry/error queue

### 7.10 Notifications

Required:

- in-app notification for share received
- email notification for share received
- email notification for public link created optional

Not required in MVP:

- rich push event stream
- websocket live collaboration events across all clients

Polling is acceptable in MVP.

## 8. Non-Functional Requirements

### Performance

- p95 metadata-list API under 300 ms for ordinary folder views under normal load
- first byte of download should start quickly for small files
- upload resume must survive network interruptions

### Scalability

Design target for MVP:

- 1,000 active users
- 10 million metadata records
- object storage as the file-content layer

This is a planning target, not a benchmark guarantee.

### Availability

Target:

- single-region deployment
- stateless app servers
- managed PostgreSQL or equivalent HA setup
- managed S3-compatible storage or proven MinIO setup

### Security

Required:

- TLS everywhere
- secure cookies / token storage
- short-lived API access tokens
- refresh token rotation
- audit logs for login, upload, delete, share create/revoke
- signed public share tokens
- checksum validation on upload
- malware scan hook as extension point, but scanning itself can be phase 2

Not required in MVP:

- end-to-end encrypted libraries
- advanced policy engines
- legal hold / eDiscovery

## 9. API and Architecture Specification

## 9.1 Recommended Architecture

Use a **modular monolith** for MVP.

Required components:

- API server
- background worker
- PostgreSQL
- RustFS as the primary strongly consistent S3-compatible object store
- optional Redis for queues/caches if needed

Do **not** start with:

- microservices
- CQRS/event sourcing
- custom block-level sync engine

Honest reasoning:

- those choices increase implementation and operational risk without being required for the first useful release

### 9.2 Domain Modules

- identity
- users/groups
- files/folders
- uploads/downloads
- sharing
- permissions
- notifications
- audit logs
- admin policy

### 9.3 Storage Model

Metadata in PostgreSQL:

- users
- groups
- group_members
- folders
- files
- file_versions
- shares
- public_links
- access_policies
- upload_sessions
- audit_events

File blobs in RustFS primary storage:

- immutable object per uploaded content version

Recommendation:

- use content hashing for dedupe opportunities later
- but do not over-invest in global dedupe for MVP

### 9.4 Sync Semantics

Recommended MVP semantics:

- server is source of truth
- clients sync by item metadata + version/etag
- whole-file upload on change
- resumable upload for interrupted transfers
- conflict handling through version mismatch detection

Do **not** build Seafile-style block sync or LAN sync in MVP.

### 9.5 Public API Surface

Required APIs:

- auth/session
- me/profile
- groups
- folders CRUD
- files CRUD
- upload session
- download
- internal shares
- public links
- notifications
- audit list for admins

Optional but useful:

- WebDAV compatibility layer in phase 1.5 or phase 2

Honest recommendation:

- do not promise WebDAV in MVP unless there is a strong integration need
- WebDAV increases compatibility value, but also expands edge-case handling substantially

## 10. Admin Features for MVP

Required:

- enforce OIDC login
- configure allowed share types
- enable/disable public links
- require password on public links
- enforce max expiration
- set default expiration
- storage quota per user or group
- suspend user
- review audit events

## 11. Explicit Non-Goals for MVP

These should be stated clearly to avoid scope creep:

- collaborative document editing
- Nextcloud federation equivalent
- Seafile virtual drive equivalent
- custom metadata system
- granular sub-folder invisibility rules
- desktop shell integration
- global full-text search
- remote wipe
- antivirus UI
- multi-tenant hosting features
- SAML in MVP-1

## 12. UX Principles

- files first
- no app-store-like extensibility
- minimal navigation
- mobile-first upload reliability
- obvious share state and permission visibility
- every failed upload must be visible and retryable
- no hidden sync magic

## 13. Recommended MVP Release Plan

### MVP-1

Ship:

- backend/API
- web app
- iOS + Android app
- OIDC
- internal shares
- public links
- upload-only file drop links
- trash/restore
- basic versioning
- audit log

Do not ship:

- desktop sync client
- SAML
- advanced admin policy engine
- full-text search

### MVP-1.5

Add after early feedback:

- shared-with-me improvements
- group-based sharing UX
- better mobile offline support
- admin reporting
- basic WebDAV access if needed

### MVP-2

Possible additions:

- desktop sync client
- selective sync
- richer preview support
- push notifications
- SAML
- remote wipe

## 14. Planning Estimates

These estimates assume:

- 2 backend/full-stack engineers
- 1 frontend engineer
- 1 mobile engineer
- 0.5 design/QA
- 0.5 DevOps/platform support
- use of PostgreSQL + S3-compatible storage + existing OIDC provider

### 14.1 Recommended MVP-1 estimate

Calendar estimate:

- 14 to 18 weeks for a usable MVP
- 20 to 28 weeks for a hardened v1 suitable for serious production rollout

Person-week estimate:

- product/spec/discovery: 2 to 3 person-weeks
- backend core file service: 8 to 10 person-weeks
- sharing + permissions + audit: 6 to 8 person-weeks
- web app: 6 to 8 person-weeks
- mobile apps: 10 to 14 person-weeks
- OIDC integration across web/mobile/API: 3 to 4 person-weeks
- infra/CI/CD/ops baseline: 3 to 5 person-weeks
- QA/hardening/perf/security fixes: 6 to 10 person-weeks

Total:

- roughly 44 to 62 person-weeks

### 14.2 If desktop sync is added to MVP-1

Add:

- 16 to 24 person-weeks
- 8 to 12 calendar weeks

Why:

- local state
- filesystem watchers
- conflict handling
- selective sync
- retry/recovery
- installer/update/channel support
- OS-specific quirks

### 14.3 Solo-founder estimate

If one strong engineer attempts this alone:

- 8 to 12 months for a usable MVP with mobile clients
- 12 to 18 months for something that can be honestly presented as production-ready

This is the honest estimate. Anything much shorter usually hides major quality risk.

## 15. Key Risks

### Highest product/engineering risks

1. Mobile background upload reliability
2. Large-file/resumable upload correctness
3. Permission inheritance edge cases
4. Public-link security and abuse prevention
5. IdP/group mapping edge cases
6. Trash/version/restore semantics

### Scope risk

The easiest way to accidentally turn this MVP into "another Nextcloud" is to add:

- desktop sync
- WebDAV
- document editing
- federation
- advanced admin features

Those should be tightly controlled.

## 16. Strong Recommendations

### Recommendation 1

Support **OIDC only** in MVP-1.

### Recommendation 2

Ship **mobile photo backup**, not general mobile filesystem sync.

### Recommendation 3

Use a **modular monolith**, PostgreSQL, and S3-compatible blob storage.

### Recommendation 4

Keep the permission model simple. Avoid custom permission builders in MVP.

### Recommendation 5

Delay desktop sync unless it is contractually required.

## 17. Source Notes

This specification was informed by current official documentation from Nextcloud and Seafile, especially:

- Nextcloud file sharing administration docs:
  https://docs.nextcloud.com/server/latest/admin_manual/configuration_files/file_sharing_configuration.html
- Nextcloud user file sharing docs:
  https://docs.nextcloud.com/server/24/user_manual/en/files/sharing.html
- Nextcloud OIDC auth docs:
  https://docs.nextcloud.com/server/latest/admin_manual/configuration_user/user_auth_oidc.html
- Nextcloud WebDAV access docs:
  https://docs.nextcloud.com/server/latest/user_manual/br/files/access_webdav.html
- Nextcloud anonymous upload/file drop docs:
  https://docs.nextcloud.com/server/latest/user_manual/en/files/file_drop.html
- Nextcloud Android client/developer docs:
  https://docs.nextcloud.com/server/20/developer_manual/client_apis/android_library/index.html
- Nextcloud desktop client overview:
  https://docs.nextcloud.com/server/latest/user_manual/en/desktop/index.html
- Seafile OAuth auth docs:
  https://manual.seafile.com/12.0/config/oauth/
- Seafile SAML docs:
  https://manual.seafile.com/12.0/config/saml2_in_10.0/
- Seafile desktop sync docs:
  https://help.seafile.com/syncing_client/install_sync/
- Seafile selective sync docs:
  https://help.seafile.com/syncing_client/selective_sync_sub-folders/
- Seafile folder permission docs:
  https://help.seafile.com/sharing_collaboration/folder_permission/
- Seafile features overview:
  https://www.seafile.com/en/features/

## 18. Final Honest Assessment

This product is feasible.

But a good "file sharing lite" product with:

- sharing
- permissions
- SSO
- mobile photo backup
- reliable uploads

is still a serious product, not a weekend build.

The safest MVP is:

- web + mobile
- OIDC
- simple shares
- basic versioning
- no desktop sync

That is the line where the product can be useful without pretending to be a full Nextcloud or full Seafile replacement on day one.
