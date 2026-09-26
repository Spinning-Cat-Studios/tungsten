//! Wall-clock-bounded evaluation (ADR 21.7.26f / D1).
//!
//! `eval_with_env` runs until the term reaches a value or gets stuck, which for
//! a non-terminating body means "never". This module adds an entry that stops
//! at a deadline instead, so a caller that cannot tolerate a hang — the test
//! runner — gets a reportable failure rather than a wedged process.
//!
//! Split from `mod.rs` to keep that file under the size limit.

use std::time::Instant;

use super::stopped::EvalStopped;
use super::{finished_or_black_hole, step_with_env, EvalEnv};
use crate::eval::StepResult;
use crate::terms::Term;

/// How many steps elapse between wall-clock checks.
///
/// The deadline is polled coarsely so the hot stepping loop stays free of a
/// syscall per reduction. The trade-off is overshoot: the bound is honoured no
/// earlier than the next interval boundary, so the actual stop time is the
/// deadline plus up to `DEADLINE_CHECK_INTERVAL` steps' work. That is
/// negligible for cheap steps (measured ~1M steps/s, so ~0.1 s) but is **not**
/// bounded in general — one step over a large term can take arbitrarily long.
/// The guarantee is therefore "terminates", not "terminates within ε of the
/// deadline"; converting a hang into a failure is what the bound is for.
pub(super) const DEADLINE_CHECK_INTERVAL: u64 = 100_000;

/// Evaluate with environment under a wall-clock deadline (ADR 21.7.26f / D1).
///
/// Unlike `eval_with_env_and_limit`, the bound a caller actually wants to
/// express is *time*, and the step count comes back on trip for reporting.
/// `Err(EvalStopped::TimedOut)` means the deadline passed before the term
/// reached a value or got stuck; `Err(EvalStopped::BlackHole)` means a global
/// re-entered its own forcing (ADR 22.7.26a) — a shape the deadline could
/// never catch, since pre-detection it exhausted the stack in milliseconds.
/// Below the deadline the result is identical to `eval_with_env` — this
/// bounds evaluation, it does not change its semantics.
///
/// The deadline is observed *between* steps, so a non-terminating reduction is
/// caught but a single step that never returns is not. The entry `strip_spans`
/// is likewise outside the bound.
pub fn eval_with_env_until(
    term: &Term,
    env: &EvalEnv,
    deadline: Instant,
) -> Result<Term, EvalStopped> {
    let mut current = term.strip_spans();
    let mut steps: u64 = 0;
    loop {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => {
                return finished_or_black_hole(current, env);
            }
        }
        steps += 1;
        if steps.is_multiple_of(DEADLINE_CHECK_INTERVAL) && Instant::now() >= deadline {
            return Err(EvalStopped::TimedOut { steps });
        }
    }
}
