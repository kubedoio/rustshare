# RustShare Product Specification
**Version:** 2026-03-28  
**Document type:** Product specification  
**Audience:** founders, product, architecture, engineering, LLM implementation agents

---

## 1. Executive thesis

If an experienced file-sharing company restarted in 2026, it would not chase generic consumer sync.

It would aim at **security-conscious technical organizations** that need:
- controlled sharing
- self-hosted or sovereign deployment options
- tight identity and permission control
- strong auditability
- practical AI on top of authorized content
- a clean bridge between files and team communication

That is the defensible space for RustShare.

The market direction supports this. Dropbox is investing in enterprise security controls and AI-powered universal search; Box is investing in permission-aware AI across enterprise content plus agentic extraction; Microsoft is putting Copilot directly on files in OneDrive; Google is turning Drive into an AI-queryable content layer.[^1][^2][^3][^4][^5][^6][^7] That means the next winning file platform is not “just sync.” It is **content access + governance + AI + workflow integration**.

---

## 2. Product position

### 2.1 Product category
Self-hosted or private-cloud file-sharing and sync platform for technical teams and regulated organizations.

### 2.2 Product promise
A secure file platform that combines:
- Dropbox-like usability
- enterprise-grade governance
- chat-aware collaboration
- AI that only works on content the caller is allowed to access

### 2.3 Primary segment
**Primary target:** technical SMBs, MSPs, platform teams, internal IT teams, and regulated mid-market organizations that need data control and self-hosting options.

### 2.4 Why this segment
This segment is the most realistic because:
- broad consumer file sync is dominated by incumbents with huge distribution advantages
- large-enterprise horizontal platforms require long compliance and procurement cycles
- technical mid-market and infrastructure-centric teams feel the pain of self-hosting, integration, and permission control most sharply
- these buyers value operational control, not just polish

### 2.5 Segment exclusions
RustShare Phase 1 is **not** optimized for:
- mass consumer photo backup
- creator/content publishing workflows
- generalized ECM replacement
- highly collaborative Office-like live document editing
- autonomous AI agents making destructive storage decisions

---

## 3. Product goals

### 3.1 Business goals
1. Become the most credible self-hosted/private-cloud file-sharing option for technical teams.
2. Establish a clean integration path with RustChat and other collaboration tools.
3. Differentiate on secure architecture, transparent permissions, and permission-aware AI.
4. Reach “usable every day” status with a tight Phase 1 instead of feature sprawl.

### 3.2 User goals
Users must be able to:
- upload, organize, sync, and recover files
- share internally and externally with understandable permissions
- access files from web and lightweight clients
- trust that chat/file permissions line up
- quickly find information across files
- know who accessed or changed what

### 3.3 Non-goals for Phase 1
- full office suite replacement
- arbitrary plug-in marketplace
- autonomous AI workflows with write/delete rights
- multi-region active/active complexity
- massive analytics stack
- tenant-custom AI model hosting as a baseline requirement

---

## 4. Core product principles

1. **Permission clarity beats feature count.**
2. **Auditability beats hidden magic.**
3. **Object storage is for durable content; metadata behavior must still be explicit and testable.**
4. **Chat integration is valuable only if permissions remain consistent.**
5. **AI must be explainable, bounded, and permission-aware.**
6. **The first release must feel reliable, not ambitious.**

---

## 5. Product scope

## 5.1 Phase 1 minimum viable product
Phase 1 must deliver a **working daily-use file platform**, not a demo.

Required capabilities:

### Identity and tenancy
- organization/workspace/tenant model
- user identity via OIDC
- stable user, group, and tenant identifiers
- role model: owner/admin/member/guest
- support for internal groups as permission principals

### File operations
- folder creation and navigation
- upload/download
- rename/move/delete/restore
- version history for changed files
- search by name/path/basic metadata
- reliable large-file transfer
- conflict-safe overwrite semantics

### Sharing
- internal shares to users and groups
- external/public links with clear capability modes:
  - read-only
  - upload-only
  - read/write only if explicitly enabled
