//! Typing rules for control flow constructs.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use crate::typecheck::rules::type_of;
use crate::typecheck::TypeResult;
/// Type check early return: `return t : ⊥` where `t` is well-typed
pub(in crate::typecheck) fn type_of_return(ctx: &Context, t: &Term) -> TypeResult<Type> {
    let _ = type_of(ctx, t)?;
    Ok(Type::Void)
}
