# 5. Entity Resolution

The feature that makes this more than a drawing tool: when you add a phone
number to a person in the "Warsaw logistics" case, and that same number sits on
a company director in a case you closed eight months ago, the system should tell
you — with a reason you can check.

## 5.1 Rules of engagement

1. **Nothing merges automatically.** Ever. A wrong automatic merge corrupts an
   investigation silently and is not detectable later.
2. **Every suggestion is explainable.** The UI shows which features fired and
   what each contributed. A score with no explanation is not actionable.
3. **A rejection is permanent.** "These are different people" is expensive
   knowledge; the system remembers it and never asks again.
4. **Cross-case links, not cross-case merges.** Two cases stay two cases. A
   confirmed match creates a bridge you can traverse, not a fusion.
5. **Recomputable.** Weights, thresholds and normalisation rules will change.
   A full rescan must be a single command with no data loss.

## 5.2 Pipeline

```
entity written
     │
     ▼
[1] normalise properties          (osint-core, same code as the write path)
     │
     ▼
[2] compute blocking keys         → block_key table
     │
     ▼
[3] shortlist candidates          (index lookup on shared blocking keys)
     │                              cap: 500 candidates per entity
     ▼
[4] score each pair               (feature vector → log-odds → [0,1])
     │
     ▼
[5] filter                        (drop below threshold; drop 'different'
     │                             tombstones; drop already-merged/linked)
     ▼
[6] write match_candidate rows + publish SSE event
```

Steps 2–6 run in a background worker off the `match_job` queue, so a write
returns immediately. Target latency from write to visible suggestion: under 5
seconds at 100k entities.

## 5.3 Blocking

Scoring every pair is O(n²) and unnecessary. Blocking generates cheap keys and
only pairs entities that share one.

| Key kind | Derived from | Example |
| --- | --- | --- |
| `ident` | any `identifier`, `iban`, `wallet`, `checksum` property, normalised | `ident:NIP5252248481` |
| `phone` | E.164 phone | `phone:+48601234567` |
| `email` | normalised email, plus the local part alone | `email:jkowalski@ex.com` |
| `domain` | registrable domain from a url/domain/email | `domain:example.com` |
| `name_sorted` | name tokens folded, deduplicated, sorted | `ns:jan kowalski` |
| `name_phonetic` | Double Metaphone of each token, sorted | `np:JN KLSK` |
| `name_trigram` | 3 most selective trigrams of the folded name | `ng:kow` |
| `dob` | birth date at year precision | `dob:1978` |
| `addr` | postcode + house number | `ad:00549-12` |

A candidate must share **at least one** key. High-selectivity keys (`ident`,
`phone`, `email`, `wallet`) are enough on their own; low-selectivity ones
(`dob`, `name_trigram`) only shortlist when combined with another. Keys shared
by more than a configurable number of entities (default 200) are treated as
noise and skipped — this is what stops "Warszawa" or `dob:1980` from dragging in
half the database.

Schema compatibility is a hard gate: a `Person` is never compared to a
`Vehicle`. Comparison is allowed within a schema, and between a schema and its
ancestors (`Company` vs `LegalEntity`, `Company` vs `Organization`).

## 5.4 Scoring

For each candidate pair, a feature vector is computed. Each feature yields a
value in `[-1, 1]`; features combine as a weighted sum passed through a logistic
function, which is a practical stand-in for a Fellegi–Sunter log-odds model
without needing labelled training data up front.

