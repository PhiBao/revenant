# Tasks: revenant-core engine

- [x] T1 Workspace and crate skeleton (`Cargo.toml`, `crates/revenant-core`). Covers the toolchain pin.
- [x] T2 CDX timestamp parser with unit tests for R1.1 and R1.2. Covers R1.3 through a property test.
- [x] T3 Classifier: `Capture`, `Verdict`, `Selection`, `classify`. Covers R2.1 through R2.5 and R3.4.
- [x] T4 Property tests: order invariance (R3.1), no false anachronism (R3.2), proven digest match (R3.3), parser never panics (R1.3).
- [x] T5 Real-data unit tests from docs/spike/m1-y2k.md: the 1999 root gives UNCERTAIN, and award.gif gives ANACHRONISM.
- [x] T6 Fixture-driven test: load `fixtures/snapshots/y2k/` and assert the verdicts above. Needs the fixture from the next task.
- [x] T7 Capture the fixture from the archive with PROVENANCE.md.
- [x] T8 Autopsy: Completeness and Fidelity over a page's referenced files, with a per-file ledger (`src/autopsy.rs`, 3 unit tests). Definitions from PLAN.md §R.3.
- [x] T9 HTML rewriter (`src/rewrite.rs`): unwrap archive URLs, repair links, strip scripts, handlers and comments. Unit, property and real-page tests. Covers R5.7.
- [x] T10 `SnapshotSource` trait, `SeededSource::from_dir`, and `resurrect()` (`src/pipeline.rs`). 14 tests in `tests/pipeline.rs`. Covers R5.1 to R5.10.
- [ ] T11 Live Wayback source (needs a network crate; goes in the API service, not in this crate).
- [ ] T12 R6: `origin_modified`, `parse_http_date`, `Verdict::Attested`, `Basis`, classify rules, property tests.
- [ ] T13 R6.8/R6.9: pipeline asks the source for the origin time, certificate records basis and dates-only vitals.
- [ ] T14 R7: `nearest` restore.
- [x] T12 R6 server evidence. [x] T13 pipeline asks for the origin date. [x] T14 R7 nearest. [x] T15 R8 annotated view.
- [ ] T16 R9: CSS reader (`src/css.rs`), `<style>` and `style=""` in the HTML rewriter, pipeline worklist with depth and cap.
