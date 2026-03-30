# RustShare Contract Document
**Version:** 2026-03-28  
**Document type:** Behavioral contract / acceptance criteria  
**Audience:** engineering, QA, LLM implementation agents

---

## 1. Purpose

This document defines the behavioral contracts that must remain true while RustShare is polished and extended.

**Priority rule:**
- If code, design, and product spec disagree, this contract document is the enforcement layer.
- A feature is not considered implemented until the relevant contract is testable.

The contracts are written so they can be converted into:
- integration tests
- API contract tests
- end-to-end tests
- property tests
- storage invariants
- security regression tests

---

## 2. Status vocabulary

Each contract should be marked in the repo as one of:
- implemented
- partial
- missing
- conflicting

---

## 3. Global invariants

### G-01 Tenant isolation
No authenticated or anonymous request may read, list, modify, or infer another tenant’s protected content without an explicit cross-tenant sharing mechanism.

### G-02 Permission monotonicity
A response may expose only the content the current principal could already access through the official permission model.

### G-03 Auditability
Every security-sensitive action must produce a durable audit event.

### G-04 Idempotent retries
Safe client retries must not create duplicate durable side effects.

### G-05 Revocation correctness
Permission or link revocation must take effect on subsequent access checks without requiring manual cleanup.

### G-06 Restore viability
Backup/export artifacts must be sufficient to restore tenant data and verify the restored result.

### G-07 AI permission parity
AI/indexing/search services must never answer from content the requesting principal cannot access through the normal product.

---

## 4. Identity and tenancy contracts

### I-01 Stable subject identity
Given a user authenticates through OIDC,
when the same identity returns,
then the platform must resolve the same internal subject unless an explicit migration occurred.

### I-02 Tenant binding
Given a request is authenticated,
then every durable operation must execute inside exactly one tenant context.

### I-03 Group principal support
Given a group exists,
when a share is granted to that group,
then every effective member obtains the share and non-members do not.

### I-04 Deprovision safety
Given a user is disabled or removed,
when token refresh or subsequent access occurs,
then access must be denied according to configured policy and new shares cannot be created by that user.

### I-05 SCIM compatibility target
The platform must expose or support a provisioning path compatible with SCIM-style user/group lifecycle semantics, even if full SCIM API support lands after initial implementation.[^1][^2]

---

## 5. File and metadata contracts

### F-01 Create file
Given an authorized user uploads a new file into a writable location,
then:
- exactly one logical file record is created
- one current version becomes active
- the file appears in listings for authorized viewers
- an audit event is emitted

### F-02 Replace file
Given an authorized user overwrites an existing file,
then:
- the file identity remains stable
- a new version is created
- the previous version remains recoverable according to retention rules
- the current version pointer changes atomically

### F-03 Rename and move
Given an authorized user renames or moves a file,
then:
- file identity remains stable
- version history remains attached
- shares remain attached unless policy explicitly says otherwise
- no duplicate blob is created solely due to rename/move

### F-04 Delete and restore
Given an authorized user deletes a file,
then:
- the file disappears from standard listings
- recovery metadata remains available until purge or retention expiry
- restore returns the file with its identity and history intact unless explicitly documented otherwise

### F-05 Folder listing consistency
Given two consecutive reads without intervening writes,
then folder listing order and pagination behavior must be deterministic.

### F-06 Path safety
The namespace layer must reject path traversal and invalid normalization edge cases.

### F-07 Metadata/blob consistency
For every active file version, exactly one readable blob reference and one metadata record must exist.
Broken references must be detectable by verification tooling.

### F-08 External object mutation protection
If storage is object-store-backed, out-of-band mutations must not silently corrupt metadata expectations.
The platform must either detect divergence or explicitly state unsupported behavior.

---

## 6. Sharing contracts

### S-01 Internal share
Given owner A shares item X with user B or group G,
then only B or members of G gain access according to the granted capability.

### S-02 Public read link
Given a public read link exists,
then anonymous users may read only the linked scope and only with the granted capability.

### S-03 Upload-only link
Given an upload-only public link exists,
then an anonymous caller may upload into the target scope but may not browse unrelated existing content unless the contract explicitly allows a submission receipt view.

### S-04 Expiry
Given a share or public link has expired,
then subsequent access attempts must fail and be auditable.

### S-05 Revocation
Given a share or public link is revoked,
then subsequent access checks must deny access without waiting for cache expiry beyond the documented SLA.

### S-06 Share discoverability
A principal must be able to see items directly shared with them in “Shared with me” or equivalent views.

### S-07 Share governance
Admin policy must be able to restrict whether external or public sharing is allowed at tenant/workspace level.

### S-08 Secret hygiene
Public-link secrets must be unguessable and never exposed in logs or analytics in plain text.

---

## 7. Sync and client contracts

