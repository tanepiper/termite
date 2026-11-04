# 04 — Output guard: output flags, `matches_request`, `manipulated`, calibration

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 03
Spec: [../spec.md](../spec.md) (section 4)

## Details

- `src/guard/output.rs`: output flags from the spec table, computed from the parsed command (reuse the policy's shell-word parser): new network destinations, decode piped into a shell, profile/cron/launchd/`authorized_keys`/sudoers writes, termite's own paths, `sudo` without a `privilege` flag, container escape paths (from ticket 06's environment), and description/command mismatch (a destructive command whose description has none of the destructive words).
- Root detection for the card: `sudo`, `su`, `doas`, `pkexec` as the program in any pipeline segment.
- Two more `noul` questions in the safety request: `matches_request` and `manipulated`, with descriptions for both answers.
- Calibrate on Kev-4B with a table like the existing one (benign, risky, injected, hostile), and choose thresholds; write the table into the tests.
- A `Verdict` gains `Withheld`; `REFUSE_HARMFUL` or a withholding output flag gives it.

## Tests first

- AC5: one command per output flag, plus near-misses.
- Calibration rows as unit tests on parsed answers.
- `run` never auto-runs a command with any output flag (mocked models).
