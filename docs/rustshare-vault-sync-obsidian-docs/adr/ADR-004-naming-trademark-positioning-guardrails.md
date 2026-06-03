# ADR-004: Naming, Trademark, and Product Positioning Guardrails

## Status

Accepted.

## Context

RustShare plans to support synchronization of local Markdown-based vaults used by Obsidian clients. Obsidian also provides its own paid cloud synchronization product. Because of this, RustShare must avoid any naming, framing, UI wording, documentation language, or implementation language that could imply that RustShare is an official Obsidian product, an Obsidian-approved service, a replacement for Obsidian’s paid sync service, or a reverse-engineered implementation of Obsidian’s own sync service.

## Decision

RustShare will implement this feature as a generic vault synchronization capability with an Obsidian vault adapter.

Preferred product name:

```text
RustShare Vault Sync
```

Preferred adapter description:

```text
Obsidian vault support
Obsidian vault adapter
Obsidian vault connector
Support for local Obsidian vault folders
```

## Forbidden Names and Phrases

The following names and phrases must not appear in customer-facing UI, documentation, marketing material, release notes, website copy, plugin marketplace copy, or sales material:

```text
Obsidian Sync replacement
Obsidian Sync alternative
Obsidian Cloud
RustShare Obsidian Cloud
Official Obsidian integration
Official Obsidian Sync
Obsidian-compatible cloud
Powered by Obsidian
Obsidian-approved
Obsidian-certified
Better than Obsidian Sync
Drop-in replacement for Obsidian Sync
Obsidian enterprise sync
Obsidian team sync
```

These phrases may appear only inside this ADR, compliance specs, blocklists, and tests that intentionally detect forbidden terminology.

## Allowed Descriptive Phrases

```text
RustShare Vault Sync
Obsidian vault support
Obsidian vault adapter
Connect a local Obsidian vault
Sync local Markdown vaults to RustShare
Store Obsidian vault files in RustShare
Open in Obsidian
Works with local Obsidian vault folders
Self-hosted vault storage for teams using Obsidian
```

## Required Disclaimer

All public documentation and plugin descriptions must include:

```text
Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
```

## Technical Constraints

RustShare must not:

```text
- reverse engineer Obsidian Sync
- use private or undocumented Obsidian Sync APIs
- intercept Obsidian Sync traffic
- copy Obsidian Sync protocol behavior through protocol analysis
- modify or redistribute the Obsidian application
- bundle Obsidian with RustShare
- claim compatibility with Obsidian Sync
- use Obsidian logos, app icons, or commercial brand assets without permission
```

RustShare may:

```text
- use the public Obsidian plugin API
- read and write local vault files with user permission
- preserve Markdown, attachments, folders, and links
- provide RustShare-owned sync APIs
- provide an Obsidian plugin that talks only to RustShare APIs
```

## API Naming Decision

Do not use this as the primary namespace:

```text
/api/obsidian-sync/v1
```

Use:

```text
/api/vault-sync/v1
```

Represent Obsidian as an adapter:

```json
{
  "adapter": "obsidian_vault"
}
```

## CI / Review Requirement

The repository should include a terminology check that fails or warns when forbidden phrases appear outside allowlisted compliance files.

## Acceptance Criteria

```text
- No customer-facing page calls the feature an Obsidian Sync replacement.
- No UI label implies official Obsidian affiliation.
- The plugin does not use Obsidian logos, icons, or copied branding.
- The implementation uses only public Obsidian plugin APIs and local vault files.
- RustShare backend APIs are RustShare-owned and generic.
- Public documentation includes the non-affiliation disclaimer.
- LLM implementation prompts include these naming and framing restrictions.
- Pull requests introducing forbidden terminology must be rejected or corrected before merge.
```
