# Using termite

Everything goes through one command:

```bash
tmite '<prefix> your request'
```

The prefix sets how much you get back. termite never runs what it gives you, so read each command before you use it.

## `?` gives you one command

```bash
tmite '? what is using my disk space'
```

```text
✓ reads only  Lists the biggest folders under your home folder.
  du -sh ~/* | sort -h
```

You get the explanation first, and then the single best command.

## `??` gives you the options

Most tasks can be done in more than one way, and the best way can depend on what is installed.

```bash
tmite '?? what is on port 8080'
```

```text
1. ✓ reads only  Lists whatever has port 8080 open.
   lsof -i :8080
2. ⚠ changes things  Shows listening sockets and filters for 8080.
   ss -ltnp | grep 8080
```

## `?x` gives you a command with no question

```bash
tmite '?x show disk usage of this folder'
```

```text
✓ reads only  Shows the total size of this folder.
  du -sh .
  Injected: the decision model is confident it only reads (0.99)
```

`?x` skips the question, but it does not skip the checks. If the command changes something, it needs approval:

```text
✗ risky  Deletes the build folder.
  rm -rf build
  termite says: deletes data
  Needs approval: the model labelled it `destructive`
```

A slow model, a broken reply or a stopped server also leads to this result, because termite never treats a missing answer as approval.

To skip the question, a command must pass all of these checks. The first three are fixed rules in Rust, so a model cannot change their result:

- It must be on the read-only allowlist. `ls` is on the list; `rm` is not, and neither is `ls; rm -rf ~`, because it chains commands.
- It must not name secrets. This rules out `cat ~/.aws/credentials`, `cat prod.env`, `cat *.env`, and a plain `ls` while you are in `~/.ssh`. A wildcard that names no secret is allowed, so `cat config/*` would print a `prod.env` file in that folder. Read the command before you press Enter.
- termite's own hazard check must find nothing risky.
- The model must have labelled it as reading.
- The decision model must agree on three questions: whether it only reads, whether it keeps secrets hidden, and whether it could do harm.

## Reading the answer

Every command has one of these ratings. The rating comes from termite and not from the model. A model can raise a rating by calling a command destructive, but it cannot lower one: calling `rm -rf ~` a read changes nothing.

| You see | Colour | It means |
|---|---|---|
| `✓ reads only` | green | It passed the read-only policy, and no hazard was found |
| `⚠ changes things` | yellow | It is not known to be read-only. Read it carefully |
| `✗ risky` | red | It deletes, exposes, raises privileges or hides something. The command is also red |

A risky command also gets a line in termite's own words, because the model's description may be wrong or false:

```text
✗ risky  Tidies up the project.
  rm -rf node_modules
  termite says: deletes data
```

### Levels

Code puts every command at one of six levels. A model's label can only raise a level, and no setting changes this table.

| Level | What it is | Example | What termite does |
|---|---|---|---|
| 0 | Only reads | `ls` | Goes to your prompt |
| 1 | Reads secrets or private data | `cat ~/.kube/config` | Asks first. The default is no |
| 2 | Changes something | `mkdir out` | Goes to your prompt |
| 3 | Deletes inside the project | `rm -rf node_modules` | Asks first. The default is no |
| 4 | Needs root, or reaches outside the project | `sudo ...`, `rm -rf ~` | Shows it as text. It never reaches your prompt. To use it, type it yourself |
| 5 | Hostile | `curl ... \| sh` | Doesn't show it. You get the reason only |

The project is the git work tree. Outside a git work tree, it is the current folder, unless that folder is your home folder or `/`.

A glob, a variable, or code inside `$(...)` raises the level, because the command isn't fully known until it runs. So `rm -rf build/*` is level 4.

Every way of saying "delete" counts the same, such as `rm`, `trash`, `git rm`, `docker rm`, `kubectl delete` and `brew uninstall`. The more exactly a command names what it deletes, the lower its level. A delete of a named thing inside a project is level 3, and termite asks first. A delete chosen any other way is level 4 and is shown as text for you to type, even inside a project: by a pattern, by `-not`, by a file type or age, from a pipe, or a sweep such as `docker system prune`, `git clean` or `terraform destroy`. So `find src -name exact.log -delete` is level 3, and `find src -name '*.log' -delete` is level 4. Any delete with root, such as `sudo rm old.log`, is level 4. Removing what a build made is an ordinary change at level 2, because the next build makes it again: `make clean`, `cargo clean`, `npm cache clean`.