### C-01 Device pairing
Given a user pairs a device through QR or manual key,
then the new device obtains a scoped, revocable trust relationship and no reusable long-lived secret is displayed after completion.

### C-02 Initial sync correctness
Given a desktop client selects a folder set,
then the local materialized state converges to the authorized remote state.

### C-03 Retry safety
Given network interruption during upload/download,
when the client retries,
then the final durable state is correct and duplicates are not created.

### C-04 Conflict behavior
Given concurrent edits occur without server-side locking,
then the conflict outcome must be deterministic and documented.

### C-05 Selective sync
Given a folder is not selected for sync,
then its content is not materialized locally except for allowed metadata or placeholders.

### C-06 Revoked device
Given an admin revokes a device,
then subsequent sync/authentication from that device must fail within the documented revocation window.

---

## 8. Search and indexing contracts

### Q-01 Name/path search
The platform must support basic search by file name and path for authorized content.

### Q-02 Authorization-filtered results
Search results must be filtered by the same permission model as normal browsing.

### Q-03 Eventual indexing
If full-text or semantic indexing is enabled,
new or changed content must become searchable within a documented eventual-consistency window.

### Q-04 Delete propagation
Deleted, revoked, or expired content must stop appearing in search or AI answers according to the documented propagation SLA.

---

## 9. AI contracts

### A-01 Optional subsystem
The core file platform must remain functional if AI/indexing services are disabled.

### A-02 Permission-aware answers
Given a user asks AI about a file, folder, or collection,
then the answer must only use content that user can access through the standard product.

### A-03 Source attribution
For summarization or Q&A over files, the system should provide source file references and, where feasible, source snippets or ranges.

### A-04 No training-by-default
Tenant content must not be used for external model training by default.

### A-05 Tenant controls
A tenant admin must be able to disable AI or restrict specific AI features.

### A-06 Write isolation
AI must not perform file mutation, share creation, permission changes, or deletion unless a separate explicit write-capable workflow is designed and approved.

### A-07 Extraction boundaries
Structured extraction outputs must inherit the source file’s permission boundaries.

The importance of permission-aware AI is reflected in Box AI for Hubs and similar enterprise systems, which operate on content the user can access and expose citations; Microsoft and Google are likewise making file AI features a layer on top of existing file access models.[^3][^4][^5]

---

## 10. Chat integration contracts

### H-01 Service boundary
RustShare and chat must integrate through APIs/events/identity, not through a shared database.

### H-02 Shared identity
If integrated with chat, both systems must resolve users and groups through a shared identity source or a synchronised mapping.

### H-03 Link unfurl permission check
Given a user posts a RustShare link into chat,
then previews/unfurls must be shown only to viewers authorized to access that file or collection.

### H-04 Attachment/share from chat
Given a user chooses a file from RustShare inside chat,
then chat receives a stable reference plus permission-checked rendering rules, not unrestricted blob access.

### H-05 Revocation propagation
Given access to a file is revoked in RustShare,
then chat preview/access must stop on subsequent checks.

### H-06 Event-driven sync
Integration events must be signed or authenticated, idempotent, and replay-safe.

Slack’s Events API is a good reference for event-driven integrations that receive scoped events over HTTP or socket-based delivery and acknowledge before processing business logic.[^6]

---

## 11. Security contracts

### X-01 Transport security
All authenticated and share-related endpoints must require HTTPS in production deployments.

### X-02 Secret handling
No raw access tokens, public-link secrets, or object-store credentials may be logged.

### X-03 Rate limiting
Anonymous share endpoints, auth endpoints, and search endpoints must be rate limited.

### X-04 Malware/DLP hookability
The ingest pipeline must provide a policy hook for scanning/classification, even if not enabled by default.

### X-05 Memory-safe preference
New core services should prefer memory-safe implementation languages unless a justified exception exists.[^7]

### X-06 Least privilege for storage
Service credentials must be scoped to the minimum storage access needed for runtime behavior.

### X-07 Tenant-aware observability
Operational telemetry must not leak one tenant’s sensitive object names or secrets into another tenant’s views.

---

## 12. Storage architecture contracts

This section is intentionally strict because storage design is where many file platforms fail.

### ST-01 Authoritative content model
The authoritative durable content model must distinguish:
- blob data
- namespace/metadata
- permission state
- audit/event state
- search/index state

Whether those live in one substrate or several, they must remain logically separable.

### ST-02 Accepted production recommendation
The recommended production architecture is:
- object storage for blobs and immutable versions
- strongly consistent metadata/permission state
- asynchronous search/index/event projections

### ST-03 Allowed RustShare adaptation
RustShare may use an object-store-centric or filesystem-as-database-inspired design **only if** it proves:
- atomic enough metadata transitions for user-visible correctness
- durable detection of blob/metadata drift
- strong tenant isolation
- scalable listing and search projections
- recovery and verification workflows
- no hidden dependence on a removed relational database

