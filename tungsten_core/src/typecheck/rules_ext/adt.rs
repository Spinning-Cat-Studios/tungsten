//! Typing rules for flat ADT construction and matching.

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

use crate::typecheck::error::TypeError;
use crate::typecheck::rules::{check_type_wf, type_of, types_equal};
use crate::typecheck::TypeResult;
/// Type check ADT construction: `adt_construct [adt_ty] idx payload : adt_ty`
pub(in crate::typecheck) fn type_of_adt_construct(
    ctx: &Context,
    adt_ty: &Type,
    idx: usize,
    payload: &Term,
) -> TypeResult<Type> {
    check_type_wf(ctx, adt_ty)?;
    match adt_ty {
        Type::Adt(_name, _type_args, variants) => {
            if idx >= variants.len() {
                return Err(TypeError::InvalidVariantIndex {
                    index: idx,
                    num_variants: variants.len(),
                });
            }
            let (_ctor_name, expected_payload_ty) = &variants[idx];
            let payload_ty = type_of(ctx, payload)?;
            if !types_equal(&payload_ty, expected_payload_ty) {
                return Err(TypeError::TypeMismatch {
                    expected: expected_payload_ty.clone(),
                    got: payload_ty,
                });
            }
            Ok(adt_ty.clone())
        }
        _ => Err(TypeError::NotAnAdt {
            got: adt_ty.clone(),
        }),
    }
}

/// Type check ADT match: `adt_match scrut arms : τ`
pub(in crate::typecheck) fn type_of_adt_match(
    ctx: &Context,
    scrut: &Term,
    arms: &[(usize, String, Box<Term>)],
) -> TypeResult<Type> {
    let scrut_ty = type_of(ctx, scrut)?;
    let Type::Adt(_name, _type_args, variants) = &scrut_ty else {
        return Err(TypeError::NotAnAdt { got: scrut_ty });
    };

    if arms.is_empty() {
        return Err(TypeError::EmptyMatch);
    }

    let mut result_ty: Option<Type> = None;

    for (idx, var, body) in arms {
        if *idx >= variants.len() {
            return Err(TypeError::InvalidVariantIndex {
                index: *idx,
                num_variants: variants.len(),
            });
        }
        let (_ctor_name, payload_ty) = &variants[*idx];

        // Bind the payload variable and typecheck body
        let arm_ctx = ctx.with_term(var, payload_ty.clone());
        let body_ty = type_of(&arm_ctx, body)?;

        match &result_ty {
            None => result_ty = Some(body_ty),
            Some(expected) if !types_equal(&body_ty, expected) => {
                return Err(TypeError::BranchTypeMismatch {
                    then_type: expected.clone(),
                    else_type: body_ty,
                });
            }
            Some(_) => {} // types match, continue
        }
    }

    Ok(result_ty.unwrap())
}