- expiration, revocation, and auditing for external links
- “shared with me”

### Sync and clients
- web UI usable on desktop and mobile
- desktop sync client for selected folders
- device pairing/onboarding
- resumable/background sync
- deterministic conflict behavior

### Governance and trust
- immutable audit log for:
  - login
  - upload
  - download
  - share create
  - share revoke
  - delete
  - restore
  - public-link access
- admin visibility for storage, share usage, and risky external sharing
- rate limits and abuse guardrails

### Reliability
- backup/export path
- restore verification
- observability for core flows
- clear health model

### Chat integration baseline
- file link previews/unfurls in chat
- permission-checked attachment/share from chat
- shared identity and group mapping
- no shared database between chat and file services

## 5.2 Phase 1.5
These are near-term, high-value additions once Phase 1 is stable:
- thumbnails/previews
- content indexing and OCR for searchable documents
- basic file lock / “currently editing” presence
- admin policies for external sharing
- tenant-level retention and legal-hold basics
- antivirus or malware-scanning hook
- DLP/classification hook
- richer group-based sync policies

## 5.3 AI Phase A
AI features that are strategically useful and safe enough to add early:

1. **Ask this file**
2. **Ask this folder / collection**
3. **Permission-aware semantic search**
4. **Auto-generated summaries**
5. **Metadata/tag suggestion**
6. **Extraction of structured fields from documents**
7. **Duplicate / near-duplicate / likely-related file detection**

These directions match where Dropbox, Box, Microsoft, and Google are investing: universal search, multi-file Q&A, summaries, and extraction over authorized content.[^1][^2][^3][^4][^5][^6][^7]

## 5.4 AI features explicitly deferred
- autonomous permission changes
- autonomous deletion or retention actions
- agentic workflows with broad write scope by default
- unrestricted “chat with the whole company” without policy boundaries
- training custom foundation models as a Phase 1 blocker

---

## 6. Recommended product shape

## 6.1 Best-fit packaging
RustShare should be sold and positioned as:

- self-hosted / private-cloud first
- optionally managed by provider or MSP
- identity-integrated
- chat-integrated
- AI-assisted, not AI-dependent

## 6.2 Honest differentiation
RustShare should not try to out-Box Box on enterprise content suites or out-Dropbox Dropbox on mainstream distribution.

It should win on:
- operational control
- self-hosting
- clear permissions
- integration with technical collaboration
- predictable architecture
- auditability
- secure AI on private content

---

## 7. RustShare-specific guidance

The current MVP already appears to have important pieces:
- file/folder CRUD
- uploads/downloads/move/rename/delete/restore
- version history
- internal sharing
- public and upload-only links
- notifications and real-time updates
- “shared with me”
- replication and backup/restore work
- web UI and pairing direction

That means the main risk is no longer “can it upload files?”
The main risk is now **architectural convergence**:
- tenant isolation
- permission model consistency
- sync semantics
- governance depth
- enterprise identity
- search/indexing
- chat integration boundaries
- contract-test coverage

---

## 8. Assumed current-state gap map

This table is based on the product state described in conversation, not a repo audit.

| Area | Assumed status | Notes |
|---|---|---|
| Basic CRUD and sharing | Implemented / partial | Strong MVP base already exists |
| External/public links | Implemented / partial | Needs stronger governance contracts |
| Multi-tenant model | Partial | Must be explicit and first-class |
| OIDC identity | Partial | Needs firm tenant and group semantics |
| SCIM provisioning | Missing | Important for serious org adoption |
| Desktop sync | Partial / missing | Must become a first-class Phase 1 deliverable |
| Mobile usability | Partial | Web/mobile baseline needed even if native sync comes later |
| Audit and admin governance | Partial | Must become product-level, not incidental logs |
| Search/indexing | Partial / missing | Name/path search is not enough |
| AI features | Missing / deferred | Safe, bounded Phase A only |
| Chat integration | Partial / strategic | Must be service-boundary based |
| Contract test coverage | Missing / insufficient | Required before major refactor |
| Storage isolation and metadata guarantees | Partial | Must be enforced with invariants |

