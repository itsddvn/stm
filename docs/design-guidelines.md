# STM Design Guidelines

Status: UI Contract v1.1 approved and locked on 2026-08-21 after lifecycle runtime and viewport verification.

## Product direction

STM (Smart Tools Management) is a dense desktop operations product for individual developers. Its visual language is industrial and utilitarian, optimized for ownership, trust, privilege, capability, and recovery decisions.

- `DESIGN_VARIANCE=3`
- `MOTION_INTENSITY=2`
- `VISUAL_DENSITY=6`
- Color strategy: restrained, with safety orange at no more than 10% visual weight.
- Memorable element: the ownership rail linking every tool, skill, or MCP server to its authoritative manager or client.

The soft-play seeded direction was rejected as a poor domain fit. The adjacent industrial direction better supports consequential operations without suggesting playfulness or background automation.

## Tokens

The runtime source is `src/styles/tokens.css`; the durable contract is `contracts/ui/design-token-contract.ts`.

- Canvas and surfaces use cool graphite OKLCH neutrals. Do not mix warm grays.
- Use `--color-accent` only for focus, active selection, ownership continuity, and primary consent.
- Success, warning, danger, and information always pair color with text or a Phosphor icon.
- Use Source Sans 3 for body and interface copy. Use Barlow Condensed only for page and resource display headings. Both support Vietnamese text and are bundled locally through pinned `@fontsource` dependencies with no runtime external font requests.
- Use the 4px spacing scale: 4, 8, 12, 16, 24, 32, 48, 64, and 96px.
- Radius is sharp: 0, 4, or 8px. Do not introduce pills or rounded cards.
- Depth uses hairline borders and surface tints. Do not add card shadows, gradients, or glass effects.

## Layout

- Minimum supported viewport: 1024x720.
- Review viewports: 1024x720, 1280x800, and 1440x900.
- Persistent desktop sidebar, sticky utility bar, and a single main scroll region.
- Tools, Skills, and MCP Servers use master-detail layout. Updates and History use data tables with adjacent detail or boundary panels.
- At tighter desktop widths, secondary metadata collapses before core labels, state, and actions.
- All targets remain at least 44x44px. Body inputs remain at least 16px.

## Components

- One icon family: Phosphor Icons, regular weight by default and fill only for selected navigation or result emphasis.
- Buttons have default, hover, focus-visible, active, disabled, and consent-gated states.
- Native `dialog` provides Escape handling, focus containment, and focus return.
- Status badges state the condition in text. Color never carries the meaning alone.
- Lists use hairlines and row selection, not generic SaaS cards.
- Loading uses stable skeleton rows. Empty states explain how to reach another fixture state.
- Lifecycle reviews use one dense evidence hierarchy: fixture-only simulation banner, identity/version grid, exact managed command or vendor handoff, affected resources, limitations/revalidation, digest/expiry, then consent.
- Bulk lifecycle review nests bordered child plans without turning them into cards. Each child keeps its authoritative execution boundary visible.
- Execution views keep progress and cancellation above per-item results, receipts, redacted detail, and follow-up plan actions.

## Motion

- Motion communicates state only and lasts 100 to 250ms.
- Animate color, opacity, or transform. Never use `transition: all`.
- No page-load choreography, decorative loops, parallax, bounce, or animated backgrounds.
- `prefers-reduced-motion` reduces animation and transition duration to 1ms.

## Accessibility

- Keyboard navigation follows visual order. Route changes focus the page heading.
- A skip link reaches main content.
- Focus-visible uses a 2px high-contrast ring.
- Controls use semantic links, buttons, inputs, labels, tables or list roles, and dialog semantics.
- Dialogs contain named headings and descriptions. Dynamic states use polite live regions.
- Contrast target: 4.5:1 for normal text and 3:1 for large text and meaningful controls.
- Copy uses plain active language. Visible UI contains no em dash or marketing clichés.

## Prohibited patterns

No gradients, glassmorphism, purple, raw black or white, ghost cards, mixed icon families, over-rounding, hidden focus rings, silent mutation, decorative status dots, or backend-derived decisions in React.

## Approval record

- Project lead approved the running STM interface and eleven PNG baselines on 2026-08-20.
- Project lead approved the verified UI Contract v1.1 running interface and lifecycle viewport matrix on 2026-08-21.
- Manifest `1.1.0` is locked against the regenerated v1.1 artifact digests. Any intentional UI change requires explicit reopen, version bump, re-verification, approval, and lock regeneration.
