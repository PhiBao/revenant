# How Kiro was used

This follows the eight questions in the Kiroween 2026 rules. Each answer says what was done, points to files, and says what was not done. The dated evidence log at the end has the details.

## 1. Kiro IDE, CLI, Web or Crew: which, and how they differed

**Kiro CLI 3.2.0 only.** The whole project was built in one long CLI session. I did not use the IDE, Kiro Web or Kiro Crew, so I have no comparison between them to offer.

What the CLI made possible here: running the real product from the agent's own shell (the server, the browser tests, the container), so claims could be checked the moment they were made. Two examples are in the log: the rate-limit bypass was reproduced with a `curl` loop before the fix and again after, and the demo video's terminal scenes show real command output captured from the repository.

## 2. Custom agents

Two, in `.kiro/agents/`. Both pass `kiro-cli agent validate --path` and appear as `Workspace` agents in `kiro-cli agent list`.

- **`revenant-reviewer`** (read-only: `read`, `grep`, `glob`, `thinking` are auto-approved; no write tool is trusted). I ran it for real: `kiro-cli chat --agent revenant-reviewer --no-interactive --trust-tools=fs_read,grep,glob,read "<review brief>"`. It read the live-mode and preview code and returned six findings with file, line, reproduction and fix. Every finding was checked before acting:
  - One was reproduced on a running server: `http://0x7f.0x1/` was accepted by the live endpoint, which contradicted the README's claim that IP addresses are rejected. Fixed, with tests, and re-checked on a server afterwards (six spellings of an IP address now return `bad_url`).
  - Two more were fixed: the preview's sandboxing header could silently disappear on a bad config value (now the server refuses to start, and serving fails closed), and internal error text reached clients (now fixed messages, details in the log).
  - Three were real design issues I did not fix before submitting: no per-client quotas for job storage or live-run slots, and IPv6 clients can use many addresses against a per-address limit. They are in the README under Limitations and in the spec as S4.
- **`revenant-rust`** (engine work, with the steering and both specs pinned as resources). I wrote it but **did not run it**. The code in this repository was written in the main session, not through this agent, so I cannot report an effect on the build.

Before the agent existed, the same kind of read-through by me found one other bug (the rate limit trusted a header the client writes). That one is in the log too.

## 3. Agent hooks

Five, in `.kiro/hooks/`: engine tests on save for `crates/revenant-core`; fixture and pipeline tests when anything under `fixtures/` changes; quick checks (`scripts/verify.sh --quick`) when the agent stops; a spec-first reminder on every prompt; and a PreToolUse guard (`.kiro/hooks/scripts/block-secrets.sh`) that blocks `git commit` and `git push` when the staged diff contains a credential.

The honest state: **the hooks were registered late in the session and take effect from the next session start, so I did not see any of them fire during this build.** What I did verify is the guard script itself, by hand, against three inputs: a clean commit passes, an unrelated command passes, and a staged fake `cfat_` token exits 2 with the offending line (the output is in the demo video). It exists because a real Devpost token was pasted into this project's chat early on. I cannot claim the hooks improved the process; I can claim they are tested and ready.

## 4. Spec-driven development

Three specs in `.kiro/specs/`: `revenant-core-engine` (R groups), `resurrection-api` (A, L and S groups) and `web-ritual` (W). Requirements are in EARS form. Every test name or comment maps to a requirement id (for example R5.10 for "a failed lookup is UNKNOWN, never MISSING").

How it differed from vibe coding, from what I saw: the spec was where real data was allowed to disagree with the plan. Each change was written into the requirements first, then the code:

- R2.3 `UNCERTAIN`: the first y2k.gov probe found a page that changed between two captures. The plan's four verdicts could not express it.
- R0: five images I had called anachronisms were archived only as HTTP 404 pages. The spike was wrong, the doc was corrected, and only 200 captures count now.
- R4: two archive digests differed while the bytes were identical, so content hashes are compared first.
- A1.6 and A1.7: the rate-limit bypass and the memory budget.
- S1 to S4: the reviewer's findings.

I did not run a controlled comparison against vibe coding, so I will not claim a speed-up. The claim I can support is narrower: the requirements file is a readable record of every place the design was wrong.

## 5. Steering

Five always-on files in `.kiro/steering/`: `product.md`, `tech.md`, `structure.md`, `security.md`, `testing.md`. They were injected into the session on every turn. The ones that shaped behavior most:

- `product.md`: "never present a seeded result as live", "a failed lookup is UNKNOWN, never MISSING", "mark an anachronism, never hide it". These became requirements and then tests.
- `security.md`: archive politeness (serial requests, backoff, 403 and 429 are not "missing"), SSRF rules, and, after the review, how to read the client address behind a proxy.
- `testing.md`: the list of required property tests.

