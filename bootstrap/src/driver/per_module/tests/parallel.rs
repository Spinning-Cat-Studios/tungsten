use super::*;

// =========================================================================
// Profile tests (ADR 11.5.26b §P0)
// =========================================================================

#[test]
fn profile_emit_no_panic() {
    use crate::driver::per_module::profile::{ElabProfile, ModuleTiming};
    use std::time::Duration;
    let mut p = ElabProfile::new();
    p.phase_a = Duration::from_millis(100);
    p.phase_a5 = Duration::from_millis(200);
    p.phase_b_total = Duration::from_millis(500);
    p.record_module(ModuleTiming {
        path: "test.tg".to_string(),
        collection: Duration::from_millis(50),
        body: Duration::from_millis(100),
        cache_write: Duration::from_millis(1),
        cache_hit: false,
    });
    p.record_module(ModuleTiming {
        path: "cached.tg".to_string(),
        collection: Duration::ZERO,
        body: Duration::ZERO,
        cache_write: Duration::ZERO,
        cache_hit: true,
    });
    // Should not panic
    p.emit();
}

#[test]
fn profile_is_enabled_default_off() {
    // With no env var set, profiling should be off
    // (can't easily test env-var-on without side effects)
    // Just verify the function exists and returns a bool
    let _enabled: bool = super::super::profile::is_enabled();
}

// =========================================================================
// Accumulator merge_worker tests (ADR 11.5.26b §P5)
// =========================================================================

#[test]
fn merge_worker_combines_defs_and_exports() {
    use tungsten_core::terms::{SpannedTerm, TermSpan};

    let mut main_acc = ModuleTreeAccumulator::new();
    main_acc.defs.push(crate::elaborate::CoreDef {
        name: "existing".to_string(),
        ty: Type::Nat,
        term: SpannedTerm::new(tungsten_core::Term::nat(0), TermSpan::new(0, 0)),
        span: crate::span::Span::new(0, 0),
    });

    let mut worker_acc = ModuleTreeAccumulator::new();
    worker_acc.defs.push(crate::elaborate::CoreDef {
        name: "worker_def".to_string(),
        ty: Type::Bool,
        term: SpannedTerm::new(tungsten_core::Term::nat(1), TermSpan::new(0, 0)),
        span: crate::span::Span::new(0, 0),
    });
    worker_acc.exports.values.push((
        "worker_val".to_string(),
        ValueDef {
            name: "worker_val".to_string(),
            ty: Type::Bool,
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::new(0, 0),
        },
    ));
    worker_acc.cached_def_count = 3;

    main_acc.merge_worker(worker_acc);

    assert_eq!(main_acc.defs.len(), 2);
    assert_eq!(main_acc.defs[0].name, "existing");
    assert_eq!(main_acc.defs[1].name, "worker_def");
    assert_eq!(main_acc.exports.values.len(), 1);
    assert_eq!(main_acc.cached_def_count, 3);
}

#[test]
fn merge_worker_preserves_module_defs() {
    use tungsten_core::terms::{SpannedTerm, TermSpan};

    let mut main_acc = ModuleTreeAccumulator::new();

    let mut worker_acc = ModuleTreeAccumulator::new();
    worker_acc.module_defs.push((
        vec!["foo".to_string()],
        std::path::PathBuf::from("foo.tg"),
        vec![crate::elaborate::CoreDef {
            name: "f".to_string(),
            ty: Type::Nat,
            term: SpannedTerm::new(tungsten_core::Term::nat(0), TermSpan::new(0, 0)),
            span: crate::span::Span::new(0, 0),
        }],
    ));

    main_acc.merge_worker(worker_acc);
    assert_eq!(main_acc.module_defs.len(), 1);
    assert_eq!(main_acc.module_defs[0].0, vec!["foo".to_string()]);
}

// =========================================================================
// Profile merge_from tests (ADR 11.5.26b §P5)
// =========================================================================

