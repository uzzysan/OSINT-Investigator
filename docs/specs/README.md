# Specs

Implementation specs, written from the design documents and scoped so that one
agent can execute one spec. Each carries triage labels in its front matter; a
spec labelled `ready-for-agent` needs no further triage before work starts.

| # | Spec | Milestones | Labels | Status |
| --- | --- | --- | --- | --- |
| [0001](0001-foundation-and-domain-core.md) | Foundation and domain core | M0, M1 | `ready-for-agent` | ready |

## Format

Problem statement and solution from the user's perspective · an extensive
numbered list of user stories · implementation decisions (modules, interfaces,
contracts — never file paths or code) · testing decisions including the seams
· out of scope · further notes.

A spec names the open [wayfinder tickets](../wayfinder/map.md) it depends on
rather than assuming them closed, so an implementing agent knows which parts of
it are provisional.
