//! The thresholds and the verdict. Contract: pure code, no I/O. Refusal
//! wins, and running without asking needs all three answers on the safe
//! side.

/// "Only reads" must be at least this likely to run without asking.
pub const MIN_ONLY_READS: f64 = 0.85;

/// "Keeps secrets hidden" must be at least this likely to run without
/// asking.
pub const MIN_NO_SECRETS: f64 = 0.6;

/// "Could be harmful" must be at most this likely to run without asking.
pub const MAX_HARMFUL: f64 = 0.3;

/// "Could be harmful" at or above this refuses the command outright.
pub const REFUSE_HARMFUL: f64 = 0.9;

/// The decision about running a command without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// It may run without asking.
    SafeUnguarded,
    /// The user must confirm it.
    NeedsConfirmation,
    /// It must not be offered to run at all.
    Refuse,
}

/// The decision model's three safety answers, each a probability that the
/// statement is true.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafetyVerdict {
    only_reads: f64,
    no_secrets: f64,
    harmful: f64,
}

impl SafetyVerdict {
    /// How likely the command only reads.
    pub fn only_reads(&self) -> f64 {
        self.only_reads
    }

    /// How likely the command keeps secrets hidden.
    pub fn no_secrets(&self) -> f64 {
        self.no_secrets
    }

    /// How likely the command is harmful.
    pub fn harmful(&self) -> f64 {
        self.harmful
    }

    /// Create a verdict from the three probabilities.
    pub fn new(only_reads: f64, no_secrets: f64, harmful: f64) -> Self {
        Self {
            only_reads,
            no_secrets,
            harmful,
        }
    }

    /// Apply the thresholds. Refusal wins; running without asking needs
    /// all three answers on the safe side.
    pub fn verdict(&self) -> Verdict {
        if self.harmful >= REFUSE_HARMFUL {
            return Verdict::Refuse;
        }

        if self.only_reads >= MIN_ONLY_READS
            && self.no_secrets >= MIN_NO_SECRETS
            && self.harmful <= MAX_HARMFUL
        {
            return Verdict::SafeUnguarded;
        }

        Verdict::NeedsConfirmation
    }

    /// One sentence for the user, naming the answer that decided it.
    pub fn reason(&self) -> String {
        let Self {
            only_reads,
            no_secrets,
            harmful,
        } = *self;

        match self.verdict() {
            Verdict::Refuse => format!(
                "the decision model rates it likely harmful ({harmful:.2})"
            ),
            Verdict::SafeUnguarded => format!(
                "the decision model is confident it only reads ({only_reads:.2})"
            ),
            Verdict::NeedsConfirmation if only_reads < MIN_ONLY_READS => {
                format!(
                    "the decision model is not sure it only reads ({only_reads:.2})"
                )
            },
            Verdict::NeedsConfirmation if no_secrets < MIN_NO_SECRETS => {
                format!(
                    "the decision model thinks it may show secrets ({no_secrets:.2})"
                )
            },
            Verdict::NeedsConfirmation => format!(
                "the decision model rates it possibly harmful ({harmful:.2})"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_HARMFUL, MIN_NO_SECRETS, MIN_ONLY_READS, REFUSE_HARMFUL,
        SafetyVerdict, Verdict,
    };

    const SAFE: (f64, f64, f64) = (1.0, 1.0, 0.0);

    fn verdict((reads, secrets, harmful): (f64, f64, f64)) -> Verdict {
        SafetyVerdict::new(reads, secrets, harmful).verdict()
    }

    #[test]
    fn given_answers_on_each_threshold_then_the_stricter_side_is_kept() {
        let just_under = |limit: f64| limit - 1e-9;
        let just_over = |limit: f64| limit + 1e-9;

        assert_eq!(verdict(SAFE), Verdict::SafeUnguarded);
        assert_eq!(
            verdict((MIN_ONLY_READS, MIN_NO_SECRETS, MAX_HARMFUL)),
            Verdict::SafeUnguarded
        );
        for unclear in [
            (just_under(MIN_ONLY_READS), 1.0, 0.0),
            (1.0, just_under(MIN_NO_SECRETS), 0.0),
            (1.0, 1.0, just_over(MAX_HARMFUL)),
            (1.0, 1.0, just_under(REFUSE_HARMFUL)),
        ] {
            assert_eq!(verdict(unclear), Verdict::NeedsConfirmation);
        }
    }

    /// However sure the other two answers are, harm wins.
    #[test]
    fn given_likely_harm_then_refusal_wins_over_everything() {
        for reads in [0.0, 0.5, 1.0] {
            for secrets in [0.0, 0.5, 1.0] {
                assert_eq!(
                    verdict((reads, secrets, REFUSE_HARMFUL)),
                    Verdict::Refuse
                );
            }
        }
    }

    #[test]
    fn given_not_a_number_then_it_is_never_safe() {
        for odd in [
            (f64::NAN, 1.0, 0.0),
            (1.0, f64::NAN, 0.0),
            (1.0, 1.0, f64::NAN),
        ] {
            assert_ne!(verdict(odd), Verdict::SafeUnguarded);
        }
    }

    #[test]
    fn given_each_outcome_then_the_reason_names_the_deciding_answer() {
        let reason = |(reads, secrets, harmful)| {
            SafetyVerdict::new(reads, secrets, harmful).reason()
        };

        assert!(reason(SAFE).contains("confident"));
        assert!(reason((1.0, 1.0, 0.95)).contains("likely harmful"));
        assert!(reason((0.5, 1.0, 0.0)).contains("only reads"));
        assert!(reason((1.0, 0.2, 0.0)).contains("may show secrets"));
        assert!(reason((1.0, 1.0, 0.5)).contains("possibly harmful"));
    }
}
