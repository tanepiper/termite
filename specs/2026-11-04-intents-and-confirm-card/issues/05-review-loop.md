# 05 — The review loop between the two models

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 04
Spec: [../spec.md](../spec.md) (section 4)

## Details

- When the request has a flag, or the reply has an output flag or a doubtful decision score, send the decision model's concerns and the fixed instructions back to the generating model, asking for a safer command for the same task or none.
- Each new command goes through every check from the start.
- `review_rounds` in the config: optional, default 2, accepted 0–3; anything above 3 is a config error. The loop also stops at the generation time limit.
- Verdicts only tighten across rounds: flags are a union over the query; once withheld or display-only, never insert.

## Tests first

- AC7 with scripted mock models: a loop that converges to a safe command; one that never does (ends after `review_rounds`); one where a later round is more dangerous (stays at the stricter verdict); `review_rounds = 4` refused by config loading; `0` disables the loop.