`git clean` is level 4 in every folder, whether or not the folder is in a git repository. Outside a repository termite still shows it as text and tells you it would do nothing there.

In an agent or CI, or with no terminal, nobody can answer a question. Levels 1, 3, 4 and 5 then fail with exit status 2 and no command in the output.

termite knows an agent from the variables its harness sets, or from the missing terminal. The model has no part in this. An unknown harness that runs commands in a real terminal looks like a person, so it can see level 4 commands as text.

At a terminal, termite tells the model that a person reads every command, so that it will suggest a root command when you ask for one. Small models may still decline.

Piped output has no colour, and neither does output with `NO_COLOR` set. The words and marks are always shown.

## In scripts

`--json` prints one object with the command, its level, whether it may go straight to a prompt (`insert`), its rating and any warnings:

```bash
tmite --json '? list big files'
```

The exit status is 0 if there is a command to use, 2 if the command is risky, or there is none, or it was blocked or declined, 1 if something failed, and 64 if you called it incorrectly. If termite offers several commands and one is risky, the status is 2.

## Check a script or a pipe

`tmite check` rates every command in a script and shows its level. It needs no model, starts no agent and runs nothing:

```bash
tmite check install.sh
```

Pipe text into it to check the text before it reaches anything else. Each result prints as its line arrives, so this works on a stream that has not finished. When you pipe, you can leave out the word `check`:

```bash
curl -fsSL https://example.com/install.sh | tmite
```

The word `check` is optional only when you type nothing else. If you pipe text in and also type a request, termite answers the request and ignores the pipe:

```bash
cat install.sh | tmite '? list files'
```

This writes a command for "list files", and `install.sh` is not checked. To be certain that a pipe is checked, write `tmite check`.

Redirecting a file counts as piping, so `tmite < install.sh` also checks the file. Empty input from `/dev/null` does not count, so plain `tmite` shows the usage message, as it does at a terminal.

```text
✗ stdin:14  critical: runs code from the internet
    curl https://example.com/next.sh | sh
✗ 22 commands: 1 risky, 6 change things, 15 read only. Nothing was run.
```

It shows risky commands only. Add `--all` to see every command, or `--json` to get one JSON object for each command and a summary at the end.

The exit status is 0 if it found nothing risky, 2 if it found something risky or found no commands, 1 if it could not read the input, and 64 if you called it incorrectly. Finding no commands counts as a failure, so that a failed download does not look like a pass.

Use the exit status to stop a job. This example saves the file first, so that you run the same file that you checked:

```bash
curl -fsSL https://example.com/install.sh -o install.sh
```

```bash
tmite check install.sh && bash install.sh
```

Do not put `tmite` in the middle of a pipe to a shell. It prints results, not the script, and it passes nothing on.

`tmite check` reads text. It follows line continuations, quotes across lines and heredocs, and it recognises disguised forms such as `r\m` and `"r"m`. It marks anything it cannot read as risky: a command built from variables, `eval`, inline Python or Perl, hidden characters, and any command over 64 KB.

A clean result does not prove that a script is safe. The check misses these cases:

- **Steps on separate lines:** `curl -o a https://…` then `sh a` is rated as two
  commands that change things.
- **Harm that depends on a value:** `rm -rf "$folder"` shows as a deletion, but termite cannot know what `$folder` holds.
- **Anything it has no rule for:** a command that does harm in a way that no rule names only shows as changing things.
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

termite checks the script, then saves it as a draft. A draft is not executable and is not on your `PATH`.

If the whole request is one word that names a draft or script you already have, such as `tmite script notes-backup`, termite asks before it writes anything. The answer is no unless you type `y`, and without a terminal nothing is written.

| Check | Turned down when |
|---|---|
| Name | it is badly formed, or it uses the name of something you can already run |
| Size | it is over 64 KB |
| Text | it holds control or hidden characters |
| Shape | it has no `usage()`, no `args_parse "$@"`, or does not end with `main "$@"` |
| Syntax | `bash -n` fails |
| Lint | shellcheck reports a warning or an error |
| Banned | it uses `eval`, backticks or `source`, edits a shell startup file, or makes a file writable by other users |