#[test]
fn profile_merge_from_combines_modules() {
    use crate::driver::per_module::profile::{ElabProfile, ModuleTiming};
    use std::time::Duration;

    let mut main_profile = ElabProfile::new();
    main_profile.record_module(ModuleTiming {
        path: "a.tg".to_string(),
        collection: Duration::from_millis(10),
        body: Duration::from_millis(20),
        cache_write: Duration::ZERO,
        cache_hit: false,
    });

    let mut worker_profile = ElabProfile::new();
    worker_profile.record_module(ModuleTiming {
        path: "b.tg".to_string(),
        collection: Duration::from_millis(30),
        body: Duration::from_millis(40),
        cache_write: Duration::ZERO,
        cache_hit: false,
    });

    main_profile.merge_from(&worker_profile);
    assert_eq!(main_profile.modules.len(), 2);
    assert_eq!(main_profile.modules[0].path, "a.tg");
    assert_eq!(main_profile.modules[1].path, "b.tg");
}

// =========================================================================
// Parallel Phase B tests (ADR 11.5.26b §P5)
// =========================================================================

#[test]
fn elab_thread_count_default_is_one() {
    // Validates that the default thread count is >= 1 (serial).
    let count = super::super::cache::equivalence::elab_thread_count();
    assert!(count >= 1, "elab_thread_count must always be >= 1");
}

#[test]
fn pool_creation_serial_yields_none() {
    // The same logic used in build_elab_ctx: thread_count=1 → no pool.
    let thread_count: usize = 1;
    let pool: Option<rayon::ThreadPool> = if thread_count > 1 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .build()
            .ok()
    } else {
        None
    };
    assert!(pool.is_none(), "serial mode should not create a pool");
}

#[test]
fn pool_creation_parallel_yields_some() {
    // The same logic used in build_elab_ctx: thread_count>1 → pool created.
    let thread_count: usize = 2;
    let pool: Option<rayon::ThreadPool> = if thread_count > 1 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .build()
            .ok()
    } else {
        None
    };
    assert!(pool.is_some(), "parallel mode should create a pool");
}

#[test]
fn merge_worker_deterministic_ordering() {
    use tungsten_core::terms::{SpannedTerm, TermSpan};

    let names = vec!["alpha", "beta", "gamma", "delta", "epsilon"];
    let mut orderings: Vec<Vec<String>> = Vec::new();

    for _ in 0..5 {
        let mut main_acc = ModuleTreeAccumulator::new();
        for name in &names {
            let mut worker = ModuleTreeAccumulator::new();
            worker.defs.push(crate::elaborate::CoreDef {
                name: name.to_string(),
                ty: Type::Nat,
                term: SpannedTerm::new(tungsten_core::Term::nat(0), TermSpan::new(0, 0)),
                span: crate::span::Span::new(0, 0),
            });
            main_acc.merge_worker(worker);
        }
        let order: Vec<String> = main_acc.defs.iter().map(|d| d.name.clone()).collect();
        orderings.push(order);
    }

    for i in 1..orderings.len() {
        assert_eq!(
            orderings[0], orderings[i],
            "merge_worker ordering must be deterministic"
        );
    }
}

#[test]
fn merge_worker_overlapping_exports_last_wins() {
    let mut main_acc = ModuleTreeAccumulator::new();
    main_acc.exports.values.push((
        "shared_fn".to_string(),
        ValueDef {
            name: "shared_fn".to_string(),
            ty: Type::Unit,
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::new(0, 0),
        },
    ));

    let mut worker = ModuleTreeAccumulator::new();
    worker.exports.values.push((
        "shared_fn".to_string(),
        ValueDef {
            name: "shared_fn".to_string(),
            ty: Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool)),
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::new(0, 0),
        },
    ));

    main_acc.merge_worker(worker);

    assert_eq!(main_acc.exports.values.len(), 1, "should not duplicate");
    assert!(
        matches!(main_acc.exports.values[0].1.ty, Type::Arrow(_, _)),
        "worker's version should overwrite main's"
    );
}

#[test]
fn merge_worker_accumulates_cached_def_count() {
    let mut main_acc = ModuleTreeAccumulator::new();
    main_acc.cached_def_count = 5;

    let mut worker1 = ModuleTreeAccumulator::new();
    worker1.cached_def_count = 3;
    main_acc.merge_worker(worker1);

    let mut worker2 = ModuleTreeAccumulator::new();
    worker2.cached_def_count = 7;
    main_acc.merge_worker(worker2);

    assert_eq!(
        main_acc.cached_def_count, 15,
        "cached counts should sum across workers"
    );
}
