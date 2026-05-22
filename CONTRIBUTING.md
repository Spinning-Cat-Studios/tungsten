# Contributing to Tungsten

Thank you for your interest in Tungsten.

## Current Status (v1.5)

Tungsten is under active single-maintainer development. The v1.5 release focused on compiler architecture, performance, and language ergonomics.

**Pull requests are not being accepted at this time.** This is a bandwidth decision — the project is at a stage where reviewing and integrating external changes would slow down core development work. A structured contribution workflow (CI for external PRs, review process, contributor guidelines) is planned for v2.0.

## What's Welcome Now

Bug reports, questions, and feedback are genuinely appreciated:

- **Bug reports** — with a minimal `.tg` reproduction (see `examples/` for format)
- **Questions and ideas** — via GitHub Issues or Discussions
- **Documentation issues** — typos, unclear explanations, broken links

These help improve the project and are always welcome.

## v2.0 and Beyond

Starting with v2.0, Tungsten will accept community contributions through a structured process. Contribution guidelines, CI for external PRs, and a workflow for integrating changes will be documented before then.

If you have ideas you'd like to discuss in the meantime, opening an issue is the right path.

## Design Policy

Language or architecture changes require an Architecture Decision Record (ADR) and maintainer approval. Please discuss ideas before proposing changes.

## Code Style (for future contributors)

- Rust: `cargo fmt` + `cargo clippy`
- Tungsten: follow conventions in `src/compiler/`
- Tests required for non-trivial changes

## Security

Please report security issues privately. See `SECURITY.md`.

## License

Tungsten is licensed under MIT.