| Feature | Weight | Notes |
| --- | --- | --- |
| `shared_strong_identifier` | 6.0 | Matching IBAN, VIN, tax number, passport, wallet, file hash. Alone sufficient. |
| `shared_phone` | 4.0 | Post-E.164. |
| `shared_email` | 4.0 | Exact after normalisation. |
| `name_similarity` | 3.0 | max(Jaro-Winkler, token-set ratio) over all name/alias pairs. Non-linear: below 0.75 contributes negatively. |
| `name_phonetic_match` | 1.0 | Handles transliteration (`Kowalski` / `Kowalsky`). |
| `dob_match` | 2.5 | Exact date. Year-only match scores 0.8 of it. |
| `dob_conflict` | −5.0 | Two different full dates of birth. Near-veto. |
| `shared_address` | 1.5 | Normalised address key match. |
| `shared_domain` | 1.0 | Email or website on the same registrable domain. |
| `shared_neighbour` | 2.0 | The two entities are connected to the same third entity. Strong in practice, cheap to compute from `relation`. |
| `country_conflict` | −1.0 | Different nationalities with no overlap. |
| `schema_distance` | −0.5/step | Comparing across an inheritance step. |
| `gender_conflict` | −2.0 | Where both are stated and differ. |

Weights live in configuration, not in code, and each is documented with its
rationale. Defaults were chosen from the behaviour of comparable systems
(OpenSanctions' `nomenklatura` scoring, Aleph's cross-referencing) and will be
calibrated against real cases during Milestone 5.

**Thresholds** (configurable):

- `OSINT_ER_SUGGEST_THRESHOLD` (default `0.62`) — surfaces in the review queue.
- `OSINT_ER_STRONG_THRESHOLD` (default `0.85`) — marked "strong", sorted first,
  badge on the entity.

## 5.5 Explanation

Every candidate stores its feature breakdown as JSON, rendered as prose:

> **Score 0.91 — strong**
> - Same phone number `+48 601 234 567` (+4.0)
> - Name similarity 0.94: *Jan Kowalski* / *Jan Kowalsky* (+2.8)
> - Both connected to **ACME Sp. z o.o.** (+2.0)
> - Dates of birth differ in precision only: *1978* / *1978-04-12* (+2.0)
>
> Found in case **Baltic freight** (you are in **Warsaw logistics**).

## 5.6 Resolution actions

| Action | Same case | Cross case |
| --- | --- | --- |
| **Merge** | Available. Properties union; conflicting single-valued properties are kept as alternatives with both sources; relations re-pointed; the losing entity keeps its id with `merged_into` set, so old links and agent references still resolve. Full undo. | Not offered. |
| **Link as same-as** | Available but discouraged (merge is usually right). | The primary action. Creates `entity_link`. |
| **Mark as different** | Writes a permanent tombstone. | Same. |
| **Defer** | Hides for 30 days or until either entity changes. | Same. |

Merge is a transactional operation recorded under one `batch_id`; undo replays
the audit log backwards.

## 5.7 Cross-case traversal

A confirmed `entity_link` makes the graph traversable across case boundaries.
The canvas does **not** dissolve the boundary: linked entities from another case
appear as "foreign" nodes with a distinct outline, a case badge, and a "open in
that case" action. Traversal queries take an explicit `follow_links: bool` so
neither the UI nor an agent crosses cases by accident.

## 5.8 Rust implementation notes

| Need | Crate |
| --- | --- |
| Jaro-Winkler, Levenshtein, token ratios | `strsim` |
| Double Metaphone, Soundex | `rphonetic` |
| Unicode folding / transliteration | `unicode-normalization`, `deunicode` |
| Phone parsing to E.164 | `phonenumber` |
| IBAN / checksum validation | `iban_validate`, hand-rolled for NIP/PESEL/VIN |
| Deterministic ids | `ulid` |

The scorer is a pure function `score(&EntityView, &EntityView, &Weights) ->
(f32, Features)` in `osint-resolve`, unit-tested against a fixture set of pairs
with expected verdicts. That fixture set is the regression suite for weight
changes.

## 5.9 Evaluation

Milestone 5 builds a labelled fixture set of ~300 pairs (true matches, hard
negatives, near-misses) drawn from synthetic and public data. Reported metrics:
precision, recall and F1 at each threshold, plus the count of pairs surfaced per
1,000 entities — because a resolver that is right but noisy will simply be
ignored by its user.

**Explicit non-goal**: perfect recall. Missing a match costs an opportunity;
a flood of bad suggestions costs the feature's credibility. The default
thresholds are tuned for precision.
