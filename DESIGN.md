# DESIGN.md — Smart-Money Shadow visual world

Mode: **Operate** (dashboard, task-focused). Method: distilled from Impeccable
(pbakaus/impeccable) craft-floor + operate references — this file is our own
persistent design skill; all UI work must comply.

## World

"A trading desk terminal at 2am." Dense, calm, phosphorescent. The screen is dark
glass; data glows on it. Nothing decorative survives: every pixel is either data,
state, or wayfinding.

## Non-negotiables (from Impeccable craft floor, adapted)

- No pure black/gray — every neutral is tinted toward blue-green.
- No purple/blue gradients, no gradient text, no glass blur as decoration.
- No card-in-card nesting; elevation declared ONCE per surface (1px border, no shadow+border ghosts).
- No kicker/eyebrow labels above headings; section headers are small uppercase micro-labels, nothing else.
- No emoji/unicode glyphs as icons; no bounce/elastic easing; motion 150–250ms, state-conveying only.
- Numbers in data contexts ALWAYS use tabular figures (`font-variant-numeric: tabular-nums`).
- Browser surfaces are themed: selection, scrollbars, focus rings.

## Typography

- **UI / display:** Space Grotesk (500/700). Headings tight tracking (-0.02em), no fluid sizing.
- **Data / mono:** JetBrains Mono (400/500/700) with `tabular-nums`. Prices, sizes, signatures,
  addresses, timestamps, all metrics.
- Scale (fixed rem): micro-label 10px/uppercase/tracking +0.08em; body 13px; data 12–13px mono;
  section title 15px; hero metric max 20px. Ratio ≈1.15–1.2, no jumps larger than 1.25×.

## Palette (all tinted, single accent family)

- `--bg` #070b10 (page) · `--panel` #0b1118 · `--panel-2` #0f1620 (secondary layer for strip/header)
- `--line` #182230 (borders) · `--line-strong` #243244
- `--ink` #d9e2ec (primary text) · `--ink-dim` #8494a8 (secondary, tinted, ≥4.5:1 on panel)
- `--buy` #34d399 (emerald-400) · `--sell` #f87171 (red-400)
- `--accent` #22d3ee (cyan-400) — brand/interactive only: links, LIVE pulse, focus rings
- `--warn` #fbbf24 (amber-400) — risk badges mid/high, dry-run markers

Semantic states: hover = lighten 4%, active = lighten 8%, disabled = 40% opacity,
error text = --sell, success = --buy. Accent never decorates.

## Components

- Panels: `background: var(--panel); border: 1px solid var(--line); border-radius: 10px;` no shadows.
- Rows/items inside panels are divided by `1px var(--line)`, NOT nested cards.
- Badges: 10px uppercase mono, 1px border in the state color at 40% alpha, text in state color, bg transparent.
- Risk meter: 24px segmented bar (not a pill with a number alone).
- LIVE indicator: 6px dot, `--buy`, 2s opacity pulse; nothing else on the page pulses.
- Flash-on-update: 1.2s background flash on changed rows (existing flash-in), ease-out, transform/opacity only.
- Ticker: continuous marquee, mono 11px, buy green / sell red, separated by thin dividers.

## Layout

- 3 columns ≥1280px: signals 40% / token board 30% / positions+trades 30%. Below that, stack.
- Status strip: single row instrument cluster, segments separated by 1px vertical lines.
- Spacing scale: 4/8/12/16/24. Tight groups, generous separation between panels (16px).
- More space above a section header than below it.

## Anti-goals

Anything that reads "SaaS template": hero metrics with captions, icon tiles, marketing copy,
feature cards, rounded-2xl everywhere, bounce easing, skeleton screens that outstay the data.
