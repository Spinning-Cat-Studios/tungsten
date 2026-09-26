//! The symptom-query corpus itself — data only, split from the gate beside it
//! when the table reached that file's size cap (ADR 18.9.26g).

/// One or more symptom-shaped queries per registry category.
///
/// The rules a query here must satisfy, all three enforced below: it reaches
/// its category ([`super::every_symptom_query_reaches_its_pattern`]), every category
/// has one ([`super::every_registry_category_has_a_symptom_query`]), and it does not
/// simply restate the category name
/// ([`super::symptom_queries_do_not_name_their_own_category`]).
pub(in crate::doctor::suggest_tools) const SYMPTOM_QUERIES: &[(&str, &[&str])] = &[
    (
        "comparator-stuck",
        &[
            "my test passes even when i break the code it checks",
            "the suite is green but i do not think it checked anything",
        ],
    ),
    (
        "segfault",
        &[
            "the compiled program dies the moment it starts",
            "running the binary just prints segmentation fault and stops",
        ],
    ),
    (
        "stack overflow",
        &[
            "it dies on large input but works on a small one",
            "the run is killed after recursing very deep",
        ],
    ),
    (
        "infinite loop",
        &[
            "my compiled program never prints anything and runs forever",
            "the binary sits there and never returns",
        ],
    ),
    (
        "compiler wedge",
        &[
            "tungsten check has printed nothing for ten minutes",
            "the compiler never finishes on this file",
        ],
    ),
    (
        "abi mismatch",
        &[
            "the fields come back in the wrong order after calling into rust",
            "a struct handed to an extern reads as garbage on the other side",
        ],
    ),
    (
        "nested pattern",
        &[
            "the variables bound inside a match arm hold the wrong things",
            "destructuring gives the wrong element",
        ],
    ),
    (
        "miscompile / wrong value",
        &[
            "the answer is wrong but nothing errors",
            "the compiled binary and the evaluator print different results",
        ],
    ),
    (
        "merge-arms lowering divergence",
        &[
            "one branch of my match compiles differently from the other",
            "the two branches return the same type and the compiler disagrees",
        ],
    ),
    (
        "linker / self-compile",
        &[
            "the build fails at the end after everything compiled",
            "building the compiler with itself stops at the last step",
        ],
    ),
    (
        "referenced but not declared",
        &[
            "something my code calls has no definition by the time it is emitted",
            "a name appears in the emitted code that was never defined",
        ],
    ),
    (
        "colliding-name misresolution",
        &[
            "the wrong function runs and i have two with the same name",
            "the call lands in a different module than i expected",
        ],
    ),
    (
        "bootstrap/self-host divergence",
        &[
            "the same file passes one compiler and fails the other",
            "a projection builder disagrees with the type walk beside it",
            "wrong field value",
        ],
    ),
    (
        "stale/warm elaboration cache",
        &[
            "it worked the first time and fails on the second run",
            "the same command worked a minute ago and now finds nothing",
        ],
    ),
    (
        "termination",
        &[
            "my recursive function is rejected and i do not know why",
            "the compiler will not accept a function that calls itself",
        ],
    ),
    (
        "termination-proof-boundary",
        &[
            "my theorem is refused because of a function it calls",
            "a lemma that used to build no longer does",
        ],
    ),
    (
        "type mismatch",
        &[
            "the compiler rejects my function with a type error",
            "it says the two sides do not line up and they look identical",
        ],
    ),
    (
        "elaboration error",
        &[
            "the name is not found even though it is defined right there",
            "the compiler says something is undefined that i can see",
        ],
    ),
    (
        "encoding / μ-type",
        &[
            "the type prints with an α_ in the middle of it",
            "the printed type has a binder in it i did not write",
        ],
    ),
    (
        "mutual recursion",
        &[
            "two types that refer to each other break the build",
            "my types form a cycle and something downstream is wrong",
        ],
    ),
    (
        "constructor / duplicate registration",
        &[
            "the same case is registered twice for my sum type",
            "the variant count it reports is not what i wrote",
        ],
    ),
    (
        "record field",
        &[
            "reading a field gives back the value of a different one",
            "the struct member i asked for is not the one i got",
        ],
    ),
    (
        "cir variant lookup",
        &[
            "i need every place a variant gets built",
            "where is this constructor applied across the module tree",
        ],
    ),
    (
        "match dispatch",
        &[
            "the match fails at runtime saying it is not a sum type",
            "a cross-module match blows up at runtime",
        ],
    ),
    (
        "encoding nondeterminism",
        &[
            "the same input gives a different answer on each run",
            "the build is flaky and nothing changed between runs",
        ],
    ),
    (
        "cross-file error",
        &[
            "the error points at a different file than the one i edited",
            "the mistake is in some other file and not this one",
        ],
    ),
    (
        "import alias",
        &[
            "the name is not in scope even though i imported it",
            "i imported it and the compiler still cannot find it",
        ],
    ),
    (
        "private-item access / name collision",
        &[
            "the compiler says my function is private and it is public",
            "one of two functions with the same name became unreachable",
        ],
    ),
    (
        "compile-time hotspot",
        &[
            "the build takes far too long and i do not know where",
            "checking this file is slow and i want to know why",
        ],
    ),
    (
        "profile symbol attribution",
        &[
            "perf shows a symbol i cannot map back to my code",
            "the profile lists names that are not in my source",
        ],
    ),
    (
        "change-scoped quality gate",
        &[
            "is this violation mine or was it here already",
            "the gate is red and i want only what my change did",
        ],
    ),
    (
        "inspect a definition",
        &[
            "i want to see what my function turned into",
            "show me the definition after the compiler is done with it",
        ],
    ),
    (
        "unexplained proof hole",
        &["check says contains sorry but i wrote no sorry"],
    ),
    (
        "inspect an encoding",
        &[
            "which constructor is inl and which is inr",
            "what does a match lower to for this type",
        ],
    ),
];
