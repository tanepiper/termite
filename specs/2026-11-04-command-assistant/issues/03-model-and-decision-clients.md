# 03 — `CommandSource` (llama-server) and `DecisionClient` (systemone)

Status: resolved
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: 01
Spec: [../spec.md](../spec.md)

Needs the owner's approval to add `reqwest` (async, `rustls-tls`, `json`; default features off) before work starts. The otty fork pulls it in through `termite`, so it must pass otty's `cargo deny check` there.

## Details

- `config.rs`: read `~/.config/termite/config.json`: `model_url`, `model`, `decision_url`, `generation_timeout_secs` (20), `decision_timeout_secs` (5). The token comes only from `TERMITE_DECISION_TOKEN`.
- `generate.rs`: trait `CommandSource` (one HTTP implementation for `llama-server` `/v1/chat/completions`, with `response_format` set to the spec's JSON schema, 1–5 candidates). The system prompt takes the context: OS, shell, current folder, session kind. Keep it short, because the model is 7–8B.
- `decide.rs`: trait `DecisionClient` with `rank` (`POST /rank`) and `safety` (`POST /safety`), matching the spec's formats. Send `Authorization: Bearer …` only when the token is set.
- Both traits get `#[cfg_attr(test, mockall::automock)]`. They are traits only because they cross the HTTP boundary (spec decision 12).
- Errors are typed: timeout, HTTP status, invalid JSON, unknown verdict or risk.

## Tests first

- Parsing: valid model reply, reply with 0 or 6 candidates (error), unknown risk (error); valid `/rank` and `/safety` replies; `/rank` naming an id that was not sent (error); unknown verdict (error).
- The request body builders produce the spec's JSON exactly (snapshot by `serde_json::json!` comparison).

## Comments

**2026-11-04, Claude Code — done.** Owner approved `reqwest`. 25 unit tests and 6 doc tests pass; fmt and clippy `-D warnings` are clean.

- New files: `src/config.rs`, `src/context.rs`, `src/error.rs`, `src/generate.rs` (`CommandSource`, `LlamaServer`), `src/decide.rs` (`DecisionClient`, `DecisionEndpoint`, `Ranked`, `SafetyVerdict`).
- Dependencies: `reqwest` 0.13.5 with `json` and `rustls` (0.13 renamed `rustls-tls` to `rustls`), default features off; `serde` (derive) and `serde_json` to read replies; `mockall` 0.15 for tests only.
- The traits return `impl Future + Send` rather than using `async fn`, so `otty-assist` can run them with iced's `Task::perform`, which needs `Send` futures. `mockall` mocks them as boxed futures; a test checks a mock can be driven.
- `Context` carries `os`, `shell`, `cwd`, `session` (`Local`/`Ssh`) and `is_idle`. Only the first four are sent to the endpoints.
- `SafetyVerdict::allows_unguarded()` is the endpoint's half of the `?x` gate: `safe_unguarded`, risk `read`, no side effects.
- The ranking is sorted by score, best first, whatever order the endpoint sends. An id that was not sent, or is named twice, is an error. Missing ids are allowed, and those commands are dropped.
- The config file refuses unknown fields, so a token pasted into it (`decision_token`) is an error instead of being silently ignored.
- Not tested live: `llama-server` is installed but was not running, and `~/.config/termite/config.json` does not exist yet. The `tmite` CLI (ticket 05) is the first place to check it end to end.
