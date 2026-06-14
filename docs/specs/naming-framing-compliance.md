# SPEC-005: Naming and Framing Compliance

## Scope

This specification applies to:

```text
backend API names
frontend labels
plugin labels
README files
documentation
website copy
sales material
release notes
LLM prompts
tests
example data
```

## Required Product Name

```text
RustShare Vault Sync
```

## Required Disclaimer

```text
Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
```

## Forbidden Customer-Facing Terms

```text
Obsidian Sync replacement
Obsidian Sync alternative
Obsidian Cloud
RustShare Obsidian Cloud
Obsidian-compatible cloud
Official Obsidian integration
Official Obsidian plugin
Official Obsidian sync
Obsidian-approved
Obsidian-certified
Powered by Obsidian
Better than Obsidian Sync
Drop-in replacement for Obsidian Sync
Enterprise Obsidian Sync
Obsidian team sync
```

These terms may appear only in compliance/blocklist files.

## Allowed Customer-Facing Terms

```text
RustShare Vault Sync
Obsidian vault support
Obsidian vault connector
Connect a local Obsidian vault
Sync local Markdown vaults to RustShare
Store Obsidian vault files in RustShare
Open in Obsidian
Works with local Obsidian vault folders
Self-hosted vault storage for teams using Obsidian
```

## Backend Naming

Allowed:

```text
/api/vault-sync/v1
adapter = "obsidian_vault"
source_type = "external_vault"
```

Avoid:

```text
/api/obsidian-sync/v1
/api/obsidian-cloud/v1
/api/obsidian-sync-replacement/v1
```

## LLM Prompt Requirement

Every implementation prompt must include:

```text
Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”
```

## Acceptance Criteria

```text
- README includes disclaimer.
- Plugin description includes disclaimer.
- UI does not include forbidden terms.
- API namespace is generic.
- CI or review checklist checks terminology.
- Forbidden terminology appears only in compliance docs and tests.
```
