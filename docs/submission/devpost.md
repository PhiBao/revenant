# Devpost submission text: Revenant

Prepared from the live form (`get_submission_requirements`, read Nov 11, 2026). Nothing here has been submitted. Paste into the Devpost form, and edit anything that is not in your voice.

## Project name (max 60)

Revenant

## Tagline (max 200)

Bring a dead website back as it was on a day you choose, and see which files are true. Every file carries its capture date and a fidelity verdict.

## Description (features and functionality)

**The problem.** When a website dies, the Wayback Machine often holds a copy. But each file on a page is saved at a different moment, and archive playback quietly mixes them, so you see a page that never existed in that form. A 2015 study found that at most about one archived page in five was both complete and consistent in time. Tools that restore sites from the archive exist and sell well, but they are built to hand you files. If you are the person the site belonged to, or you must rely on what it said on a given day, a page that looks complete but mixes in files from another year is worse than an honest gap.

**What Revenant does.** You pick a page and a date. Revenant rebuilds the page from the public archive and tells you which parts are true.

1. *The slab.* Choose a stored specimen, or paste the address of a page of your own (live).
2. *The autopsy.* Two numbers and a ledger. Completeness is the share of the page's files that the archive holds a usable copy of. Fidelity is the share of those files known, or likely, to look the same on the chosen day. Every file gets a verdict: Proven, Likely, Uncertain, Anachronism, Missing, or Unknown. Verdicts are shown as a word and a shape as well as a color.
3. *The resurrection.* The same page restored two ways over one viewport: with the nearest capture of every file, and with only what can be shown as it was that day. A draggable divider compares them.
4. *The certificate.* Download either ZIP. The certificate lists every file's capture time, its distance from the chosen day, and a SHA-256 for the ZIP, with the two shell commands to check it yourself.

**What is real.** Runs are labelled SEEDED (saved copies of archive data) or LIVE (the Internet Archive was asked during the run). A file the archive cannot be asked about is shown as Unknown and kept out of the score, never counted as lost. The same inputs give the same ZIP, byte for byte. There is no AI model in the loop: every verdict is a deterministic function of capture times and content.

**The specimen** is part of Y2K.gov, a US federal government site whose domain no longer resolves. Works by US federal employees are not under copyright (17 U.S.C. §105). The authorship of each graphic on the page is not confirmed, and the pages carry no copyright notice; the repository says so.

**Security.** The address a visitor types is never fetched; its host and path are only sent as an encoded query to the archive's index. Previews are served from `/p/` only, with a sandboxing Content-Security-Policy, inside `sandbox=""` frames, with scripts removed. ZIP paths are validated. Requests, bodies, runs and retained memory are capped.

**Limitations.** One page at a time; links to other pages are flagged, not followed. Scripts are removed. No per-client quotas yet. Details are in the README.

## Category

Resurrection

## Public GitHub repo

https://github.com/PhiBao/revenant

## Working demo URL

*Not set. See "Before you submit" below.* The repository runs with one command: `docker compose up`.

## Testing instructions for judges

1. Open the app. The stored specimen (Y2K.gov briefings page) is preselected with a default date.
2. Press *Begin autopsy*, then *Resurrect it*. This is labelled SEEDED and takes about a second.
3. Choose *A page of your own*, press *Try the Y2K.gov page, live*, tick the rights box, and press *Begin autopsy*. This asks the real Internet Archive one question at a time and takes about a minute. It is labelled LIVE. If the archive is slow it may take longer, and a file it will not answer for is shown as Unknown.
4. In the certificate, copy the "Check it yourself" commands. `sha256sum` must print the hash shown on the page, and `grep -r "web.archive.org"` on the unzipped files must print nothing.
5. No login is needed.
6. Everything is checked by `bash scripts/verify.sh`.

## Explain how your project meets the main technology requirement (Kiro)

Built entirely in **Kiro CLI 3.2.0** (no IDE, Web or Crew). Full, honest write-up with file paths: `KIRO_USAGE.md` in the repository root.

- **Specs:** three in `.kiro/specs/` (engine, API, web). Requirements are in EARS form and every test maps to a requirement id. Real data changed the specs four times, each logged in the spec before the code changed: a fifth verdict (`UNCERTAIN`), a rule that only HTTP 200 captures count, content-hash comparison, and a security review's findings.
- **Steering:** five always-on files in `.kiro/steering/` (product, tech, structure, security, testing).
- **Hooks:** five in `.kiro/hooks/`, including a PreToolUse guard that blocks `git commit` and `git push` when the staged diff contains a credential. The guard script is tested by hand; the hooks were registered late in the session and did not fire during the build.
- **Custom agents:** two in `.kiro/agents/`. `revenant-reviewer` is read-only and was run for real with `kiro-cli chat --agent revenant-reviewer --no-interactive`. It returned six findings; one was reproduced on a running server and fixed, two more fixed, three documented as limitations. `revenant-rust` was written but not run.
- **MCP:** the Devpost MCP server (OAuth), used read-only to match this text to the real submission form.
- **Property-based testing:** eleven property tests. They pinned invariants; they did not catch a bug that unit tests missed, and the write-up says so.
- **Powers:** none used.

## Pre-existing code or work

None. The repository was created on 2026-11-11, inside the submission period. Dependencies are listed in `Cargo.toml` and `apps/web/package.json`. Fonts (Fraunces, IBM Plex Mono) are open-licensed packages.

## Bonus prizes (suggestion only)

- **Blog Prize:** yes, if you publish a post on a third-party platform that mentions Kiroween 2026. A draft outline is in `blog-draft.md`.
- **Social Blitz:** only if you write the post yourself. It is judged on original thoughts.
- **Best Kiro Crew App:** not entering. No Crew app was built.
- **Startup, Student, Ghost of Kiroween Past:** only if each is true for you.

## Fields only you can answer

Country of residence; age of majority; eligible jurisdiction; sponsor employment (must be "No" to stay eligible); registered for Kiroween 2025; submitter type; the two feedback questions; and the exact eligibility text on the Rules tab.

## Before you submit

1. **A working demo URL is required** by the form. `fly.toml` and `render.yaml` are ready; deploying creates a public service on your account, so it needs your go-ahead.
2. **Upload the video** (`/home/kiter/kiro-submission/revenant-demo.mp4`, 2:23, no audio) to YouTube as public, and paste the link.
3. **Check the video for third-party material.** It shows the archived Y2K.gov page (likely free of copyright, authorship of the logo unconfirmed), a short quote from a 2015 paper's title, and no music.
4. **Confirm nothing in the specimen pages is third-party licensed** (see `fixtures/snapshots/y2k/PROVENANCE.md`, open item).
