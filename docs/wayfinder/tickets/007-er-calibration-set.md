---
id: 007
title: Entity-resolution calibration set
type: research
status: open
blocked_by: [004]
assignee:
---

## Question

What data do we calibrate the entity resolver against, and where does it come
from?

[`docs/05-entity-resolution.md`](../../05-entity-resolution.md#59-evaluation)
commits to a labelled fixture set of roughly 300 pairs — true matches, hard
negatives and near-misses — and to reporting precision, recall and F1 per
threshold. The default weights in that document are reasoned, not measured. Until
they are measured, "0.62" is a guess with a decimal point.

**Investigate**

- **Public labelled sources.** OpenSanctions publishes matching datasets and its
  `nomenklatura` scoring is open — what is reusable, and under what licence?
  Are there public record-linkage benchmarks (Febrl, the North Carolina voter
  set, DBLP-Scholar) whose shape is close enough to be useful?
- **Synthetic generation.** Take a seed set of entities and apply realistic
  corruptions: transliteration (`Kowalski`/`Kowalsky`), diacritic loss, name
  order swaps, nickname substitution, date-precision changes, typos, formatting
  differences in phone numbers and identifiers. This is likely the bulk of the
  set and needs a generator, which is itself a small piece of work.
- **Hard negatives specifically.** A resolver that only sees easy negatives will
  be badly calibrated in production. Same name different person, family members
  sharing an address and a surname, companies with near-identical names in the
  same registry — these are the pairs that matter.
- **Polish and Central European data in particular**, since that is the likely
  first use: PESEL/NIP/REGON/KRS formats, declension in names, and
  `Sp. z o.o.` / `S.A.` suffix noise.

Blocked by [004](004-schema-catalogue.md): the fixture format depends on the
final property names.

**Output**: a decision on the source mix, a licence note for anything external,
and a `crates/vesica-resolve/fixtures/` format specification.

## Answer

_(unresolved)_
