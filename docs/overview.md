# Tungsten Overview

**This document has moved to <https://spinningcatstudios.com/tungsten>.**

*The linked pages were reconciled against the 2.0-alpha compiler on 2026-09-25.*

The Tungsten documentation now lives on the web, where it can be kept current
against the language and linked to directly:

| You want | Go to |
|---|---|
| What Tungsten is, and why | <https://spinningcatstudios.com/tungsten> |
| Getting started | <https://spinningcatstudios.com/tungsten/start/install> |
| The language manual | <https://spinningcatstudios.com/tungsten/language/values-and-types> |
| Proofs | <https://spinningcatstudios.com/tungsten/proofs/curry-howard> |
| Syntax reference | <https://spinningcatstudios.com/tungsten/reference/syntax> |
| What does not work yet | <https://spinningcatstudios.com/tungsten/reference/limitations> |
| Roadmap | <https://spinningcatstudios.com/tungsten/roadmap> |

## Why it moved

The version of this document that lived here was written against v1.0 and had
drifted materially out of date — it still listed features that v1.5 shipped as
"known limitations". Keeping one copy, checked against the compiler and updated
with each release, is worth more than keeping a stale copy nearby.

The published pages were reconciled against the compiler rather than against the
changelog, so they describe what the compiler actually does, including where it
currently does the wrong thing.

## What stayed here

Compiler-internal material is unaffected and remains in this repository:

- `docs/repo-memory/` — subsystem knowledge for people working *on* the compiler
- the design decisions behind the compiler, and their rationale
- `CHANGELOG` and `ROADMAP.md` — release history and direction

The website is for people writing Tungsten; this repository is for people
writing the compiler.
