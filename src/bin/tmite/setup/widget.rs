//! The last step of setup: offer the prompt widget for the shell in use.
//! Contract: a startup file is only touched after a yes, only ever appended
//! to, and never twice.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use super::prompt::Prompt;
use crate::shell::Shell;

/// Offer the widget, and say how to add it by hand if it wasn't added.
pub(super) fn offer(
    prompt: &Prompt,
    shell: Option<Shell>,
    home: Option<&Path>,
) {
    let (Some(shell), Some(home)) = (shell, home) else {
        println!(
            "\nThere's no prompt widget for this shell. tmite works the same without one."
        );
        return;
    };

    let file = shell.startup_file(home);
    let line = shell.load_line();
    if already_there(&file, line) {
        return;
    }

    println!(
        "\nOptional: you use {}. A small widget lets you type `? request` at your prompt, which saves copying commands.",
        shell.name()
    );
    let question = format!("Add `{line}` to {}?", file.display());
    if !prompt.confirm(&question, false) {
        println!(
            "Left alone. To add it later, put that line in {}.",
            file.display()
        );
        return;
    }

    match append(&file, line) {
        Ok(()) => println!("Added. Open a new shell to use it."),
        Err(error) => {
            println!("Could not write to {}: {error}", file.display())
        },
    }
}

fn already_there(file: &Path, line: &str) -> bool {
    fs::read_to_string(file).is_ok_and(|text| text.contains(line))
}

fn append(file: &Path, line: &str) -> io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    // The person's own startup file, not one of termite's: it keeps its mode,
    // and dotfile managers often make it a symlink, which has to be followed.
    let mut startup =
        // nosemgrep: termite-files-are-written-privately
        OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(startup, "\n# termite's prompt widget\n{line}")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::offer;
    use crate::setup::prompt::Prompt;
    use crate::shell::Shell;

    fn folder(name: &str) -> std::path::PathBuf {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let path = std::env::temp_dir()
            .join(format!("termite-offer-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).expect("folder");
        path
    }

    #[test]
    fn given_a_yes_then_the_line_is_added_once_and_the_rest_is_kept() {
        let home = folder("yes");
        // nosemgrep: termite-files-are-written-privately
        fs::write(home.join(".zshrc"), "alias ll='ls -l'\n").expect("zshrc");

        offer(&Prompt::answering("y\n"), Some(Shell::Zsh), Some(&home));
        offer(&Prompt::answering("y\n"), Some(Shell::Zsh), Some(&home));

        let text = fs::read_to_string(home.join(".zshrc")).expect("zshrc");
        assert!(text.starts_with("alias ll='ls -l'\n"), "{text}");
        assert_eq!(text.matches("tmite shell zsh").count(), 1, "{text}");
        fs::remove_dir_all(home).expect("clean up");
    }

    #[test]
    fn given_no_yes_then_nothing_is_written() {
        let home = folder("no");

        offer(&Prompt::answering("\n"), Some(Shell::Bash), Some(&home));
        offer(&Prompt::without_terminal(), Some(Shell::Fish), Some(&home));

        assert!(!home.join(".bashrc").exists());
        assert!(!home.join(".config").exists());
        fs::remove_dir_all(home).expect("clean up");
    }

    #[test]
    fn given_fish_then_its_config_folder_is_made() {
        let home = folder("fish");

        offer(&Prompt::answering("yes\n"), Some(Shell::Fish), Some(&home));

        let file = home.join(".config/fish/config.fish");
        let text = fs::read_to_string(file).expect("config");
        assert!(text.contains("tmite shell fish | source"), "{text}");
        fs::remove_dir_all(home).expect("clean up");
    }
}
