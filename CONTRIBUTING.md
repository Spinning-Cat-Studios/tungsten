# Contributing to Tungsten

Thank you for your interest in Tungsten.

## Current Status (v1.0)

Tungsten v1.0 is focused on stabilizing the self-hosted compiler and establishing the release pipeline. Development is currently single-maintainer.

**Pull requests are not being accepted for v1.0.** This is a bandwidth decision — the project is at a stage where reviewing and integrating external changes would slow down core stabilisation work. Community contributions will open in v1.5 once the upstream workflow and CI are in place.

## What's Welcome Now

Bug reports, questions, and feedback are genuinely appreciated:

- **Bug reports** — with a minimal `.tg` reproduction (see `examples/` for format)
- **Questions and ideas** — via GitHub Issues or Discussions
- **Documentation issues** — typos, unclear explanations, broken links

These help improve the project and are always welcome.

## v1.5 and Beyond

Starting with v1.5, Tungsten will accept community contributions through a structured process. Contribution guidelines, CI for external PRs, and a workflow for integrating changes will be documented before then.

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
