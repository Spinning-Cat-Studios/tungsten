# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Tungsten, please report it privately by [opening a GitHub security advisory](https://github.com/Spinning-Cat-Studios/tungsten/security/advisories/new).

Please include:

- A description of the vulnerability
- Steps to reproduce (minimal `.tg` file if applicable)
- Tungsten version and OS

## Response

Tungsten is a personal research project maintained by a single author. Security reports will be handled on a best-effort basis. Expect an initial acknowledgement within a few days.

There is no bug bounty programme.

## Scope

Tungsten is a compiler and proof language. The primary security surface is:

- The compiler accepting malicious `.tg` input (crashes, memory safety issues in the bootstrap compiler)
- The LLVM codegen backend producing unsafe native code from well-typed input

Issues in dependencies (Rust crates, LLVM) should be reported to those projects directly.
