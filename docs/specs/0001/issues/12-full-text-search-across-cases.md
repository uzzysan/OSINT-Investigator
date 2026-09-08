# 12: Full-text search across Cases

**What to build:** an analyst half-remembers a name and finds it. More
importantly, they search across every Case at once and discover they have seen
that name before — which is the first hint that two investigations are connected,
months before the resolver exists to say so properly.

Search ignores diacritics, so `Kowalski` finds `Kowalskí` and the reverse.
Results say which Case each hit lives in, because that attribution is the whole
point of searching across all of them.

The search index is maintained by database triggers rather than application code,
so a write path that forgets to reindex is not a thing that can be written.

**Blocked by:** 09

**Status:** ready-for-agent

- [ ] Full-text search over Entity captions and Property values
- [ ] Diacritics are folded in both the index and the query
- [ ] Search can be scoped to one Case or run across all of them
- [ ] Results carry Case attribution
- [ ] Results can be filtered by Schema, tag and confidence
- [ ] Soft-deleted Entities and Cases never appear in results
- [ ] The index is kept current by triggers, proven by a test that writes and immediately searches
- [ ] Updating a Property updates the index; deleting one removes it
- [ ] Results are ranked, paginated with a stable cursor, and report whether more exist
- [ ] A rebuild-index operation exists for use after a normalisation rule changes
