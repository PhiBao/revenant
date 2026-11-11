# Fixture provenance: y2k.gov

## Rights basis
Y2K.gov was a US federal government information site. Content authored by US federal employees is a work of the US government and is not copyright-protected in the United States under 17 U.S.C. §105. The site's domain is no longer live (no DNS record as of 2026-11-11).

Open item before any video or public use: confirm that no page in the fixture embeds third-party-licensed media (images, fonts, scripts). The GIFs in this fixture were never archived with a successful response, so they are not included.

## Source
- Wayback Machine CDX API, queried 2026-11-11: `https://web.archive.org/cdx/search/cdx`.
- File: `captures.tsv`. One row per archived response. Columns: `label, ts, digest, mimetype, status, original`.
- The index keeps every status code. The engine drops non-200 rows (`crates/revenant-core/src/index.rs`).
- Labels use the form `y2k.gov/<path>`. `www.` and port variants are merged under one label.

## Counts (captures.tsv)
- `y2k.gov/` root: 385 rows with HTTP 200 and 29 rows with HTTP 404.
- Five GIF labels: 0 rows with HTTP 200, 40 rows with HTTP 404 in total.

## Reproducing
The query and filter are in `captures.tsv`'s source rows. Re-run the CDX query above with `url=y2k.gov/` and `url=y2k.gov/gif/<name>.gif`. Expect the counts to grow over time as the archive adds captures, so the test pins this snapshot's timestamps rather than the counts.

## briefings fixture
Captured 2026-11-11 from the CDX API: labels are the page and its 11 local references. Source rows keep all statuses. Only the 200 rows are used by the engine.
No blob bodies are stored yet, so the ZIP packaging step will need them.
