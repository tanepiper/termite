# 09 — Provenance log, file checks and alerts

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (section 7)

## Details

- `src/provenance.rs`: `provenance.jsonl` (0600, written through `write_private` append semantics: read, add one line, write a new file, rename), entries per the spec, `prev` chaining, HMAC-SHA256 per entry.
- Key: macOS Keychain via `security add-generic-password` / `find-generic-password` (service `termite-provenance`), Linux Secret Service via `secret-tool` when present, else `$TERMITE_HOME/provenance.key` (0600) with a warning. Tests use an injected key.
- New dependencies to ask the owner about before adding: `sha2` and `hmac` (RustCrypto). No other crypto.
- File checks before trusting the library, a script or the config: unchanged / changed (show a diff against the last draft copy if one is kept, highlight flagged lines, ask; accept logs `changed-accepted`) / unknown file in `bin/` (reported, never run).
- Alerts: `alert` entries (request hash, flags, verdict; no request text); `tmite alerts`; `tmite provenance <path>` and `tmite provenance verify`.

## Tests first

- AC10: append and verify; removing, editing or reordering an entry breaks verification at that entry; a wrong MAC is caught; a changed file is detected; an unknown file in `bin/` is reported.
- No request text appears in the log (property test over random requests).
