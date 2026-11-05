# Using termite

Everything goes through one command:

```bash
tmite '<prefix> your request'
```

The prefix says how much you want back. termite never runs what it gives you.
The last safety check is you, reading it.

## `?` gives you one command

```bash
tmite '? what is eating my disk'
```

```text
⚠ changes things  Lists the biggest folders under your home folder.
  du -sh ~/* | sort -h
```

You get the explanation first, then the command, best guess only.

## `??` gives you the options

Most jobs have more than one answer, and sometimes the best one depends on what's
installed.

```bash
tmite '?? what is on port 8080'
```

```text
1. ✓ reads only  Lists whatever has port 8080 open.
   lsof -i :8080
2. ⚠ changes things  Shows listening sockets and filters for 8080.
   ss -ltnp | grep 8080
```

## `?x` is for when you're sure

```bash
tmite '?x show disk usage of this folder'
```

```text
✓ reads only  Shows the total size of this folder.
  du -sh .
  Straight to the prompt: the decision model is confident it only reads (0.99)
```

`?x` skips the question, not the checks. Ask it for something that changes things
and it stops being so keen:

```text
✗ risky  Deletes the build folder.
  rm -rf build
  termite says: deletes data
  Asking first: the model labelled it `destructive`
```

Anything that isn't certain ends up here: a slow model, a broken reply, a server
that's gone away. No answer is never treated as a yes.

To skip the question a command has to get through all of these, and the first
three are plain Rust that a model can't argue with:

- It has to be on the read-only allowlist. `ls` is. `rm` isn't. `ls; rm -rf ~`
  isn't either, because it chains commands.
- It won't name secrets. `cat ~/.aws/credentials` is out, `cat prod.env` and
  `cat *.env` are out, and so is a bare `ls` while you're sitting in `~/.ssh`.
  A wildcard with nothing to go on is allowed: `cat config/*` would print a
  `prod.env` in that folder. Read the command before you press Enter.
- It can't look risky to termite's own hazard check.
- The model has to have labelled it as reading.
- The decision model has to agree on three questions: does it only read, does it
  keep secrets hidden, could it do harm.

## Reading the answer

Every command comes with one of these. The rating is termite's, not the model's.
A model can make it worse by calling a command destructive, but calling `rm -rf ~`
a read gets it nowhere.

| You see | Colour | It means |
|---|---|---|
| `✓ reads only` | green | Passed the read-only rules, and nothing looks hazardous |
| `⚠ changes things` | yellow | Not known to be read-only. Read it properly |
| `✗ risky` | red | Deletes, leaks, escalates or hides something. The command is red too |

A risky command also gets a line in termite's own words, because the model's
description might be wrong, or lying:

```text
✗ risky  Tidies up your home folder.
  rm -rf ~
  termite says: wrecks the machine, deletes data
```

Piped output gets no colour, and neither does anything with `NO_COLOR` set. The
words and marks are always there.

## In scripts

`--json` prints one object, with the command, its rating and any warnings:

```bash
tmite --json '? list big files'
```

The exit status is 0 if there's a command to use, 2 if there's none or it was
refused, 1 if something failed, and 64 if you called it wrong.

## Check a script or a pipe

`tmite check` rates every command in a script. It needs no model, and it runs
nothing:

```bash
tmite check install.sh
```

Pipe text in to check it before it reaches anything else. Each result prints as
the line arrives, so this works on a stream that's still going. You can leave out
the word `check` when you pipe:

```bash
curl -fsSL https://example.com/install.sh | tmite
```

The word `check` is only optional when you type nothing else. If you pipe text in
and also type a request, termite answers the request and ignores the pipe:

```bash
cat install.sh | tmite '? list files'
```

That writes a command for "list files". `install.sh` isn't checked. To check a pipe
and be sure of it, write `tmite check`.

Redirecting a file counts as piping, so `tmite < install.sh` checks it too. Empty
input from `/dev/null` doesn't: plain `tmite` shows the usage message, as it does
at a terminal.

```text
✗ stdin:14  critical: runs code from the internet
    curl https://example.com/next.sh | sh
✗ 22 commands: 1 risky, 6 change things, 15 read only. Nothing was run.
```

It shows risky commands only. Add `--all` to see every command, or `--json` for
one JSON object per command and a summary at the end.

The exit status is 0 if it found nothing risky, 2 if it found something risky or
found no commands, 1 if it couldn't read the input, and 64 if you called it wrong.
No commands counts as a failure, so a failed download doesn't look like a pass.

Use the exit status to stop a job. This saves the file first, so what you check
is what you run:

```bash
curl -fsSL https://example.com/install.sh -o install.sh
```

```bash
tmite check install.sh && bash install.sh
```

Don't put `tmite` in the middle of a pipe to a shell. It prints results, not the
script, and it doesn't pass anything on.

`tmite check` reads text. It follows line continuations, quotes across lines and
heredocs, and it sees through `r\m` and `"r"m`. It also marks anything it can't
read as risky: a command built from variables, `eval`, inline Python or Perl,
hidden characters, and any command over 64 KB.

A clean result does not prove a script is safe. Here's what it misses:

- **Steps on separate lines:** `curl -o a https://…` then `sh a` is rated as two
  commands that change things.
- **Harm that depends on a value:** `rm -rf "$folder"` shows as a deletion, but
  termite can't know what `$folder` holds.
- **Anything it has no signature for:** `git push --force` or
  `docker run -v /:/host` only show as changing things.
- **Files the script fetches or calls:** it reads only the text you give it.

