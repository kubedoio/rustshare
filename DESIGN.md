# Design System — RustShare

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
- **Display/Hero:** `Fraunces` — used sparingly for top-level page titles, login branding, and high-signal empty states. It gives RustShare a distinct face without making the app decorative.
- **Body:** `Instrument Sans` — default reading and interface font for forms, cards, navigation, and most content. Clear, modern, and less overused than common SaaS defaults.
- **UI/Labels:** `Instrument Sans` — medium and semibold weights for tabs, filters, buttons, and section labels.
- **Data/Tables:** `IBM Plex Sans` — for dense list views, share governance surfaces, audit screens, and metadata-heavy layouts. Use tabular figures where available.
- **Code:** `IBM Plex Mono` — for object IDs, sync metadata, device codes, event names, hashes, and audit details.
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
- **Primary:** `#C65A1E` — rust copper, the main action color for upload, share, primary CTA, selected states, and brand anchors.
- **Primary strong:** `#A34716` — hover, active, and stronger emphasis.
- **Secondary:** `#7B4A2E` — warm metal tone for secondary accents and premium highlights.
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
- **Breakpoints:**
  - `sm`: 640px
  - `md`: 768px
  - `lg`: 1024px
  - `xl`: 1280px
  - `2xl`: 1536px
- **Max content width:** `1440px` for standard pages, with file workspace allowed to run nearly full width inside the shell.
- **Border radius:**
  - `sm`: `0px`
  - `md`: `0px`
  - `lg`: `0px`
  - `xl`: `0px`
  - `pill`: `0px`
- **Guideline:** Zero border-radius everywhere. Sharp corners signal precision engineering and deliberately depart from the rounded, bubbly SaaS convention. Every component, button, card, modal, and input uses 0px radius.

## Elevation & Shadows
- **Approach:** Layered shadows for raised surfaces, no decorative blur
- **Scale:**
  - `0`: none — flat surfaces, base layers
  - `1`: `0 1px 2px rgba(21,21,21,0.06)` — subtle lift for cards, inputs
  - `2`: `0 2px 8px rgba(21,21,21,0.08)` — dropdowns, menus, popovers
  - `3`: `0 4px 16px rgba(21,21,21,0.10)` — modals, drawers, focused panels
  - `4`: `0 8px 32px rgba(21,21,21,0.12)` — full-screen overlays, command palette
- **Dark mode shadows:** Increase opacity to maintain visibility on dark backgrounds:
  - `1`: `0 1px 2px rgba(0,0,0,0.30)`
  - `2`: `0 2px 8px rgba(0,0,0,0.35)`
  - `3`: `0 4px 16px rgba(0,0,0,0.40)`
  - `4`: `0 8px 32px rgba(0,0,0,0.50)`

## Z-Index Scale
- **Approach:** Explicit numbered scale to prevent z-index wars
- **Scale:**
  - `0`: base content
  - `10`: sticky headers, floating toolbar
  - `20`: dropdowns, menus
  - `30`: drawers, side panels
  - `40`: modals, dialogs
  - `50`: tooltips, toasts, notifications

## Iconography
- **Style:** Consistent stroke icons with slightly technical character
- **Weight:** 1.5px-2px stroke, no filled icons as default
- **Character:** Geometric, precise, minimal embellishment
- **Anti-patterns:**
  - No emoji as first-class UI iconography
  - No mixed icon sets (do not combine outline and filled styles)
  - No decorative icon bubbles or colored-circle icon backgrounds as a default pattern
  - No overly playful or hand-drawn icon styles

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

## Component States
- **Approach:** Every interactive element must have explicit, distinguishable states
- **Hover:** Slight background shift or border intensification. Never rely on color alone.
  - Buttons: `background-color` shifts 8% darker, or border-color intensifies
  - Links: underline appears or color shifts to primary strong
  - Table rows: `color-mix(in srgb, var(--rs-brand) 4%, transparent)` background
- **Focus:** `outline: 2px solid var(--rs-brand)` with `outline-offset: 2px`. Never remove focus rings.
  - Focus-visible only (not on mouse click)
  - Dark mode: same ring, but ensure 3:1 contrast against dark surface
- **Active/Pressed:** `transform: translateY(1px)` + 12% darker background
- **Disabled:** `opacity: 0.45`, `cursor: not-allowed`, remove hover effects
  - Never use a different color for disabled — opacity reduction preserves palette coherence
- **Selected:** `background-color: color-mix(in srgb, var(--rs-brand) 12%, transparent)` + `box-shadow: inset 0 0 0 2px var(--rs-brand)`
- **Error state:** `border-color: var(--rs-error)` + error text below field in `text-body-sm`
- **Loading state:** Skeleton shimmer using `linear-gradient(90deg, transparent, rgba(255,255,255,0.08), transparent)` on dark, reversed opacity on light

