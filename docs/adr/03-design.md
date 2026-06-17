# RustShare Design Document
**Version:** 2026-03-28  
**Document type:** Architecture and design  
**Audience:** staff engineers, architects, implementation LLMs

---

## 1. Objective

Define the target architecture for RustShare as a serious 2026 file-sharing platform, while remaining practical for the current MVP and honest about the trade-offs.

This document answers:

- what architecture a rebuilt file-sharing company would likely choose
- where RustShare should align with that
- where RustShare may deviate
- what safety rails are mandatory if it deviates

---

## 2. Architectural thesis

A modern file-sharing platform should be built as:

- **service-oriented, not monolithic by schema**
- **object-store-centered for file blobs**
- **identity- and permission-centric for access**
- **event-driven for integrations and projections**
- **memory-safe in its core implementation choices**
- **AI-optional but AI-ready**

The cleanest platform shape is:

1. **Identity + tenant layer**
2. **Namespace / metadata / permission layer**
3. **Blob/object storage layer**
4. **Search / index / preview / AI layer**
5. **Client sync layer**
6. **Integration layer (chat, audit export, policy hooks)**

---

## 3. Recommended target user architecture

### 3.1 Primary audience
Technical organizations that care about:
- self-hosting or private-cloud control
- security and auditability
- internal/external sharing
- predictable client sync
- chat-connected collaboration

### 3.2 Why not broad consumer
Broad consumer file sync is won through distribution, ecosystem lock-in, and device-level defaults. RustShare’s defensible advantage is not there.

---

## 4. Core decisions

## D-01 System of record model
**Recommended answer:** hybrid.

Use:
- object storage as the durable blob/version substrate
- a strongly consistent metadata + permission system as the namespace truth
- asynchronous projections for search, AI, previews, notifications

This is the most industry-realistic answer.

### Why
A file-sharing product has at least five distinct durability concerns:
- bytes
- namespace
- permissions
- events/audit
- search/indexing

Trying to flatten all of those into a single primitive tends to create hidden coupling and operational pain.

## D-02 Best backend storage model for multi-tenant support
**Recommended answer:** S3-compatible object storage for blobs, with explicit tenant partitioning and explicit metadata authority.

Do **not** use a distributed POSIX filesystem as the primary product backbone unless your product goal specifically requires POSIX semantics as the top-level user contract.

### Why object storage wins
- natural fit for immutable versions and large blobs
- broad cloud/private-cloud portability
- good durability and lifecycle management
- easy bridge to backup/export/replication strategies
- natural multi-tenant partitioning patterns

AWS documents multiple valid multi-tenant S3 isolation patterns: dedicated bucket per tenant, shared bucket with prefix isolation, and access points; each has trade-offs in quotas, isolation, and manageability.[^1][^2][^3]

### Honest recommendation on “best filesystem”
If the question means “what should back the product,” the best answer is **not a POSIX cluster filesystem**. It is **object storage plus a clear namespace/metadata authority**.

If the question means “what on-prem substrate should implement the object layer,” acceptable choices include:
- AWS S3
- Ceph RGW
- another serious S3-compatible object implementation

The user-facing filesystem should be a **virtual namespace**, not your infrastructure substrate leaking into product behavior.

## D-03 Platform choice for the safest environment
**Recommended answer:**
- backend services in **Rust**
- desktop client in **Rust** with a minimal native shell approach
- Linux-based production runtime with hardened containerization
- OIDC identity integration
- principle-of-least-privilege object-store credentials
- signed event integrations

### Why
NSA and CISA explicitly highlighted memory-safe languages as a meaningful way to reduce vulnerability classes.[^4]
That does not make Rust magically safe, but it materially improves the baseline for a security-sensitive platform.

### Avoid
- unnecessary Electron-heavy privileged clients where a lighter shell can work
- broad root privileges
- shared static credentials across services
- coupling security-sensitive services through direct database access

## D-04 Integration with chat
**Recommended answer:** bounded service integration with shared identity and event-driven coordination.

Use:
- shared identity source (OIDC, optionally SCIM group provisioning)
- stable file references
- permission-aware unfurls/previews
- event-driven link preview invalidation / updates
- API-level attach/share flows

Do **not**:
- make chat and file share one database
- let chat bypass file authorization
- copy permissions manually into multiple sources of truth

