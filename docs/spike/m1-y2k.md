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

Verdict for the root at T = 1999-12-31: **UNCERTAIN**. The page changed somewhere between two captures, so the version at T cannot be proven.

## Assets referenced by the 1999-10-13 homepage
| Asset | Earliest capture | Captures | Verdict at T = 1999-12-31 |
|---|---|---|---|
| gif/award.gif | 2000-10-13 | 1 | ANACHRONISM (+9.5 months) |
| gif/award2.gif | 2000-10-13 | 1 | ANACHRONISM |
| gif/linkwht.gif | 2000-10-13 | 1 | ANACHRONISM |
| gif/splasha.gif | 2000-08-10 | 3 | ANACHRONISM (+7.3 months) |
| gif/splashb.gif | 2013-07-16 | 2 | ANACHRONISM (+13.5 years) |

Every image on the 1999 homepage was first captured after the date the page was written. A naive "nearest-in-time" restore would therefore show a 1999 page with 2000–2013 images. That is the defect the fidelity model is meant to expose.

External links on the page (`currents.net`, `ky3.com`, `latimes.com`) were not fetched. The restore keeps them as external links and does not archive them.

## Decisions taken from this spike
1. Use y2k.gov as the seeded fixture for the demo.
2. Add a fifth verdict, `UNCERTAIN`, for the case where the latest capture at or before T and the earliest capture after T have different digests. The four verdicts in PLAN.md §R.3 cannot represent this case. This is a spec amendment, to be logged in KIRO_USAGE.md.
3. The M1 go/no-go is GO. The demo has a real anachronism and a real uncertain root, and both come from the archive, not from a constructed example.
