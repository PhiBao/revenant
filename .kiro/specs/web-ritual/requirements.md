# Requirements: web ritual

Source: PLAN.md §7 (design direction) and §12.9 (failure states), as amended by §R. Format: EARS.

## W1 Flow
- W1.1 The page SHALL be one column and one flow in four acts: Slab (choose), Autopsy (vital signs and ledger), Resurrection (compare), Certificate (verify and download).
- W1.2 WHEN the visitor presses "Begin autopsy" THEN the UI SHALL create a strict job and an include-anachronisms job for the same example and date, and SHALL show Completeness and Fidelity from the strict job.
- W1.3 The first screen SHALL need no account and no typing: an example is preselected with its default date.
- W1.4 The UI SHALL label every result SEEDED or LIVE using the mode from the API, and SHALL NOT infer it.

## W2 Compare
- W2.1 The comparison SHALL show the include-anachronisms restore ("what other tools give you") and the strict restore ("what was true on that day") over the same viewport, with a draggable divider and a keyboard-operable control.
- W2.2 On narrow screens the comparison SHALL become a two-option toggle.
- W2.3 Both previews SHALL be embedded with `sandbox=""` and SHALL load only from `/p/`.

## W3 Ledger and verification
- W3.1 The ledger SHALL list every file with verdict, capture date, and distance from the target in days, and SHALL show verdicts as text as well as color.
- W3.2 The certificate act SHALL show the ZIP SHA-256 from the API and the two shell commands that check the download (`sha256sum` and `grep -r web.archive.org`).
- W3.3 The UI SHALL offer the ZIP of either restore and say plainly which is which.

## W4 States
- W4.1 WHEN the API answers `no_capture` THEN the UI SHALL show a death certificate with the verdict and the next step, and SHALL offer the alternate dates.
- W4.2 WHEN the API answers 429 THEN the UI SHALL show the wait time. WHEN the network fails THEN the UI SHALL say the API is unreachable and offer a retry.
- W4.3 Loading SHALL be shown as progress on the slab, not as a spinner.

## W5 Accessibility and identity
- W5.1 Color SHALL never be the only carrier of a verdict.
- W5.2 WHEN `prefers-reduced-motion` is set THEN no animation SHALL run and all content SHALL be visible.
- W5.3 The page SHALL work at 360 px width and with the keyboard alone.
- W5.4 The site SHALL ship an SVG favicon, a 32 px and a 180 px PNG, and `favicon.ico`, all showing the tombstone-with-EKG mark.

## W6 Evidence on the page (added Nov 12, 2026)

Source: engine requirements R6 and R7.

- W6.1 The ledger SHALL show the `ATTESTED` verdict as a word and a shape, and SHALL show, for every file, why it got its verdict (the certificate's `basis`) and when the server says the file last changed.
- W6.2 WHEN Fidelity from capture dates alone differs from Fidelity with server evidence THEN the autopsy SHALL show both numbers side by side and say what the difference is, in words.
- W6.3 The second restore SHALL be the `nearest` restore, labelled as what archive playback does.
- W6.4 WHEN the nearest restore and the strict restore contain the same files with the same hashes THEN the comparison view SHALL NOT be shown. In its place a short statement SHALL say that both methods give the same page, and that Revenant's difference is that it can show why each file is trusted.
- W6.5 WHEN the two restores differ THEN the comparison view SHALL be shown, with the labels "What archive playback gives" and "What can be shown as it was on <date>".
- W6.7 A ledger line SHALL start with a capital letter, SHALL NOT repeat itself, and SHALL NOT contain implementation detail (hashing, rewriting). Detail that belongs to the certificate as a whole SHALL be in its notes.
- W6.6 The target time SHALL be described as "the end of <date>, UTC" wherever the date is shown with a result, because a page can change during the day.

## W7 Honest waiting (added Nov 12, 2026)

Why: a real 55-file page took 201 seconds live, and the form said "about a minute". A wait that is longer than promised reads as a hang.

- W7.1 The live option SHALL say that a restore takes from under a minute for a small page to a few minutes for a large one, and why (the archive is asked one question at a time, on purpose).
- W7.2 WHILE a live run is in progress THEN the page SHALL show what it is doing now, the number of questions asked, and the elapsed time, and SHALL update at least every two seconds.
- W7.3 The page SHALL NOT show a percentage or a time-left estimate, because the number of files is not known until the page has been read.
- W7.4 WHEN a live run has taken longer than 90 seconds THEN the page SHALL say the page is a large one, and that leaving it open is fine.
