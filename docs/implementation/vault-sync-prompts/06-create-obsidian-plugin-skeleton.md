# Prompt 06: Create Obsidian Plugin Skeleton

```text
You are creating the separate TypeScript plugin package for RustShare Vault Sync.

Naming and product-positioning guardrail:
This feature must be implemented as RustShare Vault Sync with support for local Obsidian vaults. Do not call it an Obsidian Sync replacement, Obsidian Sync alternative, Obsidian Cloud, official Obsidian integration, or any phrase that implies affiliation, endorsement, certification, or replacement of Obsidian’s paid sync service. Use only public Obsidian plugin APIs and local vault files. Do not reverse engineer Obsidian Sync or use private Obsidian APIs. Do not use Obsidian logos, icons, brand colors, or commercial brand assets. Include the disclaimer: “Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.”

Task:
Create a desktop-first Obsidian plugin skeleton.

Plugin name:
RustShare Vault Sync

Acceptable description:
Self-hosted vault synchronization for local Markdown vaults used with Obsidian.

Required files:
- manifest.json
- main.ts
- styles.css
- package.json
- tsconfig.json
- esbuild.config.mjs or equivalent
- README.md
- src/settings.ts
- src/auth.ts
- src/remote-client.ts
- src/local-vault-scanner.ts
- src/sync-engine.ts
- src/conflict-resolver.ts
- src/status.ts

Requirements:
- Include non-affiliation disclaimer in README.
- Do not use Obsidian logo or brand assets.
- Do not use private APIs.
- Prefer APIs compatible with future mobile support where possible.
- If Node/Electron APIs are used, set isDesktopOnly appropriately.
- Settings page must include RustShare URL and token/device login placeholder.
- Status bar item must show disconnected/connected/syncing/synced/error.

Output:
- Plugin scaffold.
- Build instructions.
- Dev install instructions.
- List of next implementation steps.
```
