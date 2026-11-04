# 08 — Fixed rules: invariant tests, property tests, "only tightens" config test

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 05, 06, 07
Spec: [../spec.md](../spec.md) (section 7)

## Details

- `proptest` as a dev-dependency (owner approved, Round 2).
- Property tests: arbitrary strings into `check_request`, `check_command`, `read_only`, `merge_candidates` and the output flags never panic; any string with a control or hidden character is refused by `check_request` and `check_command`; anything `read_only` accepts has no `;`, `&&`, `||`, `|` into a shell, redirection, `$( )`, backticks or process substitution after parsing; `to_printable` output never contains control characters.
- Invariant tests, one per fixed rule in the spec, through `answer` with hostile mock models: no root command is ever inserted, copied or run; destructive/persistence/`self` are display only; withheld commands never appear in any output format; flags persist through the loop; fixed instructions are always last; no check can be skipped.
- AC12: a test enumerates every config key (from the struct) and asserts each one can only tighten.

## Tests first

- The tests above are the ticket.