## 6. MCP

- **Devpost MCP** (`https://devpost.com/mcp`), configured at user level in `~/.kiro/settings/mcp.json` so no credential is in this repository (it uses OAuth). Used read-only: the hackathon's rules, prizes and judging criteria; the exact submission form fields; the account's registration status. That is how the Devpost text in `docs/submission/` was matched to the real form. A pasted API token turned out not to work for tool calls, which is what sent me to the OAuth flow.
- **A Revenant MCP server was not built.** It is in the roadmap.

## 7. Property-based testing

Eleven property tests (`crates/revenant-core/tests/properties.rs`, `package_props.rs`, `rewrite_props.rs`) cover: classification does not depend on input order; `ANACHRONISM` is never returned when an earlier capture exists; a `PROVEN` verdict's digest matches a later capture; delta equals capture minus target; the timestamp parser never panics; ZIP path sanitization accepts only safe paths and rejects traversal wherever it appears; ZIP bytes do not depend on entry order; the HTML rewriter never panics on arbitrary bytes; no archive host survives in a URL attribute however the archive URL is written; scripts and inline handlers never survive.

The honest result: **property tests did not catch a bug in this repository that unit tests had missed.** They pinned invariants that I had reasoned out first. One property assertion I wrote was wrong (it forbade the string `/web/` anywhere, which a legitimate path could contain) and I removed it after noticing that before it failed.

## 8. Kiro powers

**None used.** No powers were installed in this environment. Third-party tooling that did matter: Playwright (driving a real browser to review the UI and to record the demo video), ffmpeg, Docker, and the GitHub CLI.

## Evidence log (dated)

