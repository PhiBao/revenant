# M1 spike: y2k.gov (fixture candidate)

Date of probe: 2026-11-11. Source: Wayback Machine CDX API and `id_` raw replay. Read-only. Requests were serial with at least 2 s between them.

## Site status
- `y2k.gov` and `www.y2k.gov`: no DNS record. The live site is gone, so the archive is the only source.
- Rights basis: US federal government work, public domain under 17 U.S.C. §105. Still to verify: that no page embeds third-party-licensed media.

## Coverage
- 400 unique HTML URLs with 200 status (CDX, collapsed by URL key).
- By capture year: 1998 31, 1999 116, 2000 250, 2001 2, 2016 1.

## Root page (`/`) versions
Fourteen distinct content digests. Captures closest to a 1999-12-31 target:
- 1999-10-13 (digest H7C62L…): the latest capture at or before the target. Gap of 79 days.
- 2000-03-02 (digest TUVGLI…): the earliest capture after the target. Its digest differs from the previous one.

Verdict for the root at T = 1999-12-31: **UNCERTAIN**. The page changed somewhere between two captures, so the version at T cannot be proven. Checked against `fixtures/snapshots/y2k/captures.tsv` by `crates/revenant-core/tests/fixture_y2k.rs`.

Go/no-go: GO, with the GIF finding above. The demo's real evidence is the uncertain root page and the missing images. Both come straight from the archive index.

## Assets referenced by the 1999-10-13 homepage
| Asset | Captures (all statuses) | HTTP 200 captures | Verdict at T = 1999-12-31 |
|---|---|---|---|
| gif/award.gif | 1 (2000-10-13) | 0 | MISSING (archived only as 404) |
| gif/award2.gif | 1 (2000-10-13) | 0 | MISSING |
| gif/linkwht.gif | 1 (2000-10-13) | 0 | MISSING |
| gif/splasha.gif | 6 (2000-08 to 2021) | 0 | MISSING |
| gif/splashb.gif | 2 (2013-07, 2013-08) | 0 | MISSING |

**Correction (Nov 11, 2026).** The first version of this spike listed these images as ANACHRONISM. That was wrong. Every capture of these five GIFs is an HTTP 404 page stored under the image URL, so the images were never archived. Counting the 404 rows as captures produced the false result. The engine now keeps only HTTP 200 captures (see `crates/revenant-core/src/index.rs`), and these images classify as MISSING.

This matters for the product. The archive does hold a real, provable anachronism only for the root page, not for the images. A restore must not present a 404 record as a copy of the image.

External links on the page (`currents.net`, `ky3.com`, `latimes.com`) were not fetched. The restore keeps them as external links and does not archive them.

## Decisions taken from this spike
1. Use y2k.gov as the seeded fixture for the demo.
2. Add a fifth verdict, `UNCERTAIN`, for the case where the latest capture at or before T and the earliest capture after T have different digests. The four verdicts in PLAN.md §R.3 cannot represent this case. This is a spec amendment, to be logged in KIRO_USAGE.md.
3. The M1 go/no-go is GO. The demo's evidence is the uncertain root and the missing images, both from the archive index. The earlier claim of a real anachronism on the GIFs is withdrawn.