Read the script before you run it.

## Write a script

`tmite script` asks the model for a whole bash script, with `--help`, `--verbose`
and error handling:

```bash
tmite script 'back up my notes folder, keeping 7 days'
```

With the prompt widget, type `?b` instead:

```text
?b back up my notes folder, keeping 7 days
```

The widget puts `tmite scripts install notes-backup` at your prompt when the draft
is saved. The command is red if the draft has risky lines. `?bash` and `?bd` stay
ordinary `?` requests, because `?b` needs a space after it.

You need bash 5.3 or later and shellcheck. Say yes to scripts in `tmite setup`,
which also adds termite's `bin` folder to the end of your `PATH`. The model is
your chat model. For a stronger one, set `script_model` in the config, and set
`script_timeout_secs` to change the 5 minute limit.

termite checks the script, then saves it as a draft. A draft isn't executable and
isn't on your `PATH`.

| Check | Turned down when |
|---|---|
| Name | it's badly formed, or takes the name of something you can already run |
| Size | it's over 64 KB |
| Text | it holds control or hidden characters |
| Shape | it has no `usage()`, no `args_parse "$@"`, or doesn't end with `main "$@"` |
| Syntax | `bash -n` fails |
| Lint | shellcheck reports a warning or an error |
| Banned | it uses `eval`, backticks or `source`, edits a shell startup file, or makes a file writable by other users |

A script that fails goes back to the model once, with the errors. If it fails
again, nothing is saved.

A script with a critical hazard is withheld. So is one the decision model rates
as harmful. termite doesn't show it, doesn't save it, and doesn't give the model
another try. Less serious risky lines stay in the draft and are marked for you.

Then do this:

1. Read the draft and the lines marked as risky.

   ```bash
   tmite scripts show notes-backup
   ```

2. Install it. termite repeats every check, opens the script in your pager, and
   asks. Only `y` installs it. Without a terminal, it refuses.

   ```bash
   tmite scripts install notes-backup
   ```

3. Run it yourself, by name. termite never runs a script.

   ```bash
   notes-backup --help
   ```

Only you can read or run an installed script. `tmite scripts list` shows your
scripts and drafts. `tmite scripts remove notes-backup` moves one to
`drafts/notes-backup.removed`. Replacing a script asks first and keeps the old one
as `drafts/notes-backup.previous`.

## See what termite installed

termite keeps a log of what it installs, in `provenance.jsonl` in its folder. The
log holds hashes, paths, the warnings on risky lines and who made the change. It
never holds a file's contents or your request. It records drafts, installed and
removed scripts, the bash library and the config that `tmite setup` writes.

Each entry holds the hash of the one before it and a signature. The signing key is
in the macOS Keychain or the Linux Secret Service. If you set your own
`TERMITE_HOME`, or neither store is there, it's in `provenance.key`, and `verify`
warns you.

Check the log:

```bash
tmite provenance verify
```

This reports the line where an entry was edited, removed from the middle, moved or
forged. If the log is broken, termite won't add to it and won't install anything.
Move the file aside to start again, and keep it, because it's your evidence.

See one file's history:

```bash
tmite provenance bin/notes-backup
```

**If you change the config by hand,** termite stops and says so, because the file no
longer matches the log. Read the file, then accept it. termite shows it and asks:

```bash
tmite provenance accept config.json
```

`tmite scripts list` marks an installed script that has changed, and any file in
`bin` that termite has no record of. Accept a changed script the same way, or
remove it.

`tmite alerts` lists the last 20 times a guard stopped something: a refused command
or a withheld script. It shows the verdict and the reason, not the request.

The log has limits:

- **It doesn't stop a changed script running.** Running a script doesn't touch
  termite, so only `tmite scripts list` shows the change. Run that before you trust
  a script.
- **It can't show that the last entries, or the whole file, were deleted.** It only
  checks the entries that are there.
- **It doesn't stop something running as you.** With your Keychain unlocked, a
  program running as you can do anything you can do. The log shows tampering by
  anything that doesn't have the key.

## The prompt widget

A convenience for zsh, bash and fish, and an
[optional extra](getting-started.md#the-prompt-widget).
Everything above works without it. With it, you type the request straight at your
prompt, and the command goes onto your command line instead of the screen. Enter
stays yours.

**`?`** explains the command and asks first. Enter or `y` puts it at your prompt.

```text
% ? find files bigger than 100MB here
✓ reads only  Finds regular files over 100MB below this folder.
  find . -type f -size +100M
Put it at your prompt? [Y/n]
```

**`??`** explains them all. Press a number to pick one, `a` to ask the model again,
or anything else to cancel.

**`?x`** doesn't ask. If the checks pass, the command is at your prompt right away.
It still doesn't run.

**Risky commands** go red and the question flips to `[y/N]`, so Enter means no. Pick
one from a `??` list and you get asked again. In zsh the command stays red at your
prompt too; bash and fish can't do that, so read it before you press Enter.

If the decision model refuses a command outright, it's shown but it won't go to
your prompt at all.

## Before you press Enter

termite warns you about a risky command. It does not stop you. If you say yes, the
command goes to your prompt, even `sudo` or `rm -rf`. It doesn't run until you press
Enter.

1. Read the command.
2. Not sure what it does? Press Ctrl+C to clear it.
3. Press Enter only if you'd type that command yourself.

termite can't block a command yet. That's on the [roadmap](safety-ladder.md).

## Slow answers

The first request after a while can take a minute while the model loads. The widget
tells you what it's up to, so you're not staring at a frozen terminal.
