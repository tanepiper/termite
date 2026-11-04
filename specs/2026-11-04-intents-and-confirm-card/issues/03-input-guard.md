# 03 — Input guard: layer 1 checks, flags, fixed instructions

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 01
Spec: [../spec.md](../spec.md) (section 3)

## Details

- `src/guard/input.rs`: `check_request(&str) -> Result<Request, RequestRefused>` for the layer 1 table (text, size 500, shape, mixed scripts). Reuse the hidden-character set from `command.rs`; one source of truth for both.
- Mixed scripts: a word with letters from more than one Unicode script (Latin with Cyrillic or Greek, …) is refused. Use a small built-in table of script ranges rather than a new dependency.
- `flags(&Request) -> FlagSet` for the six flags, each from a table of words and patterns, with tests per row.
- `src/guard/instructions.rs`: one fixed instruction per flag and per environment (ticket 06), as `const` text; `system_prompt` appends them last, after anything configurable. No config key can reach them.
- `answer` refuses with the layer 1 reason before any model call, and passes the flags to generation, the decision state and the card.

## Tests first

- AC3: one request per layer 1 check, refused, with no model call (mocked source that fails if called).
- AC4: each flag raised by its examples. Flags only add care, so when in doubt a flag is raised: "list deleted files" may raise `destructive`, and the test pins whichever behaviour is chosen. Plus a snapshot of the system prompt for a request with every flag.
