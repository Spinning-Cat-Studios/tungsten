//! Address-tracking primitives shared by the two audit arms (the `$direct_mt`
//! callee arm and the `$direct` shim arm, ADR 17.7.26e §2.2).

use std::collections::HashSet;

use crate::doctor::checks::ir_checks::textparse::split_top_level_commas;

/// Track the transitive derived-address set of a set of root pointers:
/// `%d = getelementptr …, ptr %p, …` / `bitcast ptr %p` / `addrspacecast ptr %p`
/// with `%p` tracked adds `%d` (fixpoint over the body).
pub(super) fn track_derived_addresses(body: &[&str], roots: &HashSet<String>) -> HashSet<String> {
    let mut tracked = roots.clone();
    loop {
        let mut changed = false;
        for line in body {
            let t = line.trim();
            let Some(eq) = t.find(" = ") else { continue };
            let (dest, rhs) = (t[..eq].trim(), &t[eq + 3..]);
            if derives_address(rhs)
                && !tracked.contains(dest)
                && ssa_tokens(rhs).iter().any(|tok| tracked.contains(tok))
            {
                tracked.insert(dest.to_string());
                changed = true;
            }
        }
        if !changed {
            return tracked;
        }
    }
}

/// True when an instruction's right-hand side derives a new address from an
/// existing one (rather than producing an unrelated value).
pub(super) fn derives_address(rhs: &str) -> bool {
    rhs.starts_with("getelementptr")
        || rhs.starts_with("bitcast")
        || rhs.starts_with("addrspacecast")
}

/// Extract the SSA-value names (`%x`) appearing in a call's argument list.
/// Token-based, so it tolerates parameter attributes (`nonnull align 8
/// dereferenceable(24)`) and comma-carrying type attributes (`sret({ i64, i64 })`)
/// between `ptr` and the name. Skips `ptr null`. Non-pointer SSA args are also
/// collected — harmless for both consumers (the alloca set contains only
/// pointer-producing names; the tracked set contains only buffer addresses).
pub(super) fn ptr_call_args(call: &str) -> Vec<String> {
    let Some(list) = call_arg_list(call) else {
        return Vec::new();
    };
    ssa_tokens(list)
}

/// The SSA names passed at the **buffer slots** of a call — the `ptr` arguments
/// carrying the canonical sret / indirect-param attributes (`sret(…)` /
/// `dereferenceable(…)`, ADR 1.7.26e §2.1 P6), in slot order.
///
/// These are exactly the slots whose `noalias` contract the tail-edge
/// distinctness assert (I5) protects. Flat `ptr` args (a recursive-ADT value
/// such as a `List`) carry no such attribute and are deliberately excluded:
/// passing the same list twice is legal and aliases nothing the ABI promises.
pub(super) fn buffer_slot_args(call: &str) -> Vec<String> {
    let Some(list) = call_arg_list(call) else {
        return Vec::new();
    };
    split_top_level_commas(list)
        .into_iter()
        .filter_map(|part| {
            let p = part.trim();
            let is_buffer_slot =
                p.starts_with("ptr ") && (p.contains("sret(") || p.contains("dereferenceable("));
            is_buffer_slot.then(|| trailing_ssa_name(p)).flatten()
        })
        .collect()
}

/// The `ptr` operand of a `load` instruction (`… = load %T, ptr %p, align N`).
pub(super) fn load_pointer_operand(instr: &str) -> Option<String> {
    let rhs = instr.split_once(" = ")?.1;
    let args = rhs.strip_prefix("load ")?;
    split_top_level_commas(args)
        .into_iter()
        .map(str::trim)
        .find(|p| p.starts_with("ptr ") || *p == "ptr")
        .and_then(trailing_ssa_name)
}

/// The value operand of a `store` (`store %T <val>, ptr <dest>, align N`) —
/// what is written, as opposed to where it is written.
pub(super) fn store_value_operand(instr: &str) -> Option<&str> {
    let rest = instr.strip_prefix("store ")?;
    split_top_level_commas(rest).into_iter().next()
}

/// The trailing `%name` of a parameter/argument entry, skipping any leading
/// type + attribute text (`ptr nonnull align 8 dereferenceable(24) %1` → `%1`).
/// `None` for a literal operand (`ptr null`).
pub(super) fn trailing_ssa_name(entry: &str) -> Option<String> {
    entry
        .trim()
        .rsplit(' ')
        .next()
        .filter(|n| n.starts_with('%'))
        .map(str::to_string)
}

/// True when the instruction is a call to a debug/memory intrinsic that is a
/// permitted use of a buffer address: it neither retains the pointer nor opens
/// a second access route that outlives the instruction.
pub(super) fn is_permitted_intrinsic(instr: &str) -> bool {
    ["llvm.memcpy", "llvm.memmove", "llvm.lifetime.", "llvm.dbg."]
        .iter()
        .any(|i| instr.contains(i))
}

/// True when the instruction calls a `$direct_mt` internal entry — the one call
/// a shim buffer pointer may legitimately be handed to (the callee reads and
/// writes it through the param only; I2/I3 audit that side).
pub(super) fn is_direct_mt_call(instr: &str) -> bool {
    instr.contains("call") && (instr.contains("$direct_mt\"(") || instr.contains("$direct_mt("))
}

/// True when a call site carries a canonical buffer-slot attribute — the
/// name-independent signal that this call speaks the Class-P indirect ABI.
pub(super) fn call_has_buffer_slot(instr: &str) -> bool {
    instr.contains("sret(") || instr.contains("dereferenceable(")
}

/// All `%`-prefixed SSA tokens in an instruction fragment.
pub(super) fn ssa_tokens(fragment: &str) -> Vec<String> {
    fragment
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']' | '{' | '}'))
        .filter(|tok| tok.starts_with('%'))
        .map(str::to_string)
        .collect()
}

/// True when any SSA token of the fragment is in `tracked`.
pub(super) fn mentions_tracked(fragment: &str, tracked: &HashSet<String>) -> bool {
    ssa_tokens(fragment).iter().any(|tok| tracked.contains(tok))
}

/// The argument list of a call instruction (text between the first `(` and the
/// last `)`).
fn call_arg_list(call: &str) -> Option<&str> {
    let (open, close) = (call.find('(')?, call.rfind(')')?);
    (open < close).then(|| &call[open + 1..close])
}
