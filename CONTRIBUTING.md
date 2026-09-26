# Contributing to Tungsten

Thank you for your interest in Tungsten.

## Current Status (2.0-alpha)

Tungsten is a research language under active single-maintainer development. The 2.0-alpha pre-release is available for experimentation, and its language surface may change before 2.0.

**Pull requests are not being accepted at this time.** This is a bandwidth decision — the project is at a stage where reviewing and integrating external changes would slow down core development work. A structured contribution workflow (CI for external PRs, review process, contributor guidelines) is planned for v2.1.

## What's Welcome Now

Bug reports, questions, and feedback are genuinely appreciated:

- **Bug reports** — with a minimal `.tg` reproduction (see `examples/` for format)
- **Questions and ideas** — via GitHub Issues or Discussions
- **Documentation issues** — typos, unclear explanations, broken links

These help improve the project and are always welcome.

## v2.1 and Beyond

The v2.1 roadmap includes a structured contribution process. Guidelines, CI for external PRs, and an integration workflow will be documented when that work is ready.

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
