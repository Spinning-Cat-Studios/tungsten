//! Native and ADT term constructors.
//!
//! Constructors for nat literals, extended string ops, extern calls, refs, and ADT.
//! String operations and recursive types are in `constructors_extended.rs`.
//! Arithmetic and boolean operations are in `constructors_arith.rs`.

use crate::types::Type;

use super::Term;

impl Term {
    /// Create a natural number literal (unary encoding: Succ^n(Zero))
    ///
    /// Warning: This builds n nested Succ terms. For large n, use `nat_lit` instead.
    #[must_use]
    pub fn nat(n: u64) -> Term {
        let mut term = Term::Zero;
        for _ in 0..n {
            term = Term::succ(term);
        }
        term
    }

    /// Create a natural number literal (efficient representation)
    ///
    /// Uses the `NatLit` variant which stores the value directly as u64,
    /// avoiding stack overflow for large numbers.
    #[must_use]
    pub fn nat_lit(n: u64) -> Term {
        Term::NatLit(n)
    }

    /// Create a natural number with automatic representation choice
    ///
    /// Uses unary encoding for small numbers (≤ threshold) for proof compatibility,
    /// and `NatLit` for large numbers to avoid stack overflow.
    #[must_use]
    pub fn nat_smart(n: u64) -> Term {
        const UNARY_THRESHOLD: u64 = 1000;
        if n <= UNARY_THRESHOLD {
            Term::nat(n)
        } else {
            Term::NatLit(n)
        }
    }

    // === Extended String Ops ===

    /// Create character at index: `char_at` s n
    #[must_use]
    pub fn str_char_at(s: Term, n: Term) -> Term {
        Term::StrCharAt(Box::new(s), Box::new(n))
    }

    /// Create substring: substring s start len
    #[must_use]
    pub fn str_substring(s: Term, start: Term, len: Term) -> Term {
        Term::StrSubstring(Box::new(s), Box::new(start), Box::new(len))
    }

    /// Create extern call: `extern_call` "symbol" [args]
    pub fn extern_call(symbol: impl Into<String>, args: Vec<Term>) -> Term {
        Term::ExternCall(symbol.into(), args)
    }

    /// Create a new ref cell: ref v
    #[must_use]
    pub fn ref_new(v: Term) -> Term {
        Term::RefNew(Box::new(v))
    }

    /// Read from ref: get r
    #[must_use]
    pub fn ref_get(r: Term) -> Term {
        Term::RefGet(Box::new(r))
    }

    /// Write to ref: set r v
    #[must_use]
    pub fn ref_set(r: Term, v: Term) -> Term {
        Term::RefSet(Box::new(r), Box::new(v))
    }

    // === Phase 2B: Flat ADT Constructors (ADR 2.2.26) ===

    /// Create an ADT constructor
    ///
    /// # Arguments
    /// - `adt_ty`: The full ADT type
    /// - `variant_idx`: Index of the constructor (0-based)
    /// - `payload`: The payload term
    #[must_use]
    pub fn adt_construct(adt_ty: Type, variant_idx: usize, payload: Term) -> Term {
        Term::AdtConstruct(adt_ty, variant_idx, Box::new(payload))
    }

    /// Create an ADT match expression
    ///
    /// # Arguments
    /// - `scrutinee`: Term to match on
    /// - `arms`: Vec of (`variant_idx`, `bound_var`, body)
    #[must_use]
    pub fn adt_match(scrutinee: Term, arms: Vec<(usize, String, Box<Term>)>) -> Term {
        Term::AdtMatch(Box::new(scrutinee), arms)
    }

    /// Helper to build ADT match arms
    ///
    /// # Arguments
    /// - `variant_idx`: Index of the constructor
    /// - `var`: Variable name to bind payload
    /// - `body`: Body term for this arm
    pub fn adt_arm(
        variant_idx: usize,
        var: impl Into<String>,
        body: Term,
    ) -> (usize, String, Box<Term>) {
        (variant_idx, var.into(), Box::new(body))
    }
}
