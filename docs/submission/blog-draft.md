# Blog post: outline and facts (not a finished post)

Written as part of the Kiroween 2026 hackathon. The Bonus Blog Prize needs a public post on a third-party platform (dev.to, Hashnode, Medium) that says so, published during the submission period (before Dec 4, 2:00 PM PT).

This file gives you the facts, in the order a post could tell them. It is deliberately not written as polished prose: the post should sound like you. Everything below is true and checkable in the repository.

## A possible spine

**Opening.** The thing I got wrong first. My plan said "nobody restores dead websites for non-developers". Research the same day showed several paid tools do exactly that. A competing Kiroween entry (Necrolink) also uses the archive. So the product changed from "a friendlier downloader" to a question those tools do not answer: *which of these files are actually from the day I care about?* The measured result behind it: at most about 1 archived page in 5 existed as presented (Ainsworth, Nelson and Van de Sompel, ACM Hypertext 2015).

**What real data did to the spec.** Four times, the archive disagreed with my plan, and each time the requirement changed before the code:
1. A page changed between two captures, and my four verdicts could not say so. Added `UNCERTAIN`.
2. Five images I had called "anachronisms" were archived only as HTTP 404 error pages. My first spike counted those as captures. It was wrong. Now only 200 captures count.
3. Two archive digests differed while the bytes were identical. Content hashes are compared first.
4. A security review found the rate limit trusted a header the client writes.

**The review that found a bug in my own work.** I read my code as an attacker and found the rate limiter would believe the left-most `X-Forwarded-For` entry. The deploy configs I had just written turned that on. I reproduced it: 40 of 40 requests accepted with a different fake address each. After the fix: 30 accepted, 10 limited. Then I ran a read-only custom agent over the live-mode code. It returned six findings. One reproduced on a running server (`http://0x7f.0x1/` was accepted although the README said IP addresses were rejected). I fixed three, and wrote the other three down as limitations instead of rushing a design change before a deadline.

**What I did not do, and why it matters.** Hooks were registered late and never fired during the build. A second agent was written but not run. No Crew, no powers. Property tests pinned invariants but did not catch a bug unit tests missed. Saying so is the point of the write-up.

**Closing.** The honest version of "AI helped me build this": it made it cheap to try the product against the real archive many times, and the repository's requirements file is a record of every place my first idea was wrong.

## Facts you can quote

- Built in Kiro CLI 3.2.0. Rust (Axum, a pure engine crate), Next.js static export, one container (140 MB, non-root).
- 131 Rust tests. 11 property tests. One script (`scripts/verify.sh`) runs format, clippy, all tests, the web build, and an end-to-end check.
- The same inputs give the same ZIP bytes. The seeded ZIP hash is identical on the host and inside the container.
- A live restore of one page takes about a minute, because requests to the archive are serial and at least 1.2 seconds apart, on purpose.
- First live run: one image stalled until the 25 second timeout. Revenant reported it as Unknown and left it out of the score (54.5% instead of 58.3%) instead of calling it lost.

## Do not claim

- That hooks improved the build (they did not fire).
- That property tests found a bug (they did not).
- A speed-up from specs over vibe coding (not measured).
- That Revenant is unique. Cutting off later captures is not new. A per-file verdict and the side-by-side comparison are what the README claims.
