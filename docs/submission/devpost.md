# Devpost submission text: Revenant

Rewritten Nov 12, 2026 to match the shipped product (server evidence, the dates-alone comparison, the annotated view, stylesheets, the month strip, the Fly.io deployment). Fields below follow the live form (`get_submission_requirements`).

## Project name (max 60)

Revenant

## Tagline (max 200)

Bring a dead website back as it was on a day you choose, and see which files are true. Every file carries its capture date, its source and a fidelity verdict.

## Description

**The problem.** When a website dies, the Wayback Machine often holds a copy. But each file on a page is saved at a different moment, and archive playback quietly mixes them, so you see a page that may never have existed in that form. Tools that restore sites from the archive exist and are built to hand you files. If the site was yours, or you must rely on what it said on a given day, a page that looks complete but mixes in files from another day is worse than an honest gap.

**What Revenant does.** You pick a page and a date. Revenant rebuilds the page from the public archive and tells you, file by file, whether it can be trusted for that day.

1. *The slab.* Choose the stored specimen, or paste the address of a page of your own (live). A month strip shows every month the archive holds a copy of the address, so you can pick a day that exists.
2. *The autopsy.* Completeness (the share of the page's files the archive holds a usable copy of) and Fidelity (the share of those files known or likely to look the same on your day), plus a per-file ledger with a verdict and the reason for it: Proven, Likely, Attested, Uncertain, Anachronism, Missing or Unknown.
3. *The resurrection.* The page restored two ways over one viewport, with a divider that sweeps once to show what it does. Left: what a method that trusts capture dates alone can vouch for. Right: what can be shown once the origin server's own dates are used as evidence. Every picture on the restored page is tagged with its verdict and date, and a missing picture is a labelled placeholder, never a broken icon.
4. *The certificate.* A ZIP with each file's capture time, source, digest and verdict, and the commands to check it yourself. The same input gives the same ZIP, byte for byte.

**The result that shaped it.** On the stored specimen (a retired US federal page, on 2 March 2000) capture dates alone score Fidelity 0%: the archive saved six images two days after that day. The archive also kept the origin server's own `Last-Modified` header for each file, and those headers say the images last changed on 27 December 1999. Revenant treats that as evidence, labels it as the server's claim, and ignores it when it contradicts what the archive itself saw. With it, Fidelity is 100%, and the same run without it is on screen next to it.

**What is real.** Runs are labelled SEEDED (saved copies of archive data) or LIVE (the Internet Archive was asked during the run). A lookup that fails is shown as Unknown and kept out of the score, never counted as lost. Revenant also follows stylesheets (`@import`, `url()`, `<style>`, `style=""`), so images reached only through CSS are found. There is no AI model in the loop: every verdict is a deterministic function of capture times, server dates and content.

**Security.** The address a visitor types is never fetched; only its host and path are sent as an encoded query to the archive's index. Previews are served with a sandboxing Content-Security-Policy, inside `sandbox=""` frames, with scripts removed. ZIP paths are validated. Requests, bodies, runs and retained memory are capped. The rate limit reads the client address from the right of `X-Forwarded-For` by the number of trusted proxies, and this was verified against the real Fly.io proxy.

**Limits.** One page at a time; links to other pages are flagged, not followed. Scripts are removed, so a Flash-only page restores as its fallback text. No per-client quotas yet. The live run takes from under a minute to a few minutes because the archive is asked one question at a time, on purpose. The app runs on one machine that sleeps when idle, so the first request after a pause takes about two seconds and jobs do not survive a sleep.

**The specimen** is part of Y2K.gov, a US federal site whose domain no longer resolves. Works of US federal employees are not under copyright, but the authorship of each graphic on the page is not confirmed, and the repository says so (`fixtures/snapshots/y2k-briefings/PROVENANCE.md`).

## Built with (tags)

rust, axum, tokio, serde, lol-html, next.js, typescript, react, internet-archive, kiro

## Category

Resurrection

## Public GitHub repo

https://github.com/PhiBao/revenant

## Working demo URL

https://revenant-kiroween.fly.dev

## Demo video

https://youtu.be/zM3AhgM-mk8

## Testing instructions for judges

1. Open the app. The stored specimen (the Y2K.gov briefings page) is preselected. The machine sleeps when idle, so the first load can take a couple of seconds.
2. Press *Begin autopsy*, then *Resurrect it*. This is labelled SEEDED and takes about a second. Watch the divider sweep between the two restorations, or drag it.
3. Choose *A page of your own*, press *Try the Y2K.gov page, live*, tick the rights box, and press *Begin autopsy*. This asks the real Internet Archive one question at a time and takes from under a minute to a few. It is labelled LIVE. A file the archive will not answer for is shown as Unknown.
4. Try *Show what the archive holds* with `http://www.fema.gov/` to see the month strip (324 months).
5. In the certificate, copy the "Check it yourself" commands. `sha256sum` must print the hash shown on the page, and `grep -r "web.archive.org"` on the unzipped files must print nothing.
6. No login is needed. Locally: `docker compose up`, or `bash scripts/verify.sh` runs formatting, lints, 222 Rust tests, the web build and an end-to-end check.

## Explain how your project meets the main technology requirement (Kiro)

Built entirely in **Kiro CLI 3.2.0** (no IDE, Web or Crew). The full write-up, with file paths and a dated evidence log, is `KIRO_USAGE.md` in the repository root.

- **Specs:** three in `.kiro/specs/` (engine, API, web) in EARS form. Every test maps to a requirement id. Real data changed the specs repeatedly, and each change was written into the spec before the code: a fifth verdict (`UNCERTAIN`), a rule that only HTTP 200 captures count, content-hash comparison, server evidence and the `ATTESTED` verdict, a dates-only comparison restore, and a cold-start retry found while recording the demo. One requirement was wrong and a test said so: the dates-only restore was specified as costing the archive nothing, and a live test showed one extra request, so the requirement was amended.
- **Steering:** five always-on files in `.kiro/steering/` (product, tech, structure, security, testing). The security file carries a finding from the build (the rate limit trusted a header the client writes).
- **Hooks:** five in `.kiro/hooks/`, including a PreToolUse guard that blocks `git commit` and `git push` when the staged diff contains a credential, and a prompt hook that reminds the agent to amend the spec before changing code. The hooks were registered late and did not fire during the first part of the build; the guard script was tested by hand.
- **Custom agents:** two in `.kiro/agents/`. `revenant-reviewer` is read-only and was run for real; it returned six findings, one reproduced on a running server and fixed. `revenant-rust` was written but not run.
- **MCP:** the Devpost MCP server (OAuth), used to read the rules and the exact submission form, and to prepare and make this submission. No Revenant MCP server was built.
- **Property-based testing:** property tests (proptest) for order-independence of classification, no false anachronism, ZIP path safety, a parser and rewriter that never panic, and the server-evidence rules (it only promotes a verdict, never demotes one). A generator-coverage test fails if any verdict becomes unreachable, so the properties cannot go vacuous. Honest result: they pinned invariants, and a mutation check on the dates-only rule was caught by an ordinary test. One property test failed because the test's own check was wrong (it matched a substring inside a word), which is recorded in the log.
- **Powers:** none used.

## Pre-existing code or work

None. The repository was created on 2026-11-11, inside the submission period. Dependencies are listed in `Cargo.toml` and `apps/web/package.json`. Fonts (Fraunces, IBM Plex Mono) are open-licensed packages.

## Bonus prizes

None selected. (Blog Prize and Social Blitz need a public post that does not exist yet.)

## Fields only the submitter can answer

Country of residence; age of majority; eligible jurisdiction; submitter type; the two feedback questions; the answer to Kiroween 2025 ("No") and sponsor employment ("No"), given by the submitter on Nov 12, 2026.
