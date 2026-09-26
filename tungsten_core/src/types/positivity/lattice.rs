//! The strictness lattice and the walk mode (ADR 7.8.26e D2).
//!
//! This is **strictness**, not sign polarity. A four-point sign lattice with
//! `Negative ⊗ Negative = Positive` decides *positivity*, which accepts
//! `type Bad3 = Mk((Bad3 -> Nat) -> Nat)` — a functor with no least fixed point
//! in Set. There is deliberately no `⊗` here: passing under an arrow domain
//! sets [`Occ::Forbidden`] / [`Mode::Forbidden`] and never flips back.

/// How a named type's *parameter* is used across all of its constructor fields.
///
/// Ordered by the lattice `Unused ⊑ Strict ⊑ Forbidden`; [`Occ::join`] is the
/// least upper bound, which is what the [`super::fixpoint`] widening uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Occ {
    /// The parameter is discarded — `type Phantom<T> = P(Nat)`.
    ///
    /// An argument passed here is **not walked at any mode**. Collapsing this
    /// into `Strict` is a false *rejection*, not merely imprecision: walking the
    /// discarded argument of `Phantom<Bad -> Nat>` descends into an arrow whose
    /// domain mentions `Bad`, and rejects a type that contains no occurrence of
    /// `Bad` at all.
    Unused,
    /// The parameter only ever occurs strictly positively.
    Strict,
    /// The parameter occurs under an arrow domain (or another forbidden
    /// position) somewhere in the definition — `type Fn1<T> = Mk(T -> Nat)`.
    Forbidden,
}

impl Occ {
    /// Least upper bound under `Unused ⊑ Strict ⊑ Forbidden`.
    #[must_use]
    pub fn join(self, other: Occ) -> Occ {
        if self >= other {
            self
        } else {
            other
        }
    }

    /// Human-readable lattice point, for `doctor check type positivity`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Occ::Unused => "unused",
            Occ::Strict => "strict",
            Occ::Forbidden => "forbidden",
        }
    }
}

/// The mode the occurrence walker carries: "am I under an arrow domain yet?".
///
/// Two-valued and **monotone** — every field's walk starts at [`Mode::Strict`]
/// and can only ever move to [`Mode::Forbidden`]. `Occ::Unused` is never a
/// mode; it inhabits [`Occ`] only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    /// Not (yet) under a forbidden position.
    Strict,
    /// Under an arrow domain, a `Ref`/`Ptr` cell, or an `Eq` witness.
    Forbidden,
}

impl Mode {
    /// The lattice point an occurrence at this mode contributes.
    #[must_use]
    pub fn as_occ(self) -> Occ {
        match self {
            Mode::Strict => Occ::Strict,
            Mode::Forbidden => Occ::Forbidden,
        }
    }

    /// The mode to walk an argument at, given the callee's parameter strictness.
    ///
    /// `None` means "do not walk this argument at all" (the `Unused` case).
    #[must_use]
    pub fn descend(self, param: Occ) -> Option<Mode> {
        match param {
            Occ::Unused => None,
            Occ::Strict => Some(self),
            Occ::Forbidden => Some(Mode::Forbidden),
        }
    }
}