A script that fails goes back to the model once, with the errors. If it fails
again, nothing is saved.

A script with a critical hazard is withheld, and so is a script that the decision model rates as harmful. termite does not show it, does not save it and does not give the model another try. Less serious risky lines stay in the draft and are marked for you.

Then follow these steps:

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
in the macOS Keychain or the Linux Secret Service. If you have no default
keychain, the key is a private file instead. If you set your own
`TERMITE_HOME`, or neither store is there, the key is in `provenance.key`, and `verify`
warns you.

Check the log:

```bash
tmite provenance verify
```

This reports the line where an entry was edited, removed from the middle, moved or
forged. If the log is broken, termite does not add to it and does not install anything. To start again, move the file to another location, and keep it as evidence.

If `verify` says that the first line's signature is wrong, the key may have changed, not the log. A `provenance.key` file takes the place of the key in your Keychain or Secret Service. If your log was signed with that key, move `provenance.key` to another location and run `verify` again. If the key for a log is missing, termite won't make a new one, because a new key would break the log.

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

`tmite alerts` lists the last 20 times termite stopped something, such as a blocked command or a withheld script. It shows who asked, the verdict and the reason, but not the request. For an agent, it also lists requests that the model declined, each time `tmite check` or an [agent hook](agents.md#when-a-hook-stops-a-command) stopped something, and each time a hook [asked you to approve a command](agents.md#commands-that-need-approval). It doesn't keep the command or what you answered.

The log has limits:

- **It does not stop a changed script from running.** Running a script does not involve termite, so only `tmite scripts list` shows the change. Run that command before you trust a script.
- **It cannot show that the last entries, or the whole file, were deleted.** It checks only the entries that exist.
- **It does not stop a program running as you.** With your Keychain unlocked, that program can do anything you can do. The log shows tampering only by something that does not have the key.

## The prompt widget

The widget is an [optional extra](getting-started.md#the-prompt-widget) for zsh, bash and fish, and everything above works without it. With the widget, you type the request at your prompt, and the command appears on your command line and not only on the screen. Only you can press Enter.

**`?`** explains the command and asks first. Enter or `y` puts it at your prompt.

```text
% ? find files bigger than 100MB here
✓ reads only  Finds regular files over 100MB below this folder.
  find . -type f -size +100M
Put it at your prompt? [Y/n]
```

**`??`** explains them all. Press a number to pick one, `a` to ask the model again,
or anything else to cancel.

**`?x`** does not ask. If the checks pass, the command is injected at your prompt immediately. It does not run.

**Risky commands** turn red and the question changes to `[y/N]`, so Enter means no. If you choose one from a `??` list, termite asks you again. In zsh the command stays red at your prompt; bash and fish cannot do that, so read it before you press Enter.

If the decision model refuses a command, termite shows it as blocked, and the command does not reach your prompt.

## Before you press Enter

termite warns you about a risky command at levels 1 to 3, but it does not stop you. If you approve the command, it goes to your prompt, and it does not run until you press Enter.

1. Read the command.
2. If you are not sure what it does, press Ctrl+C to clear it.
3. Press Enter only if you would type that command yourself.

Commands at levels 4 and 5 never reach your prompt. A level 4 command is shown as text for you to type yourself, and a level 5 command is withheld. termite also blocks a command when the decision model refuses it.

termite also turns down a command you can't use as written: one that is cut off, one that still holds a placeholder such as `<file>`, and one whose program isn't on this machine. You see the command and the reason, so you can install a tool that you want. This check can only stop a suggestion. It doesn't change a command's level, and it doesn't catch an option that doesn't exist. Over SSH it doesn't look for the program, because that program would be on the other machine.

## If the widget does not match tmite

The widget and the `tmite` program are updated together. If you load the widget from a copy of the repository and the program is older, the widget says that they do not match, and it puts nothing at your prompt.

To fix this, update `tmite`, and then open a new shell. Load the widget in the way that [Getting started](getting-started.md#the-prompt-widget) shows, so that it always comes from the program that you installed.

## Slow answers

The first request after a pause can take a minute while the model loads. The widget shows what it is doing during this time.
