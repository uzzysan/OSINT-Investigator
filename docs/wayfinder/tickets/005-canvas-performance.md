---
id: 005
title: Canvas performance at target scale
type: prototype
status: open
blocked_by: []
assignee:
---

## Question

Does Sigma.js v3 + graphology actually hold up at 20,000 nodes and 60,000 edges
on the target hardware, and what happens past that?

[`docs/03-architecture.md`](../../03-architecture.md#35-frontend) picks Sigma.js
on the reasoning that it is WebGL and Cytoscape.js is not. That reasoning is
sound but untested for *this* workload, and the canvas is the feature the whole
product is judged on. NF-2 and F-GRAPH-1 both depend on the answer.

**Build a throwaway prototype** — a static page, no backend — that:

1. Generates a synthetic graph with a realistic degree distribution (a few hubs,
   a long tail), at 1k / 5k / 20k / 50k nodes.
2. Renders it with Sigma.js v3, node icons and colours by type, edge arrows.
3. Runs ForceAtlas2 in a Web Worker and measures time to a stable layout.
4. Measures pan/zoom frame rate, hover hit-testing latency, and selection of a
   500-node box.
5. Repeats the 20k case with Cytoscape.js as a control, to confirm the choice
   rather than assume it.

**Report**: fps and layout time per size, memory, the size at which it stops
being usable, and what the fallback should be past that ceiling — sampling, a
level-of-detail scheme, server-side layout, or forcing a filter. That fallback
is currently sitting in the map's *Not yet specified* section waiting on this.

Keep the prototype on a `prototype/canvas-perf` branch; link the numbers here.

## Answer

_(unresolved)_
