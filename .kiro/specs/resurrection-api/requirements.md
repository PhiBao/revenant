# Requirements: resurrection API

Source: PLAN.md §12.7 and §12.8, as amended by §R. Format: EARS.

## A1 Jobs
- A1.1 WHEN a client POSTs `/api/resurrections` with a known example id, a date (YYYY-MM-DD) and a policy THEN the system SHALL run the pipeline and return the job id, certificate, ZIP hash and URLs. The target time SHALL be the end of that day (23:59:59 UTC).
- A1.2 WHEN the body has an unknown field, an unknown example, or a malformed date THEN the system SHALL answer 400 with a machine-readable error code and SHALL NOT run a job. WHEN the body exceeds 16 KiB THEN the system SHALL answer 413 with code `body_too_large`.
- A1.3 WHEN the pipeline finds no usable capture THEN the system SHALL answer 422 with code `no_capture`, the verdict, and a next step. It SHALL NOT store a job.
- A1.4 Job ids SHALL be 128-bit random values. Jobs SHALL expire after one hour, and at most 500 jobs SHALL be held, oldest evicted first.
- A1.5 WHEN a client sends more than 30 POSTs a minute from one address THEN the system SHALL answer 429 with `Retry-After`.

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
