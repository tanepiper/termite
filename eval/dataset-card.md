---
configs:
  - config_name: observations
    data_files: observations/*.jsonl
---

# termite eval runs

Private. Keep it private: the raw runs at the top level hold withheld commands.

- `*.json`: raw runs, including red and blue team answers. Never publish these.
- `observations/*.jsonl`: one row per test, with no request or command in it.
  Query these to compare models. The fields are in `eval/schema/observation.md`
  in the termite repository.

Written by `scripts/archive-runs.sh`, which stops unless this dataset reports
itself private. Files already here are never replaced, except this card.
