# 03 — `?b` generation: mode, style guide, schema, checks, decision notes, drafts

Status: blocked
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 01, 02
Spec: [../spec.md](../spec.md) (How it works, The bash style guide, Script checks)

> **Superseded in part (2026-11-04):** the intents spec
> (`specs/2026-11-04-intents-and-confirm-card`) replaces `?b` with
> `tmite script` and the management commands with `tmite scripts …`, and
> adds the input and output guards and the provenance log. Rebase this
> ticket on intents tickets 01, 03, 04 and 09 before starting it.

## Details

- `query.rs`: `Mode::Script` for `?b` when followed by whitespace or the end, same rule as `?x`. `?bd …` and `?bash …` are `?` queries.
- `prompts/bash-script.md`: the style guide from the spec, the preamble from `bash/preamble.sh`, one example from `bash/examples/`, and short right/wrong pairs. Included with `include_str!`; the preamble and example are included from their files, not copied, so they cannot drift.
- `src/script.rs`: the `ScriptSource` trait (like `CommandSource`) and its `LlamaServer` implementation, with the JSON schema `{name, summary, script}`; `script_model` and `script_timeout_secs` in `Config`, both optional (default `model`, 300).
- Script checks (spec table), each a separate function with its own error variant. `bash -n` and `shellcheck --severity=warning --shell=bash -` read the script on stdin, with a timeout, from the bash 5.3 found on `PATH`. A missing shellcheck or bash 5.3 stops `?b` with how to install it.
- One retry: if the checks fail, the errors go back to the model once in a follow-up message.
- Decision notes: `harmful` and `no_secrets` from `/v1/systemone`, using the script as the state, shown as notes. They never block or approve.
- The draft is saved with `write_private(drafts, name, script, 0o600)` (ticket 01) after `Home::check()`.
- `answer` gains `Answer::Script { draft, name, summary, notes }` and `Answer::ScriptRejected { errors }`.

## Tests first

- AC1: query table with `?b`, `?b\t…`, `?bd`, `?bash`.
- AC5: one script per check that breaks only that check, each turned down with its own error; one script that passes all of them (the example from `bash/examples/`).
- AC9: with mocked sources, `?b` saves a draft, makes nothing executable, and runs nothing but `bash -n` and shellcheck (check via a fake `PATH` folder whose programs record that they were called).
- Retry: a mock that fails the checks once and then passes gives a draft; one that fails twice gives `ScriptRejected`.
