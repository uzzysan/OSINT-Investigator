# 10. Design System

The visual and interaction vocabulary for the web UI (M3 onward). The living
version is a design canvas — brand sheet, colour, typography, components, motion,
and the workspace at 1440×900 and 390×844 in both themes. This document is the
implementable half: tokens, rules, and the reasoning behind them.

---

## 10.1 Principles

1. **The graph is the document; chrome gets out of its way.** Panels are quiet,
   the canvas is where contrast and colour live.
2. **Every visual difference means something.** Colour encodes schema. Border
   style encodes confidence. Amber means "unresolved". Nothing is decorative
   that could be mistaken for semantic.
3. **Never encode by colour alone.** Confidence uses border style, matches use an
   icon and a count, unsourced facts use a mark. A greyscale printout of any
   screen still reads correctly.
4. **Depth over decoration.** Gradients, glass and highlights exist to separate
   layers over a busy canvas. Where nothing sits behind a surface, it gets a
   border instead.
5. **Motion reports, it does not perform.** Something moved because something
   changed. Layout never drifts under a reader's eyes.
6. **Dense by default, never cramped.** This is a tool for long sessions —
   34px controls, 13px table text, and real space between groups.

---

## 10.2 Colour tokens

Authored in oklch, shipped as hex. Dark is the primary theme; light is designed
separately, not derived by inversion.

```css
:root {
  /* surfaces */
  --bg-void:    #F4F6F9;  --bg-base:   #FAFBFD;
  --bg-panel:   #FFFFFF;  --bg-raised: #EEF2F7;
  --line:       #DCE3EC;  --line-strong: #C9D3DF;
  /* text */
  --fg:         #111820;  --fg-2: #47566A;  --fg-muted: #64717F;
  /* accents */
  --accent:      #0E8FA3;  /* fills, icons, borders — 3.8:1 */
  --accent-text: #0A7A8C;  /* text and links — 5.0:1 */
  --accent-press:#086472;
  --attn:        #B87513;  /* match/attention fills */
  --attn-text:   #8F5A08;
  /* semantic */
  --success: #1E8A5C;  --warning: #B87513;  --danger: #C0362F;
  /* schema hues */
  --s-person: #4B5BC8; --s-company: #8A4FC0; --s-address: #C04B84;
  --s-phone:  #C85A42; --s-email:   #8F7420; --s-website: #5D8A2E;
  --s-account:#1E8A5C; --s-vehicle: #2A76B4; --s-document:#5A6878;
}

:root[data-theme="dark"], :root:not([data-theme="light"]) {
  --bg-void:    #0A0D12;  --bg-base:   #10151C;
  --bg-panel:   #0E131A;  --bg-raised: #1E2630;
  --line:       #212A34;  --line-strong: #2A333F;
  --fg:         #E8EDF4;  --fg-2: #A7B4C4;  --fg-muted: #7C8B9C;
  --accent:      #35C6D8;  --accent-text: #35C6D8;  --accent-press: #1E97A8;
  --attn:        #E3A44A;  --attn-text:   #E3A44A;
  --success: #5FC894;  --warning: #E3A44A;  --danger: #E56A66;
  --s-person: #8B9BF0; --s-company: #B98CE8; --s-address: #E88CB4;
  --s-phone:  #F09480; --s-email:   #D7AE63; --s-website: #9DC46C;
  --s-account:#5FC894; --s-vehicle: #6FB9E8; --s-document:#8A98A8;
}
```

**Accent pair.** Signal cyan `oklch(.72 .14 205)` and attention amber
`oklch(.72 .14 75)` — one lightness, one chroma, two hues, so neither
outshouts the other. Cyan is interaction only: selection, focus, primary action,
the active node. Amber is unresolved state: match suggestions, deferred
decisions. Neither is ever used decoratively.

**Schema hues** sit at `oklch(.70 .11 h)` in dark and `oklch(.50 .13 h)` in
light, and deliberately avoid 190–220° so a node is never the colour of its own
selection ring. `Document` is grey on purpose: it is evidence, not a subject of
interest.

**Contrast, measured.** fg on bg-base 15.6:1 · fg-2 8.7:1 · fg-muted 5.3:1 ·
cyan on dark 8.9:1 · light fg-muted on white 5.0:1. Light-theme cyan splits in
two: `--accent-text` (5.0:1) for anything read, `--accent` (3.8:1) for fills,
icons and borders where 3:1 is the bar. Any new colour pair is measured before
it ships, not estimated.

