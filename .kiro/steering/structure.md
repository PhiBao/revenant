# Repository structure

```
crates/revenant-core/   pure engine: types, CDX parsing, fidelity classifier, selector, rewrite, packaging
crates/revenant-mcp/    (Should) MCP stdio server over revenant-core
services/api/           Axum server: jobs, progress stream, ZIP download, preview serving
apps/web/               (M4) Next.js ritual UI, talks only to the API
fixtures/snapshots/     seeded capture sets; each site has PROVENANCE.md with its rights basis
docs/diagrams/          three diagrams (architecture, pipeline, trust boundary)
.kiro/                  specs/ steering/ hooks/ agents/ settings/  (committed, never gitignored)
KIRO_USAGE.md           write-up mapped to the rules' eight prompts, with file paths and dated evidence
```

Rules:
- `.kiro/` is committed. Do not add it to `.gitignore`.
- Keep the root README and KIRO_USAGE.md accurate with each milestone.
- Commit specs, steering, and hooks with the code they describe.
