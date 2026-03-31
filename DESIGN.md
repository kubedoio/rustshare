# Design System - RustShare

## Product Context
- **What this is:** RustShare is a self-hosted or private-cloud file-sharing and sync platform for technical teams and regulated organizations. It combines file management, controlled sharing, auditability, and permission-aware AI on authorized content.
- **Who it's for:** platform teams, internal IT, MSPs, technical SMBs, and regulated mid-market organizations that care about sovereignty, operational control, and predictable permissions.
- **Space/industry:** secure file sharing, private-cloud collaboration, governed content platforms.
- **Project type:** web application and operations dashboard, with desktop sync and device flows around it.

## Aesthetic Direction
- **Direction:** Industrial refined
- **Decoration level:** Intentional
- **Mood:** Calm, operational, and credible. The product should feel like trustworthy infrastructure with product polish, not like a generic admin panel or a soft consumer cloud app.
- **Reference posture:** More "governed content workspace" than "friendly storage toy." The UI should make permissions, auditability, and reliability feel native.

## Typography
- **Display/Hero:** `Fraunces` - used sparingly for top-level page titles, login branding, and high-signal empty states. It gives RustShare a distinct face without making the app decorative.
- **Body:** `Instrument Sans` - default reading and interface font for forms, cards, navigation, and most content. Clear, modern, and less overused than common SaaS defaults.
- **UI/Labels:** `Instrument Sans` - medium and semibold weights for tabs, filters, buttons, and section labels.
- **Data/Tables:** `IBM Plex Sans` - for dense list views, share governance surfaces, audit screens, and metadata-heavy layouts. Use tabular figures where available.
- **Code:** `IBM Plex Mono` - for object IDs, sync metadata, device codes, event names, hashes, and audit details.
- **Loading:** Use Google Fonts or self-host:
  - `Fraunces: 600,700`
  - `Instrument Sans: 400,500,600,700`
  - `IBM Plex Sans: 400,500,600`
  - `IBM Plex Mono: 400,500`
- **Scale:**
  - `text-display-2xl`: 3.5rem / 1.0 / 600
  - `text-display-xl`: 2.75rem / 1.05 / 600
  - `text-display-lg`: 2.125rem / 1.1 / 600
  - `text-title-xl`: 1.75rem / 1.2 / 600
  - `text-title-lg`: 1.375rem / 1.25 / 600
  - `text-title-md`: 1.125rem / 1.35 / 600
  - `text-body-lg`: 1rem / 1.6 / 400
  - `text-body-md`: 0.9375rem / 1.55 / 400
  - `text-body-sm`: 0.875rem / 1.45 / 400
  - `text-meta`: 0.75rem / 1.35 / 500
  - `text-mono-sm`: 0.8125rem / 1.4 / 500

## Color
- **Approach:** Restrained with a warm brand core
- **Primary:** `#C65A1E` - rust copper, the main action color for upload, share, primary CTA, selected states, and brand anchors.
- **Primary strong:** `#A34716` - hover, active, and stronger emphasis.
- **Secondary:** `#7B4A2E` - warm metal tone for secondary accents and premium highlights.
- **Neutrals:**
  - `canvas`: `#F6F3EE`
  - `surface`: `#FBF9F5`
  - `surface-raised`: `#FFFFFF`
  - `panel-muted`: `#EFE8DE`
  - `border-soft`: `#DED6CA`
  - `border-strong`: `#C7BBAA`
  - `ink`: `#151515`
  - `ink-soft`: `#3E3A35`
  - `ink-muted`: `#6C665F`
- **Semantic:**
  - success `#1D7A52`
  - warning `#A56A12`
  - error `#B63E3E`
  - info `#366D8C`
- **Dark mode:** Do not invert the light theme mechanically. Use a charcoal-metal base with lower-saturation warm accents:
  - `base-100`: `#121315`
  - `base-200`: `#181A1D`
  - `base-300`: `#24272C`
  - `base-content`: `#F3EFE8`
  - keep rust accents, but reduce warmth slightly on low-contrast surfaces
  - use lighter border separation than glow

## Spacing
- **Base unit:** `8px`
- **Density:** Comfortable compact
- **Scale:** `2xs(4) xs(8) sm(12) md(16) lg(24) xl(32) 2xl(48) 3xl(64)`
- **Guideline:** App chrome should feel tight and deliberate. Detail pages and empty states may breathe more, but file lists, toolbars, and share management screens should favor clarity over spaciousness.

