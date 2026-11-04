# 01 — `TERMITE_HOME` layout, folder checks, atomic owner-only writes, name rules

Status: resolved
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (decisions 2, 3, 5)

## Details

- New library module `src/home.rs`: `Home::from_env()` resolves `TERMITE_HOME`, else `$HOME/.config/termite`; gives the paths of `config.json`, `bin/`, `lib/termite/` and `drafts/`. `Config::path()` uses it, so `TERMITE_HOME` also moves the config.
- `Home::check()`: from `TERMITE_HOME` down to each subfolder, each must be a real folder (`symlink_metadata`, not a symlink), owned by the current uid, and not group- or other-writable. Errors name the folder and the problem. Never `chmod`s anything.
- `Home::ensure()`: creates missing folders with mode 0700 (`DirBuilder::mode`, then a check, because umask can only remove bits).
- `write_private(folder, name, bytes, mode)`: write to `.name.tmp-<random>` opened with `O_CREAT | O_EXCL | O_NOFOLLOW` and the final mode (0600 or 0700), set the mode again with `fchmod` (umask), `sync_all`, then `rename` into place. On any error, remove the temporary file. Refuses if the target exists and is a symlink.
- `ScriptName::parse`: 2–40 chars, `[a-z][a-z0-9-]*`, no trailing or doubled `-`. `ScriptName::check_free(shell)` turns it down if it is a bash/zsh keyword or builtin (fixed list), or a program found on `PATH` other than in `bin/`. Aliases and functions are checked by the zsh widget (ticket 04), which can see them.
- Uses `libc` only if `std::os::unix` cannot do it (`O_NOFOLLOW` is `custom_flags(libc::O_NOFOLLOW)`). Adding `libc` needs the owner's approval: ask first, or use the constant values from `std` where available.

## Tests first

- AC2: in a temp home, folders and files have exactly 0700/0600/0700 under umask 000 and 022 (set with `libc::umask` in a single-threaded test binary, or a subprocess); a symlinked folder, a group-writable folder and an other-writable folder are each refused with the folder named. Foreign ownership is tested through an injected `uid` (the check takes the expected uid).
- AC3: a write that fails after the temporary file was created (for example, the target is a folder) leaves no temporary file.
- AC4: a table of names: valid, too short, too long, uppercase, leading digit, `--`, trailing `-`, `if`, `cd`, `ls` (on `PATH`), and the name of a script already in `bin/` (allowed, so it can be replaced after asking).

## Comments

**2026-11-04, Claude Code — done.** `src/home/` (`mod.rs`: `Home`, `check`, `ensure`, `check_folder`; `write.rs`: `write_private`; `names.rs`: `ScriptName`, `NameTaken`), `tests/home_umask.rs`. `Config::path()` now goes through `Home`, so `TERMITE_HOME` moves the config too. `libc` 0.2 added as a direct dependency (owner approved; it was already in `Cargo.lock`). 11 new unit tests and the umask test pass; clippy `-D warnings` is clean.

- `O_NOFOLLOW | O_CLOEXEC` on top of `create_new` (`O_EXCL`), then `fchmod`, `fsync`, `rename`, `fsync` of the folder.
- The umask test is its own test binary, because the umask belongs to the whole process; it covers 000, 022, 077 and 777.
- Foreign ownership is tested through `check_as(uid)`, since the tests cannot create files owned by someone else.
- A name that only matches a non-executable file on `PATH` is allowed; one that matches a script already in `bin/` is allowed so it can be replaced after asking (ticket 04).