### ST-04 Multi-tenant isolation
If object storage is shared across tenants,
isolation must be enforced through explicit tenant partitioning and access control.
AWS documents multiple valid patterns including dedicated buckets, shared buckets with prefix isolation, and access points; the correct choice depends on quota, manageability, and isolation needs.[^8][^9][^10]

### ST-05 Verification tooling
The platform must provide tooling to verify:
- missing blobs
- orphaned metadata
- inconsistent versions
- permission anomalies
- broken indexes

### ST-06 Restore testability
A restore operation must be testable without manual interpretation.

---

## 13. Operational contracts

### O-01 Health model
The system must expose health/readiness for:
- auth/session dependencies
- storage dependency
- metadata subsystem
- indexing subsystem
- event delivery subsystem

### O-02 Backup schedule visibility
Operators must know the last successful backup/export time and verification result.

### O-03 SLO-oriented instrumentation
Core flows must emit metrics for latency, error rate, and queue/index lag where relevant.

### O-04 Audit export
Audit records must be exportable for tenant or admin review.

---

## 14. Recommended executable test suites

Minimum suites to implement:

1. **tenant_isolation_contract**
2. **share_link_contract**
3. **public_upload_only_contract**
4. **versioning_contract**
5. **restore_contract**
6. **search_authorization_contract**
7. **chat_integration_contract**
8. **device_pairing_contract**
9. **ai_permission_contract**
10. **storage_verification_contract**

---

## 15. Gherkin starter set

### Scenario: cross-tenant access is denied
```gherkin
Given tenant A contains file "alpha.txt"
And tenant B contains user "bob"
When bob attempts to read tenant A's file by direct identifier
Then the request is denied
And no file metadata is leaked
And an audit event may be recorded without exposing the object secret
```

### Scenario: upload-only link cannot browse
```gherkin
Given folder "inbox/" has an upload-only public link
When an anonymous visitor opens the link
Then they may upload a file
But they may not list unrelated existing files in "inbox/"
And the submission is recorded in the audit trail
```

### Scenario: revoked share stops chat unfurl
```gherkin
Given Alice shared file X with Bob
And Bob posted a RustShare link to file X in chat
When Alice revokes Bob's access
And Bob reloads the chat thread
Then the unfurl for file X is no longer shown to Bob
And Bob cannot open the file through the prior link
```

### Scenario: AI respects permissions
```gherkin
Given folder F contains files A and B
And user U may access file A but not file B
When user U asks AI to summarize folder F
Then the answer includes only content from file A
And the response cites file A
And the response does not reveal the title or contents of file B
```

---

## 16. References

[^1]: OpenID Connect Core 1.0 defines interoperable identity on top of OAuth 2.0. https://openid.net/specs/openid-connect-core-1_0-final.html  
[^2]: SCIM defines standard user/group provisioning and lifecycle semantics. https://datatracker.ietf.org/doc/rfc7643/  
[^3]: Box AI for Hubs: multi-file Q&A with citations, bounded by accessible content. https://support.box.com/hc/en-us/articles/29347206309395-Box-AI-for-Hubs  
[^4]: Box AI extract / Box Extract: extraction of structured metadata from enterprise content. https://support.box.com/hc/en-us/articles/42325695115155-Announcing-Box-AI-Enhanced-Extract-Agent and https://www.box.com/extract  
[^5]: Microsoft Copilot in OneDrive supports summaries and Q&A over files. https://support.microsoft.com/en-gb/office/summarize-your-files-with-copilot-10dcbe50-467d-4a61-9d5e-c98c77fd33a4  
[^6]: Slack Events API documents an event-driven integration model with scoped permissions and secure delivery. https://api.slack.com/apis/connections/events-api  
[^7]: NSA and CISA highlighted the value of memory-safe languages for reducing software vulnerabilities. https://www.nsa.gov/Press-Room/Press-Releases-Statements/Press-Release-View/Article/4223298/nsa-and-cisa-release-csi-highlighting-importance-of-memory-safe-languages-in-so/  
[^8]: AWS Storage Blog documents multi-tenant S3 access patterns including dedicated buckets, shared prefix isolation, and access points. https://aws.amazon.com/blogs/storage/design-patterns-for-multi-tenant-access-control-on-amazon-s3/  
[^9]: AWS Prescriptive Guidance documents tenant isolation for Amazon S3 using a token vending pattern. https://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/implement-saas-tenant-isolation-for-amazon-s3-by-using-an-aws-lambda-token-vending-machine.html  
[^10]: AWS multi-tenant SaaS storage and tenant isolation whitepapers describe partitioning/isolation trade-offs. https://docs.aws.amazon.com/whitepapers/latest/multi-tenant-saas-storage-strategies/multi-tenant-saas-storage-strategies.html and https://docs.aws.amazon.com/whitepapers/latest/saas-tenant-isolation-strategies/saas-tenant-isolation-strategies.html
