//! Phase 3: Arm Classification
//!
//! Groups match arms by constructor index vs catch-all patterns.

use std::collections::HashMap;

use crate::ast::{self, Pattern};
use crate::span::{Span, Spanned};

use super::context::ClassifiedArms;
use crate::elaborate::env::{self as elab_env, ModulePath, PathResolutionError};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Classify match arms by constructor index vs catch-all.
    ///
    /// Builds a map from constructor index to arm, and identifies the catch-all
    /// arm if present. Also emits warnings for unreachable arms.
    pub(super) fn classify_match_arms<'b>(
        &mut self,
        arms: &'b [ast::MatchArm],
        constructors: &[elab_env::Constructor],
        _span: Span,
    ) -> ElabResult<ClassifiedArms<'b>> {
        let mut ctor_arms: HashMap<usize, Vec<&ast::MatchArm>> = HashMap::new();
        let mut catch_all_arm: Option<&ast::MatchArm> = None;
        let mut catch_all_span: Option<Span> = None;

        for arm in arms.iter() {
            match &arm.pattern {
                Pattern::Constructor(ref path, _, _) => {
                    self.classify_constructor_arm(arm, path, &mut ctor_arms, catch_all_span)?;
                }
                Pattern::Wildcard(s) | Pattern::Var(ast::Ident { span: s, .. }) => {
                    self.classify_catch_all_arm(arm, *s, &mut catch_all_arm, &mut catch_all_span);
                }
                _ => {
                    return Err(ElabError::new(
                        arm.pattern.span(),
                        ElabErrorKind::Other("unsupported pattern in match arm".to_string()),
                    ));
                }
            }
        }

        // Suppress unused warning - constructors is used for context
        let _ = constructors;

        Ok(ClassifiedArms {
            ctor_arms,
            catch_all: catch_all_arm,
        })
    }

    /// Classify a constructor pattern arm.
    fn classify_constructor_arm<'b>(
        &mut self,
        arm: &'b ast::MatchArm,
        path: &ast::Path,
        ctor_arms: &mut HashMap<usize, Vec<&'b ast::MatchArm>>,
        catch_all_span: Option<Span>,
    ) -> ElabResult<()> {
        let name = path.item_name();

        // Warn if this arm is unreachable due to preceding catch-all
        if let Some(catch_span) = catch_all_span {
            self.warn(
                ElabError::new(arm.pattern.span(), ElabErrorKind::UnreachableArm)
                    .with_span_note(catch_span, "this catch-all pattern matches all values")
                    .with_help("remove this arm or move the catch-all pattern to the end"),
            );
            return Ok(());
        }

        // Check module visibility for qualified paths
        if !path.is_simple() {
            let module_path = ModulePath::new(
                path.module_segments()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect(),
            );
            if !self
                .env
                .is_module_accessible(&module_path, &self.current_module, true)
            {
                return Err(ElabError::private_module(
                    path.span,
                    module_path.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }

        // Resolve constructor and add to map (multiple arms may exist for same constructor)
        let info = match self
            .env
            .resolve_constructor_path(path, &self.current_module)
        {
            Ok(Some(info)) => info.clone(),
            Ok(None) => return Err(self.undefined_constructor_error(name.span, &name.name)),
            Err(PathResolutionError::ModuleNotFound(module)) => {
                return Err(ElabError::module_not_found(path.span, module.to_string()));
            }
            Err(PathResolutionError::ItemNotFound { module, item }) => {
                return Err(ElabError::item_not_in_module(
                    path.span,
                    module.to_string(),
                    item,
                ));
            }
        };

        ctor_arms.entry(info.index).or_default().push(arm);
        Ok(())
    }

    /// Classify a catch-all (wildcard or variable) arm.
    fn classify_catch_all_arm<'b>(
        &mut self,
        arm: &'b ast::MatchArm,
        pattern_span: Span,
        catch_all_arm: &mut Option<&'b ast::MatchArm>,
        catch_all_span: &mut Option<Span>,
    ) {
        if catch_all_arm.is_some() {
            // Multiple catch-alls - warn about unreachable
            self.warn(
                ElabError::new(arm.pattern.span(), ElabErrorKind::UnreachableArm)
                    .with_span_note(
                        catch_all_span.unwrap(),
                        "previous catch-all pattern already matches all values",
                    )
                    .with_help("remove this redundant catch-all pattern"),
            );
            return;
        }

        *catch_all_arm = Some(arm);
        *catch_all_span = Some(pattern_span);
    }
}
