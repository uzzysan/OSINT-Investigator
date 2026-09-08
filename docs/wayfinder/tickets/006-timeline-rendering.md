---
id: 006
title: Timeline rendering approach
type: prototype
status: open
blocked_by: [005]
assignee:
---

## Question

How is the timeline drawn, and does the chosen approach handle partial dates?

The timeline is not decoration — F-TIME-2 requires that a "2019" event renders as
a year-wide band rather than a point on 1 January, because collapsing precision
is how investigators end up asserting a sequence that the evidence does not
support. Most off-the-shelf timeline components assume exact timestamps.

**Options**

- **`vis-timeline`** — mature, handles ranges, grouping, zoom out of the box.
  Heavy, its own rendering model, and styling it to match the rest of the UI is
  known to be tedious.
- **Custom D3 scale + SVG/canvas** — full control over precision bands, grouping
  by entity, and linkage to the graph canvas. More work, but the timeline needs
  to interoperate with the canvas selection and the scrub control, which is
  exactly where a general-purpose component fights back.
- **A charting library's Gantt mode** — likely to fight the partial-date
  requirement hardest.

**Prototype**: render 500 events across a decade, mixed precision (year, month,
day, instant), grouped by entity, with a range-scrub control that emits the
selected window. Try `vis-timeline` and a D3 version; compare effort, bundle
size, and how each handles a year-precision band next to a day-precision point.

Blocked by [005](005-canvas-performance.md) only because both are frontend
prototypes and the canvas answer may constrain the shared rendering approach —
if 005 lands on canvas-based level-of-detail, the timeline should probably match.

## Answer

_(unresolved)_
