# Security rules (always on)

- SSRF: the engine fetches only allowlisted archive hosts (web.archive.org, arquivo.pt when enabled). Never fetch the user's URL directly. Seeded mode fetches nothing.
- Previews: serve from a separate origin with `Content-Security-Policy: sandbox`, `X-Content-Type-Options: nosniff`, and scripts stripped by default. Never serve restored HTML from the API origin.
- ZIP: sanitize entry paths (reject `..`, absolute paths, and drive prefixes). Enforce file-count and byte caps.
- Input: accept only http and https URLs. Cap pages, assets, and bytes per job. Rate-limit by IP. Expire jobs.
- Rights: the UI requires an ownership or rights assertion before a live run. Previews are unlisted and expire. Do not log full user URLs with query secrets.
- Secrets: never commit tokens. Use environment variables. Scan the diff before each commit.
- Archive politeness: serial lookups with at least 1.2 s between requests, Retry-After backoff, a descriptive User-Agent, and 403/429/timeouts classified as UNKNOWN or BLOCKED, never MISSING.
