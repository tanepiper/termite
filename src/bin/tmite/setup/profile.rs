//! The last part of setup: get scripts ready. Contract: nothing changes without
//! a yes, a startup file gets one block however often this runs, and the old
//! file is kept.

use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use termite::{Home, HomeError, PRIVATE_DIR_MODE, Problem};

use super::prompt::Prompt;
use crate::shell::Shell;
use crate::{library, script_tools};

const START: &str = "# >>> termite >>>";
const END: &str = "# <<< termite <<<";

const SH_BLOCK: &str = r#"export TERMITE_HOME="${TERMITE_HOME:-$HOME/.config/termite}"
case ":$PATH:" in
  *":$TERMITE_HOME/bin:"*) ;;
  *) export PATH="$PATH:$TERMITE_HOME/bin" ;;
esac
"#;
const FISH_BLOCK: &str = r#"set -q TERMITE_HOME; or set -gx TERMITE_HOME "$HOME/.config/termite"
contains -- "$TERMITE_HOME/bin" $PATH; or set -gx PATH $PATH "$TERMITE_HOME/bin"
"#;
const BASH_LOGIN_BLOCK: &str = "[ -f ~/.bashrc ] && . ~/.bashrc\n";

/// Offer everything scripts need. Failing here doesn't undo the rest of setup.
pub(super) fn offer(
    prompt: &Prompt,
    shell: Option<Shell>,
    user_home: Option<&Path>,
) {
    println!("\nOptional: termite can write bash scripts for you.");
    if !prompt.confirm("Set up scripts?", false) {
        println!("Left alone. Run `tmite setup` again to add them.");
        return;
    }
    if let Err(problem) = prepare_folders(prompt) {
        println!("Scripts aren't set up: {problem}");
        return;
    }
    if let Err(problem) = script_tools::ready() {
        println!("You can't write scripts yet: {problem}");
    }
    let (Some(shell), Some(user_home)) = (shell, user_home) else {
        println!("Add $TERMITE_HOME/bin to the end of your PATH by hand.");
        return;
    };
    for (file, block) in blocks(shell, user_home) {
        println!("\nThis goes in {}:\n\n{}", file.display(), wrapped(block));
        if !prompt.confirm("Add it?", false) {
            println!("Left alone.");
            continue;
        }
        match apply(&file, block) {
            Ok(()) => println!(
                "Done. The old file is at {}.",
                backup_of(&file).display()
            ),
            Err(error) => {
                println!("Couldn't change {}: {error}", file.display())
            },
        }
    }
}

/// Make the folders, offering to fix any that others can write to.
fn prepare_folders(prompt: &Prompt) -> Result<(), String> {
    let home = Home::from_env().map_err(|error| error.to_string())?;
    loop {
        match home.ensure() {
            Err(HomeError::Unsafe {
                path,
                problem: Problem::WritableByOthers,
            }) => {
                let question = format!(
                    "{} can be written to by other users. Set it to 0700?",
                    path.display()
                );
                if !prompt.confirm(&question, false) {
                    return Err("that folder isn't safe".to_owned());
                }
                fs::set_permissions(
                    &path,
                    Permissions::from_mode(PRIVATE_DIR_MODE),
                )
                .map_err(|error| error.to_string())?;
            },
            Err(error) => return Err(error.to_string()),
            Ok(()) => break,
        }
    }
    library::install(&home)
}

/// What to add to which file for `shell`.
fn blocks(shell: Shell, user_home: &Path) -> Vec<(PathBuf, &'static str)> {
    let file = shell.startup_file(user_home);
    match shell {
        Shell::Zsh => vec![(file, SH_BLOCK)],
        Shell::Fish => vec![(file, FISH_BLOCK)],
        Shell::Bash => {
            let login = user_home.join(".bash_profile");
            let mut found = vec![(file, SH_BLOCK)];
            let reads_bashrc = fs::read_to_string(&login)
                .is_ok_and(|text| text.contains(".bashrc"));
            if !reads_bashrc {
                found.push((login, BASH_LOGIN_BLOCK));
            }
            found
        },
    }
}

fn wrapped(block: &str) -> String {
    format!("{START}\n{block}{END}\n")
}

/// `text` with its termite block replaced, or a block added at the end. Bytes
/// outside the block don't change.
fn with_block(text: &str, block: &str) -> String {
    let wanted = wrapped(block);
    let found = text.find(START).and_then(|start| {
        let end = text[start..].find(END)? + start + END.len();
        Some((start, end))
    });
    let Some((start, end)) = found else {
        let gap = match text {
            "" => "",
            _ if text.ends_with('\n') => "\n",
            _ => "\n\n",
        };
        return format!("{text}{gap}{wanted}");
    };
    let rest = text[end..].strip_prefix('\n').unwrap_or(&text[end..]);
    format!("{}{wanted}{rest}", &text[..start])
}

fn backup_of(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".termite-backup");
    PathBuf::from(name)
}

/// Update a startup file. A link is followed, so dotfile managers keep working.
/// The new file replaces the old one in one step, with the same mode.
fn apply(file: &Path, block: &str) -> io::Result<()> {
    let target = fs::canonicalize(file).unwrap_or_else(|_| file.to_owned());
    let (old, mode) = match fs::read(&target) {
        Ok(bytes) => {
            let mode = fs::metadata(&target)?.permissions().mode() & 0o777;
            (String::from_utf8_lossy(&bytes).into_owned(), mode)
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            (String::new(), 0o644)
        },
        Err(error) => return Err(error),
    };
    let new = with_block(&old, block);
    if new == old {
        return Ok(());
    }
    if let Some(folder) = target.parent() {
        fs::create_dir_all(folder)?;
    }
    if target.exists() {
        fs::copy(&target, backup_of(file))?;
    }

    let mut temporary = target.as_os_str().to_owned();
    temporary.push(".termite-new");
    let temporary = PathBuf::from(temporary);
    // The user's own startup file keeps the mode it had, so the private-file
    // rule doesn't apply. The name is opened new, and never through a link.
    // nosemgrep: termite-files-are-written-privately
    let mut new_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)?;
    let written = new_file
        .write_all(new.as_bytes())
        .and_then(|()| new_file.sync_all())
        .and_then(|()| fs::rename(&temporary, &target));
    if written.is_err() {
        // The rename failed or never happened; the original is untouched.
        let _ = fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests;
