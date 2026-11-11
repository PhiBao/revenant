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

Third amendment (Nov 11, 2026). Two y2k captures with different CDX digests served byte-identical bodies, so a digest difference alone can raise a false `UNCERTAIN`.

- R4.1 WHEN both captures on either side of T carry a content hash THEN the system SHALL compare content hashes and ignore the CDX digests.
- R4.2 WHEN either side has no content hash THEN the system SHALL compare CDX digests.
- R4.3 WHEN content hashes are equal and digests differ THEN the verdict SHALL be `PROVEN`.

## Out of scope for this spec
Network access, de-archiving, packaging, and the HTTP API. Those belong to later specs.

## R5 Resurrection pipeline (added Nov 11, 2026)
- R5.1 WHEN given a source, a site, a page path and a target time THEN the system SHALL return a ZIP, a certificate, and the SHA-256 of the ZIP.
- R5.2 The system SHALL discover the page's embedded assets from the page itself, and SHALL NOT take them from a caller-supplied list.
- R5.3 WHEN an asset's verdict is `MISSING` THEN the system SHALL NOT include it in the ZIP, and SHALL list it in the certificate.
- R5.4 WHEN the policy excludes anachronisms THEN an `ANACHRONISM` asset SHALL stay out of the ZIP and in the certificate.
- R5.5 WHEN the page has no usable capture THEN the system SHALL return `NoCapture` and no ZIP.
- R5.6 The same source, request and target SHALL give the same ZIP bytes. The certificate SHALL NOT hold wall-clock time.
- R5.7 The ZIP SHALL contain `REVENANT-CERTIFICATE.json`. No URL attribute in the restored page SHALL point at `web.archive.org`. The certificate SHALL report how many text mentions of the archive host remain, because prose can mention it legitimately.
- R5.10 WHEN the archive cannot be asked about a file (error, block, timeout) THEN its verdict SHALL be `UNKNOWN`, it SHALL stay out of the ZIP, and it SHALL NOT count as missing in Completeness.
- R5.8 The system SHALL refuse a job above 500 assets or 64 MiB, and SHALL say which cap it hit.
- R5.9 Navigation links are not restored in single-page scope. The certificate SHALL list them as `links_not_restored`.

- R5.11 The caller MAY lower the asset and byte caps for one request (`Limits`). A request SHALL NOT be able to raise them above 500 assets and 64 MiB.
