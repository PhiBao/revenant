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
