# Terminology Blocklist

## Required Disclaimer

```text
Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
```

## Forbidden Customer-Facing Terms

These terms must not appear in UI, marketing, README, release notes, website copy, marketplace text, or sales material.

They may appear only in this blocklist, ADR-004, SPEC-005, and tests that intentionally detect forbidden terms.

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

## Approved Terms

```text
RustShare Vault Sync
Obsidian vault support
Obsidian vault adapter
Obsidian vault connector
Connect a local Obsidian vault
Sync local Markdown vaults to RustShare
Store Obsidian vault files in RustShare
Open in Obsidian
Works with local Obsidian vault folders
Self-hosted vault storage for teams using Obsidian
```

## Suggested Grep Pattern

```bash
rg -n "Obsidian Sync replacement|Obsidian Sync alternative|Obsidian Cloud|RustShare Obsidian Cloud|Obsidian-compatible cloud|Official Obsidian integration|Official Obsidian plugin|Official Obsidian sync|Obsidian-approved|Obsidian-certified|Powered by Obsidian|Better than Obsidian Sync|Drop-in replacement for Obsidian Sync|Enterprise Obsidian Sync|Obsidian team sync"   --glob '!docs/adr/ADR-004-*'   --glob '!docs/specs/SPEC-005-*'   --glob '!docs/checklists/TERMINOLOGY_BLOCKLIST.md'
```
