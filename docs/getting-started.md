# Getting started

termite is one command, `tmite`. You give it a request in plain English, a model on your own machine writes a shell command, and termite tells you what the command does and how risky it looks. termite does not run the command; you decide whether to run it.

## Set it up with your agent

Copy this prompt and give it to any agent, such as Claude Code or Codex. The agent finds your system, your harnesses and the models that fit. You type y for each change in your own terminal, and then the agent proves that the checks work. [Read about agent tools](agents.md).

[Read the prompt](agent-setup-prompt.txt)

To set it up by hand, follow the steps below.

## Install

Download a release, or build termite from source. Either way, you also need llama.cpp, which serves the models.

### Download a release

Choose your system: `macos-arm64`, `macos-x86_64`, `linux-x86_64` or `linux-arm64`. The Linux builds run on any distribution. This example is for an Apple silicon Mac:

```bash
curl -fLO https://github.com/tanepiper/termite/releases/latest/download/tmite-macos-arm64.tar.gz
```

Check the download with the [GitHub CLI](https://cli.github.com). If this fails, delete the file and do not run it:

```bash
gh attestation verify tmite-macos-arm64.tar.gz --repo tanepiper/termite
```

Unpack it, and put `tmite` in a folder on your `PATH`:

```bash
tar -xzf tmite-macos-arm64.tar.gz && mkdir -p ~/.local/bin && mv tmite ~/.local/bin/
```

The macOS builds are not signed by Apple. A file downloaded with `curl` runs normally. A file downloaded in a browser is blocked, so use `curl`.

[Releases](/termite/releases/) lists every version, with the hash of each file.

### Build from source

You need Rust 1.87 or newer and llama.cpp, which serves the models. Setup can download models for llama.cpp. If you already run Ollama or LM Studio, setup finds them and lists their models, but it doesn't download models for them.

#### macOS

```bash
brew install rust llama.cpp
```

```bash
cargo install --locked --git https://github.com/tanepiper/termite termite
```

#### Linux

Install Rust with [rustup](https://rustup.rs), because the `cargo` package in your distribution may be older than 1.87. Install llama.cpp from its [releases page](https://github.com/ggml-org/llama.cpp/releases), and make sure `llama-server` is on your `PATH`.

```bash
cargo install --locked --git https://github.com/tanepiper/termite termite
```

### Windows

termite does not support Windows. It is written for macOS and Linux.

On both systems, `tmite` installs to `~/.cargo/bin`. Make sure that folder is on your `PATH`.

## Set it up

```bash
tmite setup
```

Setup first finds the agent harnesses on this machine, such as Claude Code, Codex and Cursor, and offers each one a hook that checks shell commands before they run. It shows the change, and it changes a file only if you type `y`. See [agent tools](agents.md#add-termite-to-your-agent-harness).

Setup then looks for model servers on the usual ports and asks you to choose two models: one that writes commands, and a decision model that checks them. If a model is missing, setup picks one that fits this machine, shows the download and its size, and downloads the model only if you type `y`.

## Ask for a command

```bash
tmite '? find files bigger than 100MB here'
```

Keep the quotes, because `?` has a special meaning in your shell.

```text
✓ reads only  Finds regular files over 100MB below this folder.
  find . -type f -size +100M
```

The answer shows what the command does, how risky it looks, and the command itself. Nothing has run. To use the command, copy it.

The first answer is slow while the model loads; later answers take a few seconds.

To continue, read [using termite](using-termite.md). To test a model, read [red-teaming a model](red-team.md).

## Optional extras

### The prompt widget

The widget is optional. `tmite` does the same work in any shell, and the checks are the same with or without the widget. With the widget, you type requests at your prompt without `tmite` or quotes, and the command you choose appears on your command line, so you do not need to copy it.

There is a widget for zsh, bash and fish. `tmite setup` detects your shell and offers to add the widget, and it changes your startup file only if you agree. To add the widget yourself, put one line in that file:

| Shell | File | Line |
|---|---|---|
| zsh | `~/.zshrc` | `eval "$(tmite shell zsh)"` |
| bash | `~/.bashrc` | `eval "$(tmite shell bash)"` |
| fish | `~/.config/fish/config.fish` | `tmite shell fish \| source` |

The bash widget needs bash 4 or later. The `/bin/bash` included with macOS is version 3.2, so install a newer version with Homebrew or use zsh, which is the default on macOS.

Open a new shell after you add it. [Using termite](using-termite.md#the-prompt-widget) explains what the widget does.

### A token for the decision model

If your decision server needs a token, set it in `TERMITE_DECISION_TOKEN`. termite reads the token from that variable and never stores it. Local servers usually do not need a token.
