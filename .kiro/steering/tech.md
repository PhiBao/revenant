# Tech stack

- Backend: Rust (stable, pinned in rust-toolchain.toml). Axum + Tokio, Serde, Reqwest (rustls) for archive egress only, lol-html for streaming rewrites, zip, sha2, proptest, tracing.
- Engine crate `revenant-core` has no network or framework code. Its I/O sits behind the `SnapshotSource` trait.
- Frontend (M4 only): pnpm, TypeScript, React, Next.js. Fonts: Fraunces (display), IBM Plex Mono (data).
- Persistence: no database in the MVP. The job manifest in a content-addressed blob store is the record.
- Deploy: one container. Axum serves the API and the built web app. Previews run on a separate host.
- Avoid new dependencies unless they clearly earn their place. Record each new one in the README "Dependencies" section with its reason.