## Form & Input Specifications
- **Text inputs:** `height: 40px`, `padding: 0 12px`, `border: 1px solid var(--rs-border-soft)`, `background: var(--rs-surface-raised)`
  - Focus: border transitions to `var(--rs-brand)` with `160ms` easing
  - Error: border is `var(--rs-error)`, no shake animation
  - Placeholder: `color: var(--rs-text-muted)` at `opacity: 0.7`
- **Textareas:** Same border treatment, `min-height: 96px`, `padding: 12px`
- **Selects/Dropdowns:** Trigger matches input height. Dropdown panel uses `elevation-2` shadow.
- **Checkboxes & Radios:** 16px square, 2px border, sharp corners (0px radius). Checked state fills with `var(--rs-brand)`.
- **Toggles:** 40px wide × 20px tall track, 16px thumb. Track uses `var(--rs-border-strong)` off, `var(--rs-brand)` on.
- **Labels:** `text-body-sm` at `font-weight: 500`, `margin-bottom: 4px`
- **Helper text:** `text-meta`, `color: var(--rs-text-muted)`, below field
- **Form spacing:** `gap: 16px` between fields, `gap: 24px` between sections

## File-Type Color Mapping
- **Documents:** `var(--rs-text)` (default ink) — neutral, no distraction
- **Images:** `var(--rs-info)` — trustworthy, associated with visual media
- **Videos:** `var(--rs-secondary)` — warm metal, distinct from images
- **Audio:** `var(--rs-warning)` — amber signals sound/alert
- **Archives:** `var(--rs-text-muted)` — subdued, utility
- **Code/Scripts:** `var(--rs-success)` — green for "executable/buildable"
- **Spreadsheets:** `var(--rs-brand)` — copper for data/calculation
- **Markdown/Notes:** `var(--rs-info)` — same as images, knowledge-related
- **CAD/Design files:** `var(--rs-secondary)` — premium, engineering-related
- **Unknown:** `var(--rs-text-muted)` — neutral fallback
- **Rule:** Never use purple (`#8b5cf6`), bright blue (`#3b82f6`), or any color outside the semantic palette for file type indicators.

## Empty States
- **Approach:** Reassure and orient, never look like marketing cards
- **Layout:** Centered vertically and horizontally within the viewport or panel
- **Icon:** 48px stroke icon, `color: var(--rs-text-muted)`, no colored-circle background
- **Title:** `text-title-md`, `font-weight: 600`, product-specific copy (not "No data yet")
- **Body:** `text-body-sm`, `color: var(--rs-text-muted)`, max-width 320px, explains what the area is for
- **Action:** Optional primary button below body text, only if there's a clear next step
- **Examples:**
  - Empty folder: "This folder is empty. Drag files here or use the upload button."
  - Empty shares: "No active shares. Share a file to see it here."
  - Empty search: "No results for 'query'. Try a different term or check filters."

## Loading States
- **Skeletons:** Use `IBM Plex Sans` at matching text sizes. Blocks are rectangles with `background: var(--rs-border-soft)` and `border-radius: 0px`.
  - Shimmer: `linear-gradient(90deg, transparent, rgba(255,255,255,0.06), transparent)` animated across
  - Dark mode: reverse shimmer opacity to `rgba(0,0,0,0.10)`
- **Spinners:** 16px or 24px rotating stroke circle, `stroke: var(--rs-brand)`, `stroke-width: 2px`, no fill.
- **Progress bars:** 4px height, `background: var(--rs-border-soft)`, fill is `var(--rs-brand)`. No rounded caps.
- **Button loading:** Replace text with spinner, keep button dimensions stable, disabled interaction

## Toast & Notification Patterns
- **Position:** Bottom-right on desktop, bottom-center on mobile, `z-index: 50`
- **Container:** `elevation-3` shadow, `background: var(--rs-surface-raised)`, `border-left: 3px solid` colored by type
- **Types:**
  - Success: left border `var(--rs-success)`, icon checkmark
  - Error: left border `var(--rs-error)`, icon alert-triangle
  - Warning: left border `var(--rs-warning)`, icon alert-circle
  - Info: left border `var(--rs-info)`, icon info-circle
- **Typography:** Title `text-body-sm` weight 600, body `text-meta` weight 400
- **Duration:** Auto-dismiss after 4s for success/info, 8s for error/warning
- **Spacing:** `gap: 8px` between stacked toasts, max 3 visible

## Modal & Dialog Patterns
- **Overlay:** `background: rgba(21,21,21,0.55)` light mode, `rgba(0,0,0,0.65)` dark mode
- **Panel:** `background: var(--rs-surface-raised)`, `elevation-4` shadow, max-width 560px (standard), 720px (wide), full-width on mobile
- **Header:** `padding: 20px 24px`, title `text-title-md`, close button top-right
- **Body:** `padding: 0 24px 20px`, `max-height: 70vh`, scrollable
- **Footer:** `padding: 16px 24px`, `border-top: 1px solid var(--rs-border-soft)`, actions right-aligned with `gap: 12px`
- **Entrance:** `opacity 0→1` + `translateY(8px)→0`, `160ms` enter easing
- **Exit:** Reverse, `120ms` exit easing
- **Focus trap:** First focusable element inside modal on open, restore focus on close

