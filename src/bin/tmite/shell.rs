//! `tmite shell`: the prompt widget for each shell, built into the binary so
//! `cargo install` is all it takes. A widget is a convenience: `tmite` does
//! the same job without one.

use std::path::{Path, PathBuf};

/// A shell with a widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    /// Read a shell's name, or the path to it, as in `$SHELL`.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name.rsplit('/').next().unwrap_or(name) {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }

    /// The shell the user logs in to, if it has a widget.
    pub(crate) fn detect() -> Option<Self> {
        Self::from_name(&std::env::var("SHELL").ok()?)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
        }
    }

    /// The widget's source.
    pub(crate) fn widget(self) -> &'static str {
        match self {
            Self::Zsh => include_str!("../../../shell/tmite.zsh"),
            Self::Bash => include_str!("../../../shell/tmite.bash"),
            Self::Fish => include_str!("../../../shell/tmite.fish"),
        }
    }

    /// The line that loads the widget.
    pub(crate) fn load_line(self) -> &'static str {
        match self {
            Self::Zsh => r#"eval "$(tmite shell zsh)""#,
            Self::Bash => r#"eval "$(tmite shell bash)""#,
            Self::Fish => "tmite shell fish | source",
        }
    }

    /// The file a new shell reads, under the user's home folder.
    pub(crate) fn startup_file(self, home: &Path) -> PathBuf {
        match self {
            Self::Zsh => home.join(".zshrc"),
            Self::Bash => home.join(".bashrc"),
            Self::Fish => home.join(".config/fish/config.fish"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Shell;

    #[test]
    fn given_a_shell_path_then_its_widget_is_found() {
        let cases = [
            ("/bin/zsh", Some(Shell::Zsh)),
            ("/opt/homebrew/bin/bash", Some(Shell::Bash)),
            ("fish", Some(Shell::Fish)),
            ("/bin/dash", None),
            ("", None),
        ];

        for (name, shell) in cases {
            assert_eq!(Shell::from_name(name), shell, "{name}");
        }
    }

    #[test]
    fn given_each_shell_then_its_files_say_how_to_load_it() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
            assert!(shell.widget().contains(shell.load_line()), "{shell:?}");
        }
        assert_eq!(
            Shell::Fish.startup_file(Path::new("/h")),
            Path::new("/h/.config/fish/config.fish")
        );
    }
}
