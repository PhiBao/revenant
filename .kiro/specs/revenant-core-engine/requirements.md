# Requirements: revenant-core engine (fidelity classifier)

Source: PLAN.md §R.3 and §R.4. Format: EARS (WHEN … THEN the system SHALL …).

## R1 Timestamps
- R1.1 WHEN given a 14-digit CDX timestamp (YYYYMMDDhhmmss) THEN the system SHALL return Unix seconds in UTC.
- R1.2 WHEN given a malformed or impossible date (non-digit characters, month 0 or 13, day beyond the month length, hour 24) THEN the system SHALL return no value.
- R1.3 WHEN given any string THEN the system SHALL NOT panic.

## R2 Verdicts
Each file for a target time T gets exactly one verdict, decided from its captures:
- R2.1 `PROVEN`: the latest capture at or before T and the earliest capture after T have the same digest.
- R2.2 `LIKELY`: a capture exists at or before T, and no capture exists after T.
- R2.3 `UNCERTAIN` (spec amendment, see below): a capture exists at or before T and a capture exists after T, and their digests differ. The change happened somewhere in that window, so the version at T cannot be proven.
- R2.4 `ANACHRONISM`: no capture exists at or before T, and the earliest capture after T is the only candidate. The file did not exist as captured at T.
- R2.5 `MISSING`: the file has no captures.

## R3 Selection
- R3.1 WHEN captures are supplied in any order THEN the system SHALL return the same verdict and the same chosen capture (order invariance).
- R3.2 The system SHALL NOT return `ANACHRONISM` when a capture at or before T exists.
- R3.3 WHEN a verdict is `PROVEN` THEN the chosen capture's digest SHALL equal the digest of a capture after T.
- R3.4 The system SHALL report `delta_secs` as the chosen capture's time minus T, so negative means before T.

## Spec amendment (logged)
PLAN.md §R.3 defines four verdicts. The y2k.gov spike (docs/spike/m1-y2k.md) found a root page with captures on both sides of T and different digests. None of the four verdicts fits it, so R2.3 adds `UNCERTAIN`. Judges can read this change in KIRO_USAGE.md, which records that the spec was amended before the code.

Second amendment (Nov 11, 2026). Five y2k GIFs had captures only with HTTP 404. The first spike counted them as captures and called them anachronisms. That was a bug in the spike, not a property of the archive. R0 below now governs.

- R0.1 WHEN an index row has a status other than 200 THEN the system SHALL NOT treat it as a capture. It SHALL count the row as dropped.
- R0.2 A file with no 200 captures SHALL classify as `MISSING`, and the ledger SHALL say "archived only as 404" when rows were dropped.

## Out of scope for this spec
Network access, de-archiving, packaging, and the HTTP API. Those belong to later specs.
