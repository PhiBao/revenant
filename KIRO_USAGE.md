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