---

## 10.3 Typography

| Family | Role | Notes |
| --- | --- | --- |
| **Instrument Serif** | Display — case titles, empty states, the wordmark | Investigation is a documentary craft; the type should say so. At most one serif element per screen. |
| **IBM Plex Sans** | Interface — everything read and clicked | Drawn for technical products; holds at 12px in a dense table, and its Latin Extended covers Polish without a fallback swap. |
| **IBM Plex Mono** | Data — ids, hashes, IBANs, timestamps, normalised values | Anything a machine produced or that must be copied exactly. |

Scale — eight steps, and no others:

| Token | Size / line | Weight | For |
| --- | --- | --- | --- |
| `display-lg` | 34 / 1.1 | 400 serif | Case headers, empty states |
| `title` | 22 / 1.25 | 600 | Panel and page headings |
| `heading` | 16 / 1.4 | 600 | Inspector groups, cards |
| `body` | 14 / 1.55 | 400 | Prose, notes, descriptions |
| `body-sm` | 13 / 1.45 | 400 | Table cells, list rows |
| `label` | 12 / 1.35 | 500 | Form labels, chips, badges |
| `caption` | 11 / 1.4 | 400 | Timestamps, helper text |
| `overline` | 10 / 1.2 | 500 mono | Section eyebrows, +0.1em tracking, uppercase |

Rules: prose capped at `72ch` with `text-wrap: pretty` · `tabular-nums` on every
number in a column · entity captions never truncate silently (full value on hover
and in the inspector — a half-read name is how the wrong person gets identified)
· 700 exists for the primary button and nothing else · check every new face
against `ĄĆĘŁŃÓŚŹŻ ąćęłńóśźż` before adopting it.

---

## 10.4 Space, radius, elevation

Spacing steps: **2 4 6 8 12 16 22 32 48**. Layout is flex or grid with `gap` —
never margins between siblings, never whitespace text nodes.

Radii: **6** chips and badges · **8** controls and inputs · **11** small
overlays · **14** panels and cards · **20** mobile bottom sheets. Nothing rounder.

Control heights: **28** small · **34** default desktop · **40** large ·
**44 minimum on touch**, without exception.

| Level | Recipe | For |
| --- | --- | --- |
| `e0` | border only | Panels and cards at rest |
| `e1` | `inset 0 1px 0 rgb(255 255 255/.06), 0 1px 2px rgb(0 0 0/.4)` | Buttons, rows on hover |
| `e2` | `+ 0 6px 18px rgb(0 0 0/.5)` | Dropdowns, tooltips |
| `e3` | `backdrop-filter: blur(14px)` + `0 18px 44px rgb(0 0 0/.6)` | Glass over the canvas only |

In light, the inner top highlight becomes a hairline border and shadows soften to
`rgb(17 24 32/.05–.13)`.

**The four depth effects, and where each is allowed.** *Aurora* — two soft
radials under 22% alpha, behind the graph canvas and nowhere else. *Glass* —
only where something meaningful sits behind it; frosting a panel with a flat
wall behind it is decoration pretending to be depth. *Top highlight* — 1px inner
light on every raised surface, which is what gives depth without stacking
shadows. *Grain* — 3px dot noise at ~5% over large gradients, to kill banding.

---

## 10.5 Components

**Buttons.** One primary per view; the gradient plus the top highlight is what
makes it primary, not size and not a second colour. Secondary is a bordered
raised surface, ghost is text-only, danger is a bordered outline that fills on
hover. Focus is a 2px accent outline at 2px offset — visible on every surface,
never removed.

**Inputs.** 34px, 8px radius, inset surface a step below the panel. Focus takes
the accent border plus a 3px accent wash ring. Errors state what was wrong *and
what to send instead* — the same `fix` string the API returns, because it was
written for exactly this.

**Badges.** Schema badges carry a tinted wash, a matching border, a colour dot
and the schema name. Confidence pills carry border style: solid confirmed,
dashed probable, dotted hypothesis.