## Focus & Accessibility
- **Focus rings:** `outline: 2px solid var(--rs-brand)` with `outline-offset: 2px`
  - Minimum 3:1 contrast against adjacent background
  - Never remove on `:focus-visible`
  - Skip on mouse clicks where appropriate (`:focus-visible` only)
- **Contrast minimums:**
  - Normal text (body, labels): 4.5:1 against background
  - Large text (titles, display): 3:1 against background
  - UI components (buttons, inputs): 3:1 for boundaries
  - Disabled text: no minimum, but must be clearly distinguishable from active
- **Motion respect:** Honor `prefers-reduced-motion` — replace all transitions with `instant` or `opacity-only`
- **Scrollbar styling:** Styled scrollbars are allowed but must use `border-radius: 0px` on thumb (not 999px)

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
- No uniform bubbly radius on everything (enforced: 0px everywhere)
- No emoji as first-class UI iconography
- No dark mode that is just "same theme on black"
- No dashboard cards that look prettier than the actual file workflow

## Implementation Notes
- Replace the current `fontFamily.sans` stack with the approved font system.
- Promote design tokens into named CSS variables and DaisyUI theme values.
- Keep light and dark themes paired, not independent improvisations.
- Audit every file-management surface against this system before introducing new visual patterns.
- Ensure all components respect the zero border-radius rule — this is a deliberate departure from SaaS conventions.

## Known Code Divergences (fix these)
- `tailwind.config.js` defines `borderRadius` values (6px, 10px, 14px, 20px) — contradicts zero-radius rule. Remove or set all to `0px`.
- `frontend/src/routes/layout.css` defines DaisyUI theme with `--radius-selector: 0.625rem`, `--radius-field: 0.625rem`, `--radius-box: 1.25rem` — set all to `0rem`.
- `frontend/src/routes/layout.css` defines `--rs-accent-purple: #8b5cf6` (light) and `#a78bfa` (dark) — remove or replace with approved semantic colors.
- `frontend/src/routes/invite/[token]/+page.svelte` has `bg-purple-500/10 blur-3xl rounded-full` decorative blob — remove or replace with approved intentional decoration.
- `frontend/src/lib/components/modules/KanbanModuleView.svelte` uses Trello colors (`#eb5a46`, `#ff9f1a`, `#61bd4f`, `#0079bf`, `#c377e0`) — map to approved semantic palette.
- `frontend/src/lib/components/modules/KanbanSummaryWidget.svelte` uses same Trello colors — map to approved semantic palette.
- `frontend/src/lib/components/modals/EditFileModal.svelte` uses `text-blue-500` and `text-purple-500` for file type icons — use File-Type Color Mapping above.
- `frontend/src/lib/components/modals/CreateFileModal.svelte` uses `text-blue-500` and `text-purple-500` for file type icons — use File-Type Color Mapping above.
- `frontend/src/lib/components/preview/OfficePreview.svelte` uses `text-blue-500` and `bg-blue-50` — map to approved semantic colors.
- `frontend/src/lib/files/FileTypeIcon.svelte` uses `text-blue-400` for default file type — use approved File-Type Color Mapping.
- `frontend/src/lib/files/FileTable.svelte` uses `text-blue-400` for folder icons — use approved semantic colors.
- `frontend/src/routes/(app)/settings/+page.svelte` uses `text-blue-400` — map to approved semantic colors.
- `frontend/src/routes/layout.css` scrollbar thumb uses `border-radius: 999px` — change to `0px` to respect zero-radius rule.
- `frontend/src/routes/layout.css` body background uses `radial-gradient` decorative blobs — this is acceptable as "intentional decoration" but should be documented if kept.

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
| 2026-04-01 | Fixed login page 3-column feature grid | Converted from symmetric 3-column cards (AI Slop Pattern #2) to vertical stack with icon+text rows — more intentional, less generic SaaS template look |
| 2026-04-21 | Adopted zero border-radius everywhere | Deliberate risk: sharp corners signal precision engineering and depart from the rounded SaaS convention. Chosen via /design-consultation |
| 2026-04-21 | Added breakpoints, elevation, z-index, and iconography sections | Filled structural gaps in the existing system for implementer clarity |
| 2026-05-03 | Added component states, forms, file-type colors, empty states, loading, toasts, modals, accessibility, and focus specifications | Filled implementation gaps discovered in codebase audit |
| 2026-05-03 | Documented known code divergences | Purple accents, Trello kanban colors, blue file-type icons, rounded corners in tailwind config and DaisyUI theme all violate the design system and need cleanup |
