# How Kiro was used

This write-up follows the eight prompts in the Kiroween 2026 rules. Each section will point to file paths and dated evidence once the milestone producing that evidence is done.

1. Kiro surfaces used and how they differed: pending (M1 onward).
2. Custom agents: pending.
3. Agent hooks: pending.
4. Spec-driven development: pending. Specs will live in `.kiro/specs/`.
5. Steering docs: `.kiro/steering/` (product, tech, structure, security, testing), created in M0.
6. MCP: pending (`revenant-mcp` and the Devpost MCP).
7. Property-based testing: pending (`crates/revenant-core`).
8. Kiro powers: pending.

## Evidence log (dated)

- **Nov 11, 2026, spec amendment before code (spec-driven development).** The y2k.gov spike found a root page with captures on both sides of the target and different digests. PLAN.md's four verdicts could not represent it. The spec (`.kiro/specs/revenant-core-engine/requirements.md`, R2.3) added `UNCERTAIN` before the classifier was written. Evidence: `docs/spike/m1-y2k.md`, `crates/revenant-core/src/fidelity.rs`.
- **Nov 11, 2026, property tests (PBT).** Five property tests in `crates/revenant-core/tests/properties.rs` cover order invariance, no false anachronism, proven digest matching, delta correctness, and a parser that never panics. All 15 engine tests pass, and clippy (`-D warnings`) and fmt are clean.
- **Nov 11, 2026, a spec change caught by real data.** The fixture showed five y2k GIFs with captures that were all HTTP 404. The first spike counted them as captures and called them anachronisms. The spec gained R0 (only 200 captures count), the spike doc was corrected, and the index parser now drops non-200 rows (`crates/revenant-core/src/index.rs`, fixture test `tests/fixture_y2k.rs`). This is the clearest evidence in the repo that real archive data changed a design decision.
- **Nov 11, 2026, real data beat the design assumption (again).** Fetched bodies did not match the CDX digests (2 of 8). The packaging design stores blobs by SHA-256 of their bytes. The certificate reports the CDX digest only as the archive's claim. Evidence: `docs/spike/m1-y2k.md` ("Blob verification") and `fixtures/snapshots/y2k-briefings/blobs.tsv`.
- **Nov 11, 2026, end-to-end restore.** `crates/revenant-core/tests/restore_briefings.rs` classifies the briefings page at 2000-03-02, loads the blobs, packages the ZIP, and pins its SHA-256. Packaging property tests cover path safety and order-independent output (`tests/package_props.rs`). 40 tests pass.
- **Nov 11, 2026, third spec amendment (R4).** Two y2k captures with different CDX digests served identical bytes, so a digest difference could raise a false Uncertain. The two demo Uncertain cases were re-checked and their bytes do differ. The classifier now compares content hashes when both sides are known (`Capture::same_content`).