**Match suggestion card** — the signature component. Score at display size,
strength pill, the feature breakdown in plain language with per-feature
contributions, and the cross-case attribution ("found in *Baltic freight* — you
are in *Warsaw logistics*"). Actions: link as same-as, mark different, defer.
Merge is never offered across cases.

**Table rows.** 44px, hovering raises one elevation step. Unsourced facts carry a
danger mark — an indicator, not an error state.

---

## 10.6 The graph canvas

- Node fill is the schema hue; border style is confidence; size is degree,
  clamped to a 2.5× range so hubs read without swamping.
- The selected node gets a soft halo, a 2px accent ring, and a single breath on
  selection. It is the only element in the product allowed a glow.
- Foreign nodes — entities reached across a confirmed same-as link — are drawn at
  reduced fill with a dashed accent ring and a case badge. The boundary between
  cases stays visible; it is not dissolved.
- Same-as bridges are dashed cyan. Ordinary relations are a neutral stroke.
- New relations draw from source to target, so direction is felt rather than read
  off an arrowhead.
- Labels sit below nodes, `body-sm`, with a secondary line in mono for the
  identifying value.
- Layout settles once and then **never moves on its own**. Nodes drifting while
  being read destroys spatial memory, which is the entire reason a graph beats a
  list.

---

## 10.7 Motion

| Token | ms | For |
| --- | --- | --- |
| `instant` | 90 | Hover, focus ring, checkbox |
| `quick` | 140 | Buttons, chips, tooltips, rows |
| `settle` | 220 | Inspector, dropdowns, toasts |
| `travel` | 320 | Camera fly-to, bottom sheet, view change |

Easing: `--ease-standard: cubic-bezier(.2,.8,.3,1)` for everything on screen;
`--ease-emphasis: cubic-bezier(.4,0,.2,1)` for panels and sheets entering or
leaving.

**Never animates**: graph layout after the first settle · numbers counting up ·
anything on scroll (no reveals, no parallax) · destructive confirmations, which
appear instantly and wait.

**Reduced motion is a real mode.** Under `prefers-reduced-motion: reduce`,
durations collapse to 1ms — states still change, they simply arrive rather than
travel. Every state the product signals through motion must also be legible
without it, which is why the match badge carries a count and an icon and not
just a pulse.

---

## 10.8 Responsive and mobile

Breakpoints: **≥1280** full workspace (rail + list + canvas + inspector) ·
**900–1279** inspector becomes an overlay · **600–899** list collapses to a
drawer, canvas full width · **<600** mobile layout.

On mobile the product is for **reading and capturing**, not graph layout:

- A focus view centred on one entity with its immediate neighbours, pinch to
  zoom, tap to re-centre.
- A bottom sheet carries the selected entity — identity, key properties, actions
  — draggable to three heights.
- A five-item tab bar with a raised centre action for capture.
- Every hit target ≥44px, including icon buttons and tab items.
- **No fake chrome.** No painted status bar, no drawn keyboard. That space
  belongs to the OS, and a painted copy looks doubled up on a real device.
- Reserve the top safe area and the bottom home-indicator inset; do not draw in
  either.

---

## 10.9 Accessibility

Non-negotiable, and checked before M3 exits: every interactive element reachable
and operable by keyboard with a visible focus ring · text meets 4.5:1 and UI
components 3:1 · nothing encoded by colour alone · the graph canvas has a table
equivalent that exposes the same data · live regions announce match suggestions
and save confirmations · reduced motion honoured · touch targets ≥44px ·
`prefers-contrast: more` thickens borders and drops washes.

---

## 10.10 Implementation notes

- Tokens ship as CSS custom properties on `:root`, themed by
  `[data-theme]` with a `prefers-color-scheme` fallback, and are mapped into the
  Tailwind theme so utilities resolve to the same values. No raw hex in a
  component, ever.
- Icons are inline stroke SVG on a 16/20/24 grid at 1.6–1.8px stroke. No icon
  font, no emoji.
- Fonts are self-hosted and subset (`latin`, `latin-ext`), preloaded, with
  metric-compatible fallbacks — the whole application must run air-gapped, which
  rules out a font CDN.
- The theme toggle writes `data-theme` and persists the choice; the default
  follows the system.
- Graph node styling maps schema id → hue via one lookup shared by the canvas,
  the badges and the reports, so a colour never disagrees with itself.

---

## 10.11 Open

The product name is unresolved ([ticket 003](wayfinder/tickets/003-product-name.md)),
so the wordmark is provisional. The mark carries no letterforms and survives a
rename untouched.

Two alternate accent directions are drawn on the canvas — field green + rose
(quieter, more archival) and bone + hazard orange (near-monochrome, one loud
accent). Swapping either in is a token change and nothing else.
