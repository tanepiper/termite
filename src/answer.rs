//! Turns a query into an answer: generate, check, merge, rank, and for `?x`
//! the safety gate.

use crate::candidate::{Candidate, Risk};
use crate::command::{CommandRejected, check_command};
use crate::context::Context;
use crate::decide::{DecisionClient, Ranked, Verdict};
use crate::error::AssistError;
use crate::generate::CommandSource;
use crate::hazard::{Severity, hazards};
use crate::merge::merge_candidates;
use crate::policy::read_only_in;
use crate::progress::{Progress, Stage};
use crate::query::{Mode, Query};

/// What to show the user for a query.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// `?`: the best command, for the user to confirm.
    Confirm(Ranked),
    /// `??`: every distinct command, best first. May hold only one.
    Choose(Vec<Ranked>),
    /// `?x`: every gate check passed; it goes straight to the prompt.
    /// `reason` is the decision model's sentence.
    StraightIn {
        /// The command for the prompt.
        ranked: Ranked,
        /// Why it needs no question first.
        reason: String,
    },
    /// `?x` fell back to asking. `reason` completes "Needs approval: …".
    ConfirmWithReason {
        /// The command to confirm.
        ranked: Ranked,
        /// Why it did not go straight to the prompt.
        reason: String,
    },
    /// `?x`: the decision model refused the command. It must not reach the
    /// prompt.
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
    /// The model said it will not do this. There is no command to use.
    Declined {
        /// The model's own words. Treat as untrusted text.
        reason: String,
    },
    /// The model or the ranking could not be reached or understood.
    Failed(AssistError),
}

/// Answer a query: ask `source` for commands, drop the ones that fail the
/// command checks, merge duplicates, ask `decider` to rank them, then act
/// on the mode. Nothing here fails open: for `?x`, any doubt or error
/// falls back to [`Answer::ConfirmWithReason`]. Nothing here runs a command:
/// the most any answer does is reach the prompt, and only the user presses
/// Enter.
pub async fn answer<S, D>(
    query: &Query,
    context: &Context,
    source: &S,
    decider: &D,
    progress: &dyn Progress,
) -> Answer
where
    S: CommandSource,
    D: DecisionClient,
{
    let request = query.request();

    progress.stage(Stage::Asking);
    let generated = match source.generate(request, context, progress).await {
        Ok(generated) => generated,
        Err(AssistError::ModelDeclined { reason }) => {
            return Answer::Declined { reason };
        },
        Err(error) => return Answer::Failed(error),
    };

    let (usable, rejected) = split_usable(generated);
    if usable.is_empty() {
        return Answer::NoUsableCommand { rejected };
    }
    let candidates = merge_candidates(usable);

    progress.stage(Stage::Ranking);
    let mut ranked = match decider.rank(request, context, &candidates).await {
        Ok(ranked) if !ranked.is_empty() => ranked,
        Ok(_) => return Answer::Failed(AssistError::EmptyRanking),
        Err(error) => return Answer::Failed(error),
    };

    match query.mode() {
        Mode::Confirm => Answer::Confirm(ranked.swap_remove(0)),
        Mode::Choose => Answer::Choose(ranked),
        Mode::StraightIn => {
            progress.stage(Stage::Checking);
            straight_in(request, context, decider, ranked.swap_remove(0)).await
        },
    }
}

/// The `?x` gate. The local checks run first, so a command that fails one
/// never reaches the network. Each check can only make the answer
/// stricter.
async fn straight_in<D>(
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
                Verdict::SafeUnguarded => Answer::StraightIn {
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
/// hazard signatures and the read-only policy (including the repository's
/// git config and the folder `rg` starts from). The command checks already
/// ran in [`split_usable`].
fn local_gate(context: &Context, candidate: &Candidate) -> Option<String> {
    if candidate.risk() != Risk::Read {
        let risk = candidate.risk().as_str();
        return Some(format!("the model labelled it `{risk}`"));
    }

    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if let Err(refusal) = read_only_in(
        candidate.command(),
        std::path::Path::new(context.cwd()),
        home.as_deref(),
    ) {
        return Some(refusal.to_string());
    }

    let hazards = hazards(candidate.command());
    hazards
        .iter()
        .find(|hazard| hazard.severity >= Severity::Medium)
        .map(|hazard| format!("it looks risky ({})", hazard.category))
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
