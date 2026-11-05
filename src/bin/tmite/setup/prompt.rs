//! Questions on the terminal. Contract: without a terminal nothing is ever
//! answered yes, and choices take their default.

use std::cell::RefCell;
use std::io::{self, BufRead, IsTerminal, Write};

/// Questions on the terminal. Without a terminal, choices take their
/// default and yes/no questions are answered no.
pub(crate) struct Prompt {
    interactive: bool,
    /// Where answers come from: stdin, or fixed text in tests.
    input: RefCell<Box<dyn BufRead>>,
}

impl Prompt {
    pub(crate) fn new() -> Self {
        Self {
            interactive: io::stdin().is_terminal(),
            input: RefCell::new(Box::new(io::stdin().lock())),
        }
    }

    /// Whether a person can answer.
    pub(crate) fn is_interactive(&self) -> bool {
        self.interactive
    }

    /// A prompt that reads its answers from `answers`, as if typed.
    #[cfg(test)]
    pub(crate) fn answering(answers: &'static str) -> Self {
        Self {
            interactive: true,
            input: RefCell::new(Box::new(answers.as_bytes())),
        }
    }

    /// A prompt with no terminal: every question gets its default "no".
    #[cfg(test)]
    pub(crate) fn without_terminal() -> Self {
        Self {
            interactive: false,
            input: RefCell::new(Box::new(io::empty())),
        }
    }

    pub(crate) fn answer(&self, question: &str) -> Option<String> {
        print!("{question} ");
        let _ = io::stdout().flush();
        if !self.interactive {
            println!();
            return None;
        }

        let mut line = String::new();
        match self.input.borrow_mut().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_owned()),
        }
    }

    pub(crate) fn confirm(&self, question: &str, default: bool) -> bool {
        let hint = if default { "[Y/n]" } else { "[y/N]" };
        let answer = self.answer(&format!("{question} {hint}"));
        if !self.interactive {
            return false;
        }
        match answer {
            Some(answer)
                if answer.eq_ignore_ascii_case("y")
                    || answer.eq_ignore_ascii_case("yes") =>
            {
                true
            },
            Some(answer)
                if answer.eq_ignore_ascii_case("n")
                    || answer.eq_ignore_ascii_case("no") =>
            {
                false
            },
            _ => default,
        }
    }

    pub(crate) fn choose(
        &self,
        title: &str,
        options: &[String],
        default: usize,
    ) -> usize {
        println!("{title}");
        for (index, option) in options.iter().enumerate() {
            let marker = if index == default { " (default)" } else { "" };
            println!("  {}. {option}{marker}", index + 1);
        }

        loop {
            let Some(answer) =
                self.answer(&format!("Number [{}]:", default + 1))
            else {
                return default;
            };
            if answer.is_empty() {
                return default;
            }
            match answer.parse::<usize>() {
                Ok(number) if (1..=options.len()).contains(&number) => {
                    return number - 1;
                },
                _ => println!("Type a number from 1 to {}.", options.len()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Prompt;

    #[test]
    fn confirm_reads_yes_no_and_default() {
        assert!(Prompt::answering("Y\n").confirm("q", false));
        assert!(!Prompt::answering("no\n").confirm("q", true));
        assert!(Prompt::answering("\n").confirm("q", true));
        assert!(!Prompt::answering("maybe\n").confirm("q", false));
        assert!(!Prompt::answering("").confirm("q", false));
    }

    #[test]
    fn choose_asks_again_until_a_number_fits() {
        let options = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(Prompt::answering("9\nx\n2\n").choose("t", &options, 0), 1);
        assert_eq!(Prompt::answering("\n").choose("t", &options, 1), 1);
        assert_eq!(Prompt::without_terminal().choose("t", &options, 0), 0);
    }
}