---

## 9. Feature priority order

### P0
- tenant model
- permission model
- object/blob + metadata consistency
- sync semantics
- sharing governance
- audit events
- identity stabilization
- restore reliability

### P1
- desktop client
- search/indexing
- chat unfurl and attach/share flow
- admin console essentials
- external-link policy controls

### P2
- AI search/summarize/extract
- previews/thumbnails/OCR
- malware scanning and DLP integrations
- more advanced retention/legal hold
- automation hooks

---

## 10. Hard product requirements

1. A user must never see content they are not entitled to.
2. Share revocation must become effective promptly and measurably.
3. Public links must be policy-scoped and fully auditable.
4. Tenant identity must be stronger than UI grouping.
5. AI responses must cite source files or source ranges where possible.
6. AI must not bypass existing permissions.
7. Chat integration must not introduce a second permission truth.
8. Export/backup/restore must be operationally real, not marketing language.
9. The web product must remain fully useful without AI features enabled.
10. The architecture must allow self-hosted operation without hidden SaaS dependencies.

---

## 11. What not to do

Do not:
- build a POSIX-distributed-filesystem-first product core
- tightly couple chat and file storage in one schema or one database
- add AI before search, metadata, and permissions are trustworthy
- create many share modes without clear mental models
- rely on manual QA instead of executable contracts for tenant isolation and sharing
- optimize for giant-enterprise “everything platform” scope before daily-use reliability

---

## 12. Success criteria

RustShare Phase 1 is successful when a technical organization can:

- onboard a tenant with OIDC
- invite users or provision groups
- sync selected folders from desktop
- share safely inside and outside the organization
- recover deleted files and inspect version history
- audit who shared/downloaded/accessed files
- paste a RustShare link into chat and get a permission-aware preview
- operate the system with confidence from logs, metrics, and backups

---

## 13. References

[^1]: Dropbox announced Dash for Business as AI-powered universal search plus organization, sharing, and content access control for business teams. Source: Dropbox investor announcement, Oct 15 2024. https://investors.dropbox.com/news-releases/news-release-details/introducing-dropbox-dash-business-ai-powered-universal-search  
[^2]: Dropbox enterprise security messaging emphasizes classification, alerts/notifications, retention, and control. Source: Dropbox enterprise security pages. https://experience.dropbox.com/en-gb/enterprise-security and https://www.dropbox.com/business/security-and-compliance  
[^3]: Box AI for Hubs supports questions across multiple files with citations and respects user access boundaries. Source: Box Support. https://support.box.com/hc/en-us/articles/29347206309395-Box-AI-for-Hubs  
[^4]: Box AI extract agents and Box Extract focus on structured metadata extraction from enterprise content and workflow automation. Source: Box Support / Box. https://support.box.com/hc/en-us/articles/42325695115155-Announcing-Box-AI-Enhanced-Extract-Agent and https://www.box.com/extract  
[^5]: Microsoft documents Copilot in OneDrive for file summaries, Q&A, and multi-file operations. Source: Microsoft Support. https://support.microsoft.com/en-gb/office/summarize-your-files-with-copilot-10dcbe50-467d-4a61-9d5e-c98c77fd33a4 and https://support.microsoft.com/en-gb/office/frequently-asked-questions-about-copilot-in-onedrive-1bd55d10-7c46-417c-ab3d-e9c7be346947  
[^6]: Google Workspace is expanding Gemini directly across Drive, Docs, Sheets, and Slides for AI-assisted creation, summaries, and file Q&A. Source: Google Workspace reporting / product coverage from March 2026. https://www.theverge.com/tech/890996/google-workspace-gemini-ai-docs-sheets-drive  
[^7]: Google Drive AI overviews and file-based querying reflect the market move toward the file store as an AI-queryable knowledge surface. Source: March 2026 reporting. https://www.tomsguide.com/ai/gemini-just-got-a-major-upgrade-in-docs-sheets-slides-and-drive-heres-how-i-used-it-to-go-from-blank-page-to-finished-project
