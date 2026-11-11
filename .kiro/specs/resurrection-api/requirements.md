# Requirements: resurrection API

Source: PLAN.md §12.7 and §12.8, as amended by §R. Format: EARS.

## A1 Jobs
- A1.1 WHEN a client POSTs `/api/resurrections` with a known example id, a date (YYYY-MM-DD) and a policy THEN the system SHALL run the pipeline and return the job id, certificate, ZIP hash and URLs. The target time SHALL be the end of that day (23:59:59 UTC).
- A1.2 WHEN the body has an unknown field, an unknown example, or a malformed date THEN the system SHALL answer 400 with a machine-readable error code and SHALL NOT run a job. WHEN the body exceeds 16 KiB THEN the system SHALL answer 413 with code `body_too_large`.
- A1.3 WHEN the pipeline finds no usable capture THEN the system SHALL answer 422 with code `no_capture`, the verdict, and a next step. It SHALL NOT store a job.
- A1.4 Job ids SHALL be 128-bit random values. Jobs SHALL expire after one hour, and at most 500 jobs SHALL be held, oldest evicted first.
- A1.5 WHEN a client sends more than 30 POSTs a minute from one address THEN the system SHALL answer 429 with `Retry-After`.
- A1.6 (security review, Nov 11 2026) WHEN the server runs behind trusted proxies THEN the client address SHALL be the `X-Forwarded-For` entry that the nearest trusted proxy appended, counted from the right by the configured number of hops. The left-most entry SHALL NOT be used, because the client controls it. An entry that is not an IP address, or a header with too few entries, SHALL be ignored in favour of the socket address. WHEN no proxy is trusted THEN `X-Forwarded-For` SHALL be ignored.
- A1.7 (security review, Nov 11 2026) The total size of retained jobs SHALL NOT exceed a configured budget (default 64 MiB). WHEN a new job would exceed it THEN the oldest jobs SHALL be evicted first.

## A2 Downloads
- A2.1 `GET /api/resurrections/{id}` SHALL return the certificate and URLs, or 404 for an unknown or expired id.
- A2.2 `GET /api/resurrections/{id}/archive.zip` SHALL return the ZIP as an attachment, byte-identical to the one hashed in the certificate response.

## A3 Preview (security)
- A3.1 `GET /p/{id}/{path}` SHALL serve a restored file only if the path is in the job's ZIP. Any other path SHALL answer 404. `/p/{id}/` SHALL serve the restored page.
- A3.2 Every preview response SHALL carry `Content-Security-Policy` with `sandbox`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, and `X-Robots-Tag: noindex`.
- A3.3 The preview SHALL set its Content-Type from an allowlist of extensions. Anything else SHALL be `application/octet-stream`.
- A3.4 The JSON API SHALL NOT serve restored HTML. Only `/p/` does.

## A4 Examples and health
- A4.1 `GET /api/examples` SHALL list the seeded examples with title, site, page, default date, and rights basis.
- A4.2 Every response from a seeded job SHALL carry mode `SEEDED`. The API SHALL NOT report `LIVE` until a live source exists.
- A4.3 `GET /api/health` SHALL answer 200.

## L Live mode (added Nov 11, 2026)

