# Testing rules

- Property-based tests first for the core engine (proptest). Each property maps to a requirement in the spec.
- Required properties:
  - URL normalization is idempotent.
  - Rewritten output never contains a web.archive.org host.
  - Time-slice selection never returns ANACHRONISM when PROVEN or LIKELY exists.
  - Classification does not depend on the order of input captures.
  - ZIP path sanitization rejects traversal for arbitrary strings.
  - The parser and rewriter never panic on arbitrary bytes.
- Run before every commit: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.
- Use `cargo test` without watch mode. Do not add tests beyond the spec's acceptance criteria unless the user asks.
- Record any bug that property tests catch, with the commit that fixed it. This feeds KIRO_USAGE.md.
