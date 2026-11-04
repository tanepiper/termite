//! Turns a query into an answer: generate, check, merge, rank, and for `?x`
//! the safety gate.

use crate::candidate::{Candidate, Risk};
use crate::command::{CommandRejected, check_command};
use crate::context::{Context, Session};
use crate::decide::{DecisionClient, Ranked, Verdict};
use crate::error::AssistError;
use crate::generate::CommandSource;
use crate::merge::merge_candidates;
use crate::policy::read_only;
use crate::query::{Mode, Query};

/// What to show the user for a query.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// `?`: the best command, for the user to confirm.
    Confirm(Ranked),
    /// `??`: every distinct command, best first. May hold only one.
    Choose(Vec<Ranked>),
    /// `?x`: every gate check passed; run it now. `reason` is the decision
    /// model's sentence.
    RunNow {
        /// The command to run.
        ranked: Ranked,
        /// Why it may run without asking.
        reason: String,
    },
    /// `?x` fell back to confirming. `reason` completes "Not run
    /// automatically: …".
    ConfirmWithReason {
        /// The command to confirm.
        ranked: Ranked,
        /// Why it was not run automatically.
        reason: String,
    },
    /// `?x`: the decision model refused the command. It must not be
    /// offered to run.
    Refused {
        /// The refused command, shown for information only.
        ranked: Ranked,
        /// The decision model's reason.
        reason: String,
    },
    /// Every command the model wrote failed the command checks.
    NoUsableCommand {
        /// Each command with the reason it was turned down.
        rejected: Vec<(Candidate, CommandRejected)>,
    },
    /// The model or the ranking could not be reached or understood.
    Failed(AssistError),
}

/// Answer a query: ask `source` for commands, drop the ones that fail the
/// command checks, merge duplicates, ask `decider` to rank them, then act
/// on the mode. Nothing here fails open: for `?x`, any doubt or error
/// falls back to [`Answer::ConfirmWithReason`].
pub async fn answer<S, D>(
    query: &Query,
    context: &Context,
    source: &S,
    decider: &D,
) -> Answer
where
    S: CommandSource,
    D: DecisionClient,
{
    let request = query.request();

    let generated = match source.generate(request, context).await {
        Ok(generated) => generated,
        Err(error) => return Answer::Failed(error),
    };

    let (usable, rejected) = split_usable(generated);
    if usable.is_empty() {
        return Answer::NoUsableCommand { rejected };
    }
    let candidates = merge_candidates(usable);

    let mut ranked = match decider.rank(request, context, &candidates).await {
        Ok(ranked) if !ranked.is_empty() => ranked,
        Ok(_) => return Answer::Failed(AssistError::EmptyRanking),
        Err(error) => return Answer::Failed(error),
    };

    match query.mode() {
        Mode::Confirm => Answer::Confirm(ranked.swap_remove(0)),
        Mode::Choose => Answer::Choose(ranked),
        Mode::RunIfSafe => {
            run_if_safe(request, context, decider, ranked.swap_remove(0)).await
        },
    }
}

/// The `?x` gate. The local checks run first, so a command that fails one
/// never reaches the network. Each check can only make the answer
/// stricter.
async fn run_if_safe<D>(
    request: &str,
    context: &Context,
    decider: &D,
    best: Ranked,
) -> Answer
where
    D: DecisionClient,
{
    if let Some(reason) = local_gate(context, best.candidate()) {
        return Answer::ConfirmWithReason {
            ranked: best,
            reason,
        };
    }

    let verdict = decider
        .safety(request, context, best.candidate().command())
        .await;

    match verdict {
        Ok(verdict) => {
            let reason = verdict.reason();
            match verdict.verdict() {
                Verdict::SafeUnguarded => Answer::RunNow {
                    ranked: best,
                    reason,
                },
                Verdict::Refuse => Answer::Refused {
                    ranked: best,
                    reason,
                },
                Verdict::NeedsConfirmation => Answer::ConfirmWithReason {
                    ranked: best,
                    reason,
                },
            }
        },
        Err(error) => Answer::ConfirmWithReason {
            ranked: best,
            reason: format!("the safety check failed: {error}"),
        },
    }
}

/// The gate checks that need no network: the model's own risk label, the
/// read-only policy, a local session and an idle shell. The command checks
/// already ran in [`split_usable`].
fn local_gate(context: &Context, candidate: &Candidate) -> Option<String> {
    if candidate.risk() != Risk::Read {
        let risk = candidate.risk().as_str();
        return Some(format!("the model labelled it `{risk}`"));
    }

    if let Err(refusal) = read_only(candidate.command()) {
        return Some(refusal.to_string());
    }

    if context.session() != Session::Local {
        return Some("it runs over SSH".to_owned());
    }

    if !context.is_idle() {
        return Some("a command is still running".to_owned());
    }

    None
}

/// Keep the commands that pass the command checks, in order, and the
/// others with their reasons.
fn split_usable(
    generated: Vec<Candidate>,
) -> (Vec<Candidate>, Vec<(Candidate, CommandRejected)>) {
    let mut usable = Vec::new();
    let mut rejected = Vec::new();

    for candidate in generated {
        match check_command(candidate.command()) {
            Ok(()) => usable.push(candidate),
            Err(reason) => rejected.push((candidate, reason)),
        }
    }

    (usable, rejected)
}

#[cfg(test)]
mod tests;
