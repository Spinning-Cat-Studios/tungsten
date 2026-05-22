//! Static registry of self-test programs.
//!
//! Default tier (always run): hello, answer, option
//! Full tier (--full only): additional programs

use super::{TestEntry, Tier};

pub(super) const TEST_REGISTRY: &[TestEntry] = &[
    // Default tier — core smoke tests
    TestEntry {
        name: "hello",
        file: "examples/hello.tg",
        expected_output: "hello world\n",
        tier: Tier::Default,
    },
    TestEntry {
        name: "answer",
        file: "examples/answer.tg",
        expected_output: "42\n",
        tier: Tier::Default,
    },
    TestEntry {
        name: "option",
        file: "examples/option.tg",
        expected_output: "42\n",
        tier: Tier::Default,
    },
    // Full tier — extended coverage
    TestEntry {
        name: "arithmetic",
        file: "examples/arithmetic.tg",
        expected_output: "",
        tier: Tier::Full,
    },
    TestEntry {
        name: "strings",
        file: "examples/strings.tg",
        expected_output: "",
        tier: Tier::Full,
    },
    TestEntry {
        name: "logic",
        file: "examples/logic.tg",
        expected_output: "",
        tier: Tier::Full,
    },
    // Phase 2 additions (ADR 17.4.26a §3.2 — T4.1)
    TestEntry {
        name: "pair",
        file: "examples/pair.tg",
        expected_output: "",
        tier: Tier::Full,
    },
    TestEntry {
        name: "list_ops",
        file: "examples/list_ops.tg",
        expected_output: "",
        tier: Tier::Full,
    },
    TestEntry {
        name: "result",
        file: "examples/result.tg",
        expected_output: "",
        tier: Tier::Full,
    },
    TestEntry {
        name: "ordering",
        file: "examples/ordering.tg",
        expected_output: "",
        tier: Tier::Full,
    },
];
