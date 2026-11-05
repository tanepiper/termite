# Getting started

termite is one command: `tmite`. You give it a request in plain English, a model
on your own machine writes a shell command, and termite tells you what it does and
how risky it looks. It doesn't run it. Ever. That part's yours.

## Install

Pick your platform. You need Rust 1.85 or newer, and something that serves a
model. These steps use Ollama, because it's the shortest route.

### macOS

```bash
brew install rust ollama
```

```bash
cargo install --locked --git https://github.com/tanepiper/termite
```

### Linux

Install Rust with [rustup](https://rustup.rs). Your distro's `cargo` package may
well be older than 1.85. Install Ollama from [their download
page](https://ollama.com/download).

```bash
cargo install --locked --git https://github.com/tanepiper/termite
```

### Windows

Not supported. termite is written for macOS and Linux.

Either way, `tmite` lands in `~/.cargo/bin`, so make sure that's on your `PATH`.

## Set it up

```bash
tmite setup
```

Setup looks for model servers on the usual ports and asks you to pick two models:
one to write commands, and a decision model to check them. If you're missing one it shows you the
download, with its size, and only fetches it if you type `y`.

## Ask it something

```bash
tmite '? find files bigger than 100MB here'
```

Keep the quotes. `?` means something to your shell.

```text
✓ reads only  Finds regular files over 100MB below this folder.
  find . -type f -size +100M
```

That's the whole deal: what it does, how risky it looks, and the command. Nothing
has run. If you like it, copy it.

The first answer is slow while the model loads. After that it's a few seconds.

Next: [using termite](using-termite.md). If you're here to test a model rather
than use one, go to [red-teaming a model](red-team.md).

## Optional extras

### The prompt widget

This is a convenience, nothing more. `tmite` does the same job in any shell, and
the checks are the same either way. The widget just saves you copying: you type
requests at your prompt, without `tmite` or the quotes, and the command you pick
lands on your command line.

There's one for zsh, bash and fish. `tmite setup` spots your shell and offers to add
it, and it only touches your startup file if you say yes. To add it by hand, put one
line in that file:

| Shell | File | Line |
|---|---|---|
| zsh | `~/.zshrc` | `eval "$(tmite shell zsh)"` |
| bash | `~/.bashrc` | `eval "$(tmite shell bash)"` |
| fish | `~/.config/fish/config.fish` | `tmite shell fish \| source` |

The bash one needs bash 4 or later. The `/bin/bash` that ships with macOS is 3.2, so
get a newer one from Homebrew or use zsh, which is the macOS default.

Open a new shell. [Using termite](using-termite.md#the-prompt-widget) shows what you
get.

### A token for the decision model

If your decision server wants a token, put it in `TERMITE_DECISION_TOKEN`. termite
reads it from there and never stores it. Local servers usually don't need one.
