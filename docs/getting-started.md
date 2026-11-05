# Getting started

termite is one command, `tmite`. You give it a request in plain English, a model on your own machine writes a shell command, and termite tells you what the command does and how risky it looks. termite does not run the command; you decide whether to run it.

## Install

You need Rust 1.87 or newer and a program that serves a model. These steps use Ollama because it needs the fewest steps.

### macOS

```bash
brew install rust ollama
```

```bash
cargo install --locked --git https://github.com/tanepiper/termite
```

### Linux

Install Rust with [rustup](https://rustup.rs), because the `cargo` package in your distribution may be older than 1.87. Install Ollama from the [Ollama download page](https://ollama.com/download).

```bash
cargo install --locked --git https://github.com/tanepiper/termite
```

### Windows

termite does not support Windows. It is written for macOS and Linux.

On both systems, `tmite` installs to `~/.cargo/bin`. Make sure that folder is on your `PATH`.

## Set it up

```bash
tmite setup
```

Setup looks for model servers on the usual ports and asks you to choose two models: one that writes commands, and a decision model that checks them. If a model is missing, setup shows the download and its size, and it downloads the model only if you type `y`.

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
