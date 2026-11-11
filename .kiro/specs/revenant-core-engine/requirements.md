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

## R6 Server evidence (added Nov 12, 2026)

Why: a fresh review read the origin server's own `Last-Modified` header on the demo files (`x-archive-orig-last-modified`, which the archive preserves). Six images the archive first saved on 2000-03-04 had last changed on 1999-12-27, so they existed on 2000-03-02. Capture dates alone had called all six anachronisms and reported Fidelity 0%. The page itself had been replaced at 01:06 on 2000-03-02, so the "latest capture before the day" was already stale on that day. The v1 model could not see either fact.

- R6.1 A capture MAY carry `origin_modified`, the origin server's Last-Modified time as Unix seconds.
- R6.2 WHEN no capture at or before T has the same content as the earliest capture after T (call it A), and A.origin_modified is at or before T, THEN the content at T was A's content, and the verdict SHALL be `ATTESTED` with A chosen.
- R6.3 The server evidence SHALL be ignored, and the verdict SHALL fall back to the capture-date rules with a note, when (a) A.origin_modified is later than A's own capture time, or (b) a capture B at or before T exists, A.origin_modified is at or before B's capture time, and B's content differs from A's. In case (b) the header claims no change while the archive saw one.
- R6.4 WHEN A.origin_modified is later than T and no capture at or before T exists THEN the verdict SHALL stay `ANACHRONISM` and carry the note that the server shows this version was made after the date.
- R6.5 `ATTESTED` SHALL rank after `PROVEN` and before `LIKELY`, and SHALL count as trusted in Fidelity.
- R6.6 Server evidence SHALL NOT change a `PROVEN`, `LIKELY` or `MISSING` verdict. It MAY only turn `ANACHRONISM` or `UNCERTAIN` into `ATTESTED`.
- R6.7 Every selection SHALL carry a `basis`: the reason for the verdict in a fixed vocabulary, so a ledger can say why.
- R6.8 The pipeline SHALL ask the source for `origin_modified` only when the verdict would be `ANACHRONISM` or `UNCERTAIN` and the later capture has none, and SHALL then classify again.
- R6.9 The certificate SHALL report, per file, the basis and the origin time; and SHALL report the vitals computed from capture dates alone, so the effect of server evidence is visible.
- R6.10 HTTP date parsing SHALL accept the IMF-fixdate form only. Anything else SHALL give no value and SHALL NOT panic.
- R6.11 (resolves an earlier open question) Completeness counts a file as recovered when its verdict is anything but `MISSING` or `UNKNOWN`. `ATTESTED` and `ANACHRONISM` therefore both count as recovered. Fidelity is what separates them.

## R7 Playback baseline (added Nov 12, 2026)

Why: the comparison view needs a fair stand-in for what archive playback does. Playback picks, for each file, the capture closest in time, with no check that the files agree.

- R7.1 A request MAY ask for the `nearest` restore: for every file, choose the capture whose time is closest to T, ties going to the earlier one, and include it unless the file is `MISSING` or `UNKNOWN`.
- R7.2 The nearest restore SHALL still report Revenant's verdict for each file, so its ledger shows which of its files Revenant cannot vouch for.
- R7.3 (amended Nov 12, 2026: the certificate records the policy, so whole ZIPs cannot be identical) WHEN the nearest restore and the strict restore contain the same files THEN every file except the certificate SHALL have the same bytes and the same SHA-256, so a client can tell from the two certificates that the methods agree.

## R8 Annotated view (added Nov 12, 2026)

Why: the verdicts live in a table below the page. A visitor looking at the restored page cannot tell which image is attested, which is missing, or why a gap is there, and a missing image shows as a broken-image icon that looks like a bug.

- R8.1 The system SHALL produce an annotated copy of a restored page from the page's bytes and the certificate, without changing the ZIP or the page inside it.
- R8.2 Every `<img>` whose file is in the ZIP SHALL be wrapped in a marker carrying its verdict and a short tag (for example "attested 1999-12-27"), plus the full sentence as a tooltip. (Amended Nov 12, 2026: tags were first drawn over the image and hid the labels on navigation buttons.)
- R8.10 A tag SHALL NOT cover any part of the image it describes. It SHALL sit in the page's flow beside or below the image, and SHALL be no wider than the image.
- R8.3 Every `<img>` whose file is a site file that is not in the ZIP SHALL be replaced by a labelled placeholder of the same declared width and height, saying why ("not archived", "first saved 2000-03-04", "the archive did not answer").
- R8.4 Images that are not site files (external hosts) SHALL be left alone.
- R8.5 The annotated copy SHALL carry one `<style>` block and one bar at the top of the body naming the page's own verdict and the chosen copy's date.
- R8.6 All text inserted SHALL be escaped, and SHALL come from the certificate's fixed vocabulary and dates. No text from the archived page SHALL be copied into inserted markup.
- R8.7 Bytes outside the markup SHALL be left exactly as they were, as in R5's rewriter.
- R8.8 WHEN there is nothing to annotate THEN the input SHALL be returned unchanged.
- R8.9 The annotator SHALL NOT panic on arbitrary bytes.