- L1.1 WHEN a client POSTs `/api/live` with `url`, `date`, `policy`-independent options, and `rights: true` THEN the system SHALL start a run and answer 202 with a run id. WHEN `rights` is missing or false THEN it SHALL answer 400 `rights_required` and start nothing.
- L1.2 The system SHALL NOT fetch the URL the user typed. The URL SHALL only be sent, percent-encoded, as a query parameter to the archive's index. Every outgoing request SHALL go to the single archive host named in the server's configuration.
- L1.3 The system SHALL reject, with 400 `bad_url`: a scheme other than http or https, user info, an IP address, `localhost`, a host without a dot, a host longer than 253 bytes, and any URL longer than 2048 bytes.
- L1.4 The system SHALL use only the host and path of the URL. It SHALL ignore the query string and fragment, and SHALL say so in the run's notes.
- L2.1 `GET /api/live/{run}` SHALL return `status` (`running`, `done` or `failed`), progress (`stage`, `done`, `total`) while running, and, when done, the strict and the nearest-capture job views in the same shape as `POST /api/resurrections`.
- L2.2 WHEN a run fails THEN the response SHALL carry a code (`no_capture`, `archive_unavailable`, `too_large`, `too_many_assets`) and a message. A failed lookup SHALL NOT be reported as `no_capture`.
- L3.1 Archive requests SHALL be serial, at least 1.2 s apart, with a descriptive User-Agent. WHEN the archive answers 429 or 5xx THEN the system SHALL retry once, honouring `Retry-After` up to 10 s, and otherwise record the file as `UNKNOWN`.
- L3.2 A live run SHALL fetch at most 40 assets and 8 MiB in total. A single body SHALL be at most 4 MiB. Redirects SHALL be followed only within the archive host.
- L3.3 At most 2 live runs SHALL execute at once. WHEN the limit is reached THEN the system SHALL answer 503 `busy` with `Retry-After`.
- L3.4 The strict and the nearest-capture jobs of one run SHALL be built from one set of archive requests, so a run costs the archive no more than a single restore.
- L4.1 Every live result SHALL carry mode `LIVE`. Seeded results SHALL keep `SEEDED`.
- L5.1 For live runs the system SHALL ask the index for the last capture at or before the target and the first capture after it, and SHALL NOT download the full capture list of a file.

## S Findings from the reviewer agent (Nov 11, 2026)

Found by running `revenant-reviewer` read-only over the live and preview code. Each was reproduced or read at the cited line before it was accepted.

- S1 (fail closed) WHEN the preview CSP cannot be built THEN the system SHALL answer 500 and SHALL NOT serve the file. WHEN `frame_ancestors` is not a valid header value THEN the server SHALL refuse to start.
- S2 (L1.3, tightened) The system SHALL reject a host whose last label starts with an ASCII digit, so forms such as `0x7f.0x1`, `1.2.3.4`, `127.1` and `0177.0.0.1` are all refused.
- S3 (no internal detail) Error responses SHALL carry fixed messages. Debug output and transport errors SHALL go to the log, not to the client.
- S4 (accepted, documented) A single client can still fill the job store (findings 2 to 4 of the review). This is listed under Limitations in the README. A per-client job quota is future work.

## E Server evidence and the playback baseline (added Nov 12, 2026)

Source: engine requirements R6 and R7.

- E1.1 The `policy` field SHALL accept `strict`, `include-anachronisms` and `nearest`. Anything else SHALL be 400 `bad_request`.
- E1.2 A job's download file name SHALL end with its policy: `strict`, `with-anachronisms` or `nearest`.
- E2.1 (live) The live source SHALL read a capture's server date only from the `x-archive-orig-last-modified` header of a response from the archive host. It SHALL NOT read it from page content.
- E2.2 (live) WHEN the pipeline asks for a server date THEN the live source SHALL fetch the capture once, keep the body, and answer later requests for that body from memory, so the question costs no extra archive request when the file is then used.
- E2.3 (live) A missing or unparseable header SHALL give no server date, and the verdict SHALL stand on capture dates alone.
- E2.4 (live) One live run SHALL still make one set of archive requests for all its restores (L3.4).

- E3.1 `GET /p/{id}/?annotate=1` SHALL serve the annotated page (R8). Without the parameter the page SHALL be served exactly as it is in the ZIP.
- E3.2 The parameter SHALL have no effect on any file other than the page, on the ZIP, or on the certificate.
- E3.3 The annotated page SHALL carry the same security headers as any other preview.

## L6 One index query per file (added Nov 12, 2026)