## Layout
- **Approach:** Grid disciplined
- **Grid:**
  - mobile: 4 columns
  - tablet: 8 columns
  - desktop app shell: 12 columns
  - dense detail screens may use split panels rather than wider freeform cards
- **Max content width:** `1440px` for standard pages, with file workspace allowed to run nearly full width inside the shell.
- **Border radius:**
  - `sm`: `6px`
  - `md`: `10px`
  - `lg`: `14px`
  - `xl`: `20px`
  - `pill`: `999px`
- **Guideline:** Avoid giving every component the same radius. Buttons, chips, panels, and modal shells should have a hierarchy.

## Motion
- **Approach:** Minimal functional
- **Easing:**
  - enter: `cubic-bezier(0.22, 1, 0.36, 1)`
  - exit: `cubic-bezier(0.4, 0, 1, 1)`
  - move: `cubic-bezier(0.2, 0.8, 0.2, 1)`
- **Duration:**
  - micro: `80ms`
  - short: `160ms`
  - medium: `240ms`
  - long: `360ms`
- **Guideline:** Motion should explain sidebar collapse, row selection, menu opening, upload progress, and modal entry. Do not use decorative parallax, overscaled springiness, or playful bounce.

## Components
- **Topbar:** Compact, with more visual authority in search and breadcrumbs. Sidebar toggle should feel mechanical and precise, not soft.
- **Buttons:** Primary actions use rust copper fills. Secondary actions are surface-backed with stronger borders. Ghost buttons are for table actions and supporting controls only.
- **Cards/Panels:** Prefer quiet surfaces with strong edge definition over tinted gradient blocks. Use tonal panels to separate states like "active share," "audit," or "warning."
- **Tables/List views:** Data-heavy surfaces should use `IBM Plex Sans` for headers and metadata. Row action menus must never clip, and menus should feel like control surfaces, not floating toy popovers.
- **Badges:** Use semantic chips sparingly. Keep them small, with low-chroma fills and strong text contrast.
- **Icons:** Use consistent stroke icons with slightly technical character. Avoid emoji, mixed icon sets, and decorative icon bubbles as a default pattern.

## UX Rules
- Permission clarity beats cleverness.
- Audit and governance screens should feel as polished as the happy path, not like admin leftovers.
- File and share actions must be obvious, but destructive actions must never visually compete with the primary flow.
- Empty states should reassure and orient. They should not look like marketing cards dropped into the app.
- Mobile layouts should preserve capability, not just shrink the desktop UI.

## Anti-Patterns
- No default purple or blue SaaS gradients
- No all-Inter typography system
- No three-column generic feature-card patterns inside the app
- No uniform bubbly radius on everything
- No emoji as first-class UI iconography
- No dark mode that is just "same theme on black"
- No dashboard cards that look prettier than the actual file workflow

## Implementation Notes
- Replace the current `fontFamily.sans` stack with the approved font system.
- Promote design tokens into named CSS variables and DaisyUI theme values.
- Keep light and dark themes paired, not independent improvisations.
- Audit every file-management surface against this system before introducing new visual patterns.

## Decisions Log
| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-03-28 | Initial design system created | Defined from the rebuild docs, current RustShare UI, and the product thesis of secure, private-cloud file operations |
| 2026-03-28 | Chose industrial-refined visual direction | Matches governance, auditability, and technical-team positioning better than consumer-cloud aesthetics |
| 2026-03-28 | Chose Fraunces + Instrument Sans + IBM Plex pairing | Gives the product a distinct face while keeping dense operational surfaces readable |
| 2026-04-01 | Design audit completed | Fixed hardcoded purple colors, added border-radius scale to Tailwind config, applied font-data to admin tables |
| 2026-04-01 | Replaced purple-400 with info color | Purple not in design system; info (#366d8c) provides good contrast for image file indicators |
| 2026-04-01 | Added tabular-nums to quota columns | Improves readability for numeric data in data tables |
| 2026-04-01 | Desaturated dark mode primary color | `#d66a2d` → `#c46a35` (55% vs 67% saturation) — reduces eye strain on dark backgrounds per DESIGN.md guidance |