Slack’s Events API is a useful model for scoped event delivery and acknowledgment-before-processing patterns.[^5]

## D-05 AI architecture
**Recommended answer:** separate AI/indexing subsystem with hard permission boundaries.

Use:
- extraction/indexing pipeline
- searchable embeddings or semantic index if needed
- permission filtering at query time
- tenant controls for enable/disable
- source citation in answers
- no write permission by default

This matches the direction of Box AI, Dropbox Dash, Microsoft Copilot in OneDrive, and Google Drive/Gemini: AI as a layer on authorized content, not a replacement for the content system.[^6][^7][^8][^9][^10]

---

## 5. Proposed high-level architecture

```text
                           +----------------------+
                           |  OIDC / SCIM / IdP   |
                           +----------+-----------+
                                      |
                                      v
+-------------------+       +---------+----------+       +------------------+
|   Web UI          |<----->|  API / Auth Layer  |<----->|   Audit Stream    |
|   Desktop Client  |       +---------+----------+       +------------------+
|   Mobile Web      |                 |
+-------------------+                 v
                           +----------+-----------+
                           | Namespace / Metadata |
                           | Permissions / Shares |
                           +----+------------+----+
                                |            |
                                |            +------------------+
                                |                               |
                                v                               v
                      +---------+---------+           +---------+---------+
                      |   Blob/Object     |           | Search / Preview  |
                      |   Storage         |           | Index / AI Layer  |
                      +---------+---------+           +---------+---------+
                                |                               |
                                +---------------+---------------+
                                                |
                                                v
                                      +---------+---------+
                                      | Integration Layer |
                                      | Chat / Webhooks   |
                                      +-------------------+
```

---

## 6. RustShare-aligned implementation shape

This section adapts the ideal architecture to your known direction.

## 6.1 Blob layer
Use S3-compatible object storage as the primary durable blob substrate.

Recommended logical layout:
```text
tenant/{tenant_id}/blobs/{blob_id}
tenant/{tenant_id}/versions/{file_id}/{version_id}
tenant/{tenant_id}/exports/...
tenant/{tenant_id}/audit/...
```

Current implementation note (2026-06-18): RustShare stores durable file bytes under global content-addressed keys `blobs/{sha256}` and keeps tenant ownership, permissions, and file-to-blob mapping in metadata. Tenant isolation is enforced by repository/service checks rather than by tenant-prefixed object keys.

If RustShare prefers per-user or per-tenant buckets, that can work, but it must be justified operationally.
AWS notes that dedicated buckets are increasingly viable thanks to higher bucket quotas, while shared buckets with prefixes or access points can offer scalability and flexibility.[^1]

### My honest recommendation
- for **true enterprise multi-tenant product design**, prefer **per-tenant partitioning**
- within a tenant, do **not** over-fragment durable storage by user unless there is a very strong operational/export reason
- per-user isolation is elegant for portability, but it can complicate cross-user shares, storage accounting, admin policy, and future tenant migrations

### Recommended compromise
- **tenant is the hard security/administrative boundary**
- user ownership is metadata inside the tenant
- shared content lives naturally inside the tenant namespace
- export portability is achieved through tenant-scoped export/import tooling, not by turning each user into a storage island

If RustShare chooses per-user buckets anyway, the contract document must be enforced more aggressively.

## 6.2 Metadata layer
This is the most sensitive design choice.

### Strong recommendation
Keep explicit metadata authority for:
- namespace tree
- file identity
- versions
- permissions
- shares
- device state
- audit references
- indexing state

### Best production answer
A strongly consistent metadata store.

### RustShare-allowed path
If RustShare wants a storage-native metadata model:
- store metadata documents in object storage
- maintain explicit projections/indexes
- implement locking/version checks
- provide repair tooling
- document eventual-consistency boundaries
- never pretend this is simpler unless the contracts prove it

### Honest warning
A pure object-store metadata design can be elegant for bootstrap and portability, but it is easier to get:
- listing semantics wrong
- atomic transitions wrong
- permission propagation wrong
- repair workflows under-specified

That does not mean “do not do it.”
It means “only do it with strong contracts and explicit operational boundaries.”

## 6.3 Namespace model
Recommended canonical entities:

- tenant
- principal (user/group/service account)
- workspace/root collection
- folder
- file
- version
- share grant
- public link
- device
- audit event
- projection/index state
- policy

Each entity must have:
- stable identifier
- created_at / updated_at
- actor/source attribution
- tenant id
- concurrency/version field

## 6.4 Sharing model
Shares should be capability-based and easy to reason about.

Recommended capability set:
- read
- write/upload
- manage share
- admin override

Public links should be constrained to:
- read-only
- upload-only
- explicit read/write only if a tenant policy allows it

Avoid many subtle variants that users cannot distinguish.

## 6.5 Sync model
A file platform is only real when sync is real.

### Required client semantics
- resumable upload/download
- content hashing
- deterministic conflict naming or resolution
- selective sync
- local state journal
- revocable device binding
- backoff and retry

### Recommended architecture
- server issues scoped sync cursors/checkpoints
- client keeps a local sync journal
- server exposes delta/feed API
- upload sessions support resume
- conflict resolution is deterministic and documented

---

## 7. Chat integration design

## 7.1 Integration goal
Make files a first-class artifact inside chat without collapsing the systems into one.

## 7.2 Integration layers

### Layer 1: shared identity
- same OIDC identity source
- aligned user ids or deterministic mapping
- aligned groups if SCIM/group sync exists

### Layer 2: share and attach flows
- user can select a RustShare file from chat
- chat stores stable file reference + presentation metadata
- actual file access remains checked by RustShare

### Layer 3: unfurls / previews
- chat requests preview metadata from RustShare
- RustShare returns permission-filtered preview
- no preview for unauthorized users

### Layer 4: events
- file renamed / revoked / expired => chat preview updates or disappears
- tenant policy changes => integration behavior re-evaluated

## 7.3 What not to do
Do not:
- duplicate file permissions into chat as a second source of truth
- embed direct long-lived storage credentials in chat
- let chat cache private previews forever without revalidation

---

## 8. AI subsystem design

## 8.1 AI goals
Use AI to reduce retrieval and triage cost.

## 8.2 AI Phase A feature set
- summarize file
- answer questions on a file
- answer questions across a folder or curated collection
- suggest metadata/tags
- extract structured fields
- support semantic search over authorized content

## 8.3 AI pipeline
```text
Upload/Change Event
      |
      v
Text extraction / OCR / parsing
      |
      +--> metadata extraction
      |
      +--> preview generation
      |
      +--> full-text index
      |
      +--> semantic index (optional)
```

## 8.4 Permission strategy
Every AI query must:
1. resolve caller identity
2. resolve caller’s effective accessible file set
3. filter retrieval candidates accordingly
4. generate answer from authorized subset only
5. return file references/citations

## 8.5 Control strategy
Tenant admin controls:
- AI enabled/disabled
- semantic indexing enabled/disabled
- external model usage policy
- retention for AI-derived indexes
- whether AI can process public-link content

---

## 9. Security model

## 9.1 Identity
- OIDC for auth
- optional SCIM for provisioning
- short-lived tokens / revocable sessions
- device registration for sync clients

## 9.2 Authorization
Recommended model:
- ACL/capability model over stable principals
- tenant-scoped policy evaluation
- group expansion at read time or cached projection
- explicit admin override paths

## 9.3 Secrets
- no permanent client-side object-store credentials
- signed, scoped server-side mediated access
- public-link secrets never logged raw

## 9.4 Runtime
- hardened Linux runtime
- rootless where practical
- isolated service credentials
- signed integration events
- observability scrubbed for secrets

---

## 10. Storage isolation options

### Option A — dedicated bucket per tenant
**Best for:** strong isolation clarity, export simplicity, compliance clarity  
**Trade-off:** more objects to manage at infrastructure-control plane level

### Option B — shared bucket with tenant prefixes
**Best for:** operational simplicity at smaller scale  
**Trade-off:** more careful policy design required; some controls remain bucket-scoped

### Option C — access-point mediated isolation
**Best for:** larger-scale differentiated access control if supported by the chosen object platform  
**Trade-off:** platform support and operational complexity vary

AWS documents all three as valid patterns depending on scale and requirements.[^1][^2][^3]

### Recommendation for RustShare
- prefer **tenant as the isolation unit**
- pick **A or B** based on operational model
- do not let the storage layout define the permission model

---

## 11. Deployment model choices

