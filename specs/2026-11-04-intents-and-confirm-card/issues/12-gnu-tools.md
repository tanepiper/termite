# 12 — GNU tools in setup, `gnu_cmd` in the library

Status: ready-for-agent
Type: task
Model: Sonnet 5.5 (claude-sonnet-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (section 9)

## Details

- `tmite setup` (macOS): check `coreutils`, `findutils`, `gnu-sed`, `grep`, `gawk`, `gnu-tar` (by their `g`-prefixed programs); offer one `brew install` for the missing ones, on `y` only. Never change `PATH` order.
- The generation context lists the installed GNU programs by name.
- `bash/lib/termite/gnu.sh`: `gnu_cmd NAME` prints `gNAME` on macOS when installed, `NAME` on Linux, and fails with a hint otherwise; library `VERSION` goes to 2 (additions only).

## Tests first

- bats tests for `gnu_cmd` with fake `PATH` folders; setup's detection with a fake `PATH`.
