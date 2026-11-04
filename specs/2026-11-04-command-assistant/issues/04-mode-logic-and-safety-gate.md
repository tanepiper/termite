# 04 — Mode logic and the `?x` safety gate

Status: resolved
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 02, 03
Spec: [../spec.md](../spec.md)

## Details

- `assist.rs`: `async fn answer(query: &Query, context: &Context, source: &impl CommandSource, decider: &impl DecisionClient) -> Answer`.
- Flow for every mode: generate → command checks → merge → rank. If nothing survives the checks, return `Answer::NoUsableCommand`.
- `Answer` variants: `Confirm(Ranked)` for `?`; `Choose(Vec<Ranked>)` for `??` (best first, may hold one); `RunNow { ranked, reason }`; `ConfirmWithReason { ranked, reason }` (the `?x` fallback); `Refused { ranked, reason }`; `Failed(AssistError)`.
- The `?x` gate, in this order, stopping at the first failure: command checks, model risk is `read`, `policy::read_only`, the session is local, the shell is idle, then `/safety` returns `safe_unguarded` with `risk: read` and empty `side_effects`. The cheap local checks run before the network call. `refuse` gives `Refused`.
- `Context` carries `is_local` and `is_idle`. The CLI always sets both to true; the otty widget fills them in.
- Fail closed: any error inside the `?x` gate gives `ConfirmWithReason`; errors in `?`/`??` give `Failed`.

## Tests first (mockall)

- AC5: `?x` gives `RunNow` only when every check passes. One test per failing check alone, plus `/safety` timeout, `/safety` invalid JSON, `/rank` error, and `refuse`.
- AC6: `??` with two candidates that merge into one gives `Choose` with one item.
- All candidates fail the checks gives `NoUsableCommand`.

## Comments

**2026-11-04, Claude Code — done.** `src/answer.rs` (`answer`, `Answer`) with mock-based tests in `src/answer/tests.rs`. 36 unit tests and 6 doc tests pass; fmt and clippy `-D warnings` are clean.

- `Answer::NoUsableCommand` carries each turned-down command with its `CommandRejected` reason, so the front ends can show why.
- The gate order in code is: model risk label, read-only policy, local session, idle shell, then `/safety`. Tests check that `/safety` is never called when a local check fails, so a doubtful command never reaches the network.
- The fallback reasons complete "Not run automatically: …". For example, "the model labelled it `write`", "`rm` is not on the read-only list", "it runs over SSH", "a command is still running", "the decision endpoint expects side effects: …", and "the safety check failed: …". A `needs_confirmation` verdict shows the endpoint's own sentence.
- An empty ranking from any `DecisionClient` gives `Failed(EmptyRanking)` instead of a panic.
- `Ranked::new` exists only in tests. The shared `ready` helper (polls a future that is ready at once) moved to `src/test_support.rs`.
- A test confirms that `answer` with the real `LlamaServer` and `DecisionEndpoint` is a `Send` future, which `otty-assist` needs for `Task::perform`.
