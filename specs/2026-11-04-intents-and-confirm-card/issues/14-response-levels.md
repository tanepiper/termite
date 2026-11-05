# 14 — Confidence and response levels in every output format

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 13
Spec: [../spec.md](../spec.md) (section 2, "Risk, confidence and response levels")

## Details

- `src/response.rs` (library, pure): `Confidence` from the ranking margin and the safety answers; `Response` from risk, confidence and health, per the tables in the spec.
- `Answer` carries its `Response`; `tmite` shows it in all three formats, and `--json` adds `"response"` and `"confidence"`.
- Exit codes follow the table. `run` acts only on Success.
- The model evaluation records confidence, so the bands can be calibrated from real results.

## Tests first

- AC16: the whole table (risk by confidence), and each health rule.
- Property test: adding a health problem or lowering confidence never lowers the response.