Why: `sort=closest&closest=T&limit=N` returns captures ordered by distance from T in one request, where live mode made two (last at or before T, first after T). Page after page of real archive traffic is index questions, so halving them shortens every live run.

- L6.1 A live lookup SHALL ask the index once per file, ordered by closeness to the target time, for up to 6 HTTP 200 captures.
- L6.2 WHEN the answer holds captures on one side of T only THEN the source SHALL ask once more for the nearest capture on the missing side, because the nearest six can all be on one side while a capture exists on the other. It SHALL NOT ask again when the first answer holds fewer than 6 rows, since then the index has told us everything it has.
- L6.3 The follow-up SHALL be bounded the same way as before: `limit=-1` with `to=T` for the missing earlier side, `limit=1` with `from=T+1` for the missing later side.
- L6.4 The classification given by the one-query path SHALL equal the classification given by the two-query path for every sequence of captures. (Verified by test against the fake archive.)
- L6.5 Rows SHALL be de-duplicated by capture time.

- L6.6 (amends L3.2) With one index query per file a live run costs about half the archive requests, so the cap rises from 40 to 80 files. The byte caps are unchanged (8 MiB per run, 4 MiB per body). A page above the cap SHALL still be refused with `too_many_assets`, naming the count.

## T Timeline (added Nov 12, 2026)

Why: the date field is blind. A visitor cannot tell which days the archive covers, and the most common live failure is a date before the first capture. One index question can show the page's whole version history.

- T1.1 `POST /api/timeline` with `url` SHALL answer with the page's versions: for each point where the archive's digest for the page changed, the time of the first capture with that digest, and the total number of captures seen. It SHALL use the same address validation as live runs (L1.3) and SHALL NOT require the rights box, since it restores nothing.
- T1.2 The system SHALL ask the archive exactly one question, HTTP 200 captures only, one row per run of the same digest, capped at 400 versions. It SHALL answer `truncated: true` when the cap is hit.
- T1.3 (amended Nov 12, 2026) The timeline SHALL use the same archive pacing as live runs, so it can never make the archive see more traffic. It SHALL have its own, more generous rate limit (20 per 10 minutes per client), because one lookup is one archive request. It SHALL NOT take a live-run slot.
- T1.4 (amended Nov 12, 2026) The answer SHALL include the time of the first capture. It SHALL NOT claim a last-capture time: collapsing runs of the same digest returns the first capture of each run, so the final row is when the last version began, not when the archive last saw the page. The answer SHALL call that row `latest_version_from`.
- T1.5 The system SHALL NOT claim that content changed between two versions with different digests. It SHALL call them "versions the archive saw", because digests differ for reasons other than content (R4).
- T1.6 WHEN the archive cannot be asked THEN the answer SHALL be `archive_unavailable`. WHEN the archive has no 200 captures THEN the answer SHALL be an empty list with `versions: []`, not an error.

Amendment (Nov 12, 2026, after trying it on the real archive). With `collapse=digest` and a cap of 400, a page crawled constantly (fema.gov) returned its first 400 versions, ending in 2005, for a page the archive covers through 2026. The strip would have looked as if the page stopped existing. A date aid must show where the archive has coverage before it shows how the page changed.

- T1.2 (replaced) The system SHALL ask the archive exactly one question, HTTP 200 captures only, collapsed to one row per calendar month (`collapse=timestamp:6`). A month's row is its first capture in that month. This bounds the answer by the number of months the archive has covered (324 for a page since 1996), whatever the crawl rate, so the whole history is always shown. `truncated` SHALL stay in the answer but SHALL be true only when the answer would exceed 600 months.
- T1.5 (amended) The answer SHALL NOT claim anything about content. A month's row says the archive holds at least one capture in that month, and carries the digest of that first capture. Two months with different digests are "different versions the archive saw", not proof that the page changed (R4).
- T1.7 The answer SHALL include `latest_month`: the last month that holds a capture. Unlike the earlier `latest_version_from` it is true of the whole history.