## 11.1 Recommended baseline
- self-hosted Linux deployment
- private-cloud ready
- optional managed deployment
- object store can be cloud or on-prem
- no mandatory dependency on a public SaaS control plane

## 11.2 Why
The product’s advantage is sovereignty and control.

## 11.3 Production topology baseline
- API/auth service
- metadata/namespace service
- object storage
- background workers (preview/index/audit/export)
- search/index service
- optional AI service
- optional chat integration service

---

## 12. Migration guidance from current MVP

This is the most important practical section.

### Keep
- working file CRUD
- existing share flows that already behave correctly
- restore/backup behavior that is already real
- user-visible navigation concepts that work
- pairing UX direction if it is already simpler now

### Add next
1. explicit tenant model
2. contract-test suites
3. identity/group semantics
4. sync semantics
5. audit model
6. search/index boundary
7. chat integration service boundary

### Delay
- ambitious AI
- broad automation
- office-suite-like features
- deep workflow builder

### Refactoring rule
Do not replace the current MVP with a “clean architecture rewrite.”
Wrap the MVP with contracts, identify the divergences, and converge piece by piece.

---

## 13. Decision summary

### Recommended final answers to the original strategic questions

**Target group**  
Technical SMBs, MSPs, platform teams, and regulated mid-market orgs needing controlled self-hosted/private-cloud file collaboration.

**AI features worth pursuing**  
Permission-aware semantic search, summaries, Q&A over files/folders, metadata extraction, classification/tagging, and duplicate/related-file discovery.

**Best backend model**  
Object storage for blobs plus explicit metadata authority. Not a distributed POSIX filesystem as the core product truth.

**Safest platform choice**  
Memory-safe core services, especially Rust, on hardened Linux/container infrastructure with strong identity and least-privilege access.

**Best chat integration mechanism**  
Service boundary with shared identity and event-driven previews/share flows; not tight database coupling.

**Phase 1 features**  
Identity, core file operations, sharing, sync, device onboarding, auditability, restore, admin basics, search, and minimal chat integration.

---

## 14. References

[^1]: AWS Storage Blog — Design patterns for multi-tenant access control on Amazon S3. https://aws.amazon.com/blogs/storage/design-patterns-for-multi-tenant-access-control-on-amazon-s3/  
[^2]: AWS Prescriptive Guidance — tenant isolation for Amazon S3 using token vending. https://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/implement-saas-tenant-isolation-for-amazon-s3-by-using-an-aws-lambda-token-vending-machine.html  
[^3]: AWS SaaS storage and tenant isolation whitepapers. https://docs.aws.amazon.com/whitepapers/latest/multi-tenant-saas-storage-strategies/multi-tenant-saas-storage-strategies.html and https://docs.aws.amazon.com/whitepapers/latest/saas-tenant-isolation-strategies/saas-tenant-isolation-strategies.html  
[^4]: NSA/CISA guidance on memory-safe languages. https://www.nsa.gov/Press-Room/Press-Releases-Statements/Press-Release-View/Article/4223298/nsa-and-cisa-release-csi-highlighting-importance-of-memory-safe-languages-in-so/  
[^5]: Slack Events API. https://api.slack.com/apis/connections/events-api  
[^6]: Dropbox Dash for Business. https://investors.dropbox.com/news-releases/news-release-details/introducing-dropbox-dash-business-ai-powered-universal-search  
[^7]: Box AI for Hubs. https://support.box.com/hc/en-us/articles/29347206309395-Box-AI-for-Hubs  
[^8]: Box Extract / AI extract agents. https://www.box.com/extract and https://support.box.com/hc/en-us/articles/42325695115155-Announcing-Box-AI-Enhanced-Extract-Agent  
[^9]: Microsoft Copilot in OneDrive. https://support.microsoft.com/en-gb/office/summarize-your-files-with-copilot-10dcbe50-467d-4a61-9d5e-c98c77fd33a4  
[^10]: Google Workspace / Drive AI product direction reporting, March 2026. https://www.theverge.com/tech/890996/google-workspace-gemini-ai-docs-sheets-drive and https://www.tomsguide.com/ai/gemini-just-got-a-major-upgrade-in-docs-sheets-slides-and-drive-heres-how-i-used-it-to-go-from-blank-page-to-finished-project