- **Nov 11, 2026, spec amendment before code (spec-driven development).** The y2k.gov spike found a root page with captures on both sides of the target and different digests. PLAN.md's four verdicts could not represent it. The spec (`.kiro/specs/revenant-core-engine/requirements.md`, R2.3) added `UNCERTAIN` before the classifier was written. Evidence: `docs/spike/m1-y2k.md`, `crates/revenant-core/src/fidelity.rs`.
- **Nov 11, 2026, property tests (PBT).** Five property tests in `crates/revenant-core/tests/properties.rs` cover order invariance, no false anachronism, proven digest matching, delta correctness, and a parser that never panics. All 15 engine tests pass, and clippy (`-D warnings`) and fmt are clean.
- **Nov 11, 2026, a spec change caught by real data.** The fixture showed five y2k GIFs with captures that were all HTTP 404. The first spike counted them as captures and called them anachronisms. The spec gained R0 (only 200 captures count), the spike doc was corrected, and the index parser now drops non-200 rows (`crates/revenant-core/src/index.rs`, fixture test `tests/fixture_y2k.rs`). This is the clearest evidence in the repo that real archive data changed a design decision.
- **Nov 11, 2026, real data beat the design assumption (again).** Fetched bodies did not match the CDX digests (2 of 8). The packaging design stores blobs by SHA-256 of their bytes. The certificate reports the CDX digest only as the archive's claim. Evidence: `docs/spike/m1-y2k.md` ("Blob verification") and `fixtures/snapshots/y2k-briefings/blobs.tsv`.
- **Nov 11, 2026, end-to-end restore.** `crates/revenant-core/tests/restore_briefings.rs` classifies the briefings page at 2000-03-02, loads the blobs, packages the ZIP, and pins its SHA-256. Packaging property tests cover path safety and order-independent output (`tests/package_props.rs`). 40 tests pass.
- **Nov 11, 2026, third spec amendment (R4).** Two y2k captures with different CDX digests served identical bytes, so a digest difference could raise a false Uncertain. The two demo Uncertain cases were re-checked and their bytes do differ. The classifier now compares content hashes when both sides are known (`Capture::same_content`).
- **Nov 11, 2026, spec first, then the pipeline.** R5 (resurrection pipeline) and R5.10 (an archive failure is `UNKNOWN`, never `MISSING`) were written into `requirements.md` before `pipeline.rs`. The 14 tests in `crates/revenant-core/tests/pipeline.rs` map one-to-one to R5.1 through R5.10. 74 tests pass across the workspace.
- **Nov 11, 2026, API built from its spec.** `.kiro/specs/resurrection-api/requirements.md` (A1 to A4) came first. `services/api/tests/api.rs` has one or more tests per requirement, including the preview path-traversal cases. The spec changed once during the build: an oversize body returns 413, not 400, because that is what the framework's body limit returns, so A1.2 was amended. 97 tests pass across the workspace.
- **Nov 11, 2026, end to end on a real server.** Started the binary, POSTed a job, downloaded the ZIP (hash matched the API's), checked the preview headers, extracted the ZIP, and ran `grep -r web.archive.org` on it: no matches.
- **Nov 11, 2026, the UI was reviewed in a real browser, and the review changed the product.** Playwright drove the full flow on desktop and a 390 px phone. Screenshots caught: the favicon's line ends poking past the tombstone at 16 and 32 px (fixed by computing the ends from the stroke width), a specimen blurb with the wrong day count, a comparison caption that claimed images "never existed" when the evidence only shows they were captured later (reworded to what the data supports), and a tall hero that pushed the main button below the fold. Spec: `.kiro/specs/web-ritual/requirements.md`.
- **Nov 11, 2026, hooks and agents, tested before being trusted.** Five hooks in `.kiro/hooks/`: engine tests on save, fixture tests when a capture index or blob manifest changes, a quick verify when the agent stops, a spec-first reminder on each prompt, and a PreToolUse guard that blocks `git commit` and `git push` when the staged diff holds a credential. The guard exists because a real token was pasted into this project's chat. It was tested three ways: a clean commit passes, an unrelated command passes, and a staged fake `cfat_` token exits 2 with the offending line. Two custom agents in `.kiro/agents/`: `revenant-rust` (spec-first, with the steering and both specs pinned) and `revenant-reviewer` (read-only, attacks the preview boundary and audits claims). `scripts/verify.sh` is shared by the Stop hook, the README and CI.
- **Nov 11, 2026, the first real run against the live archive exercised a rule the spec only imagined.** `POST /api/live` restored the y2k briefings page from the real Wayback Machine in about 90 seconds. For one image (`images/3.gif`) the archive stalled until the 25 s timeout. The run reported it as `UNKNOWN`, left it out of the Completeness denominator (54.5% instead of 58.3%), and counted it in `unknown_files`. R5.10 ("a failed lookup is UNKNOWN, never MISSING") was written before any live code existed. Tests for all of this run against a local fake archive that serves the real fixture and misbehaves on demand (`services/api/tests/live.rs`): rate limits, 403, 500, off-host redirects, oversize bodies, concurrency caps. 122 Rust tests pass.
- **Nov 11, 2026, an adversarial pass found a real bug in my own work.** Reading the code as an attacker, the rate limiter trusted the left-most `X-Forwarded-For` entry, which the client writes. The `fly.toml` and `render.yaml` I had just written turned that on. Reproduced against a running server: 40 of 40 requests accepted with a different spoofed address each. The requirement was written first (A1.6: count from the right, by the number of trusted proxies; A1.7: a byte budget for retained jobs, because a count cap alone let about 16 MiB per live job pile up), then the code, then four tests. The same attack afterwards: 30 accepted, 10 limited. The finding is in `.kiro/steering/security.md` so it applies to every later change. 128 Rust tests pass.
- **Nov 12, 2026, a fresh review found the headline result was wrong, and the fix started in the spec.** Reading the origin server's `x-archive-orig-last-modified` header on the demo files showed that the six images the archive "first saved on 2000-03-04" had last changed on 1999-12-27, and that the page had been replaced at 01:06 on 2000-03-02. Capture dates alone had called the images anachronisms (Fidelity 0%) and had restored the stale copy of the page. Requirements R6 (server evidence, a new `ATTESTED` verdict, contradiction rules) and R7 (a playback-style `nearest` baseline) were written first. R7.3 was then amended once the code showed whole ZIPs could not be identical (the certificate names the policy). Evidence: `crates/revenant-core/src/fidelity.rs`, `tests/properties.rs`.
- **Nov 12, 2026, a property test guarding the property tests.** After adding nine properties I measured whether the generator ever produced `ATTESTED`: it did in 225 of 4,000 cases, and every other verdict in at least 8%. `the_generator_reaches_every_verdict` now fails if any verdict drops below 1%, so the server-evidence properties cannot silently become vacuous. Two of the properties are real constraints, not restatements: server evidence can only promote `ANACHRONISM`/`UNCERTAIN` to `ATTESTED` and never makes a verdict worse, and `ATTESTED` is never given when an earlier capture contradicts the header.
- **Nov 12, 2026, confirmed against the real archive, not only the fake one.** A live restore of the same page now returns Fidelity 100% (and 0% from capture dates alone), with the page resolved to the copy last modified at 01:06 on 2000-03-02. Asking for the server date adds no archive requests: the capture fetched to read the header is the one used later (test `asking_for_the_server_date_costs_no_extra_archive_request`). 155 Rust tests pass.
