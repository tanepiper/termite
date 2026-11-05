//! Shows the answer's progress while the main thread waits on the models.
//!
//! Contract: one small thread draws; it ends before `watching` returns, so
//! nothing it writes can land after the answer. For `--zle` it writes
//! `progress<TAB>text` lines, which come before the answer's first line.
//! People see one line on stderr, cleared at the end, and only on a
//! terminal. `--json`, and output to a pipe, get nothing. Only our own words
//! and numbers are written, never a model's text.

use std::io::{self, IsTerminal, Write};
use std::mem;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use termite::{NoProgress, Progress, Stage};

use crate::output::Format;

/// How often the elapsed time is redrawn.
const TICK: Duration = Duration::from_secs(1);

/// Erases the line the cursor is on.
const CLEAR_LINE: &str = "\r\x1b[2K";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Lines,
    OneLine,
}

/// Passes stages to the drawing thread. `Sender` is `Sync`, and sending
/// never blocks, so reporting cannot slow the answer down.
struct Reporter(Sender<Stage>);

impl Progress for Reporter {
    fn stage(&self, stage: Stage) {
        // The thread outlives the reporter, so this only fails if it
        // panicked; the answer matters more than the progress line.
        let _ = self.0.send(stage);
    }
}

/// Run `work`, showing its progress in the way `format` calls for.
pub(crate) fn watching<T>(
    format: Format,
    work: impl FnOnce(&dyn Progress) -> T,
) -> T {
    match format {
        Format::Zle => with_drawing(io::stdout(), Style::Lines, work),
        Format::Human if io::stderr().is_terminal() => {
            with_drawing(io::stderr(), Style::OneLine, work)
        },
        Format::Human | Format::Json => work(&NoProgress),
    }
}

fn with_drawing<T>(
    mut output: impl Write + Send,
    style: Style,
    work: impl FnOnce(&dyn Progress) -> T,
) -> T {
    let (sender, receiver) = mpsc::channel();
    let reporter = Reporter(sender);

    // The scope joins the thread, which ends when the reporter is dropped.
    thread::scope(|scope| {
        scope.spawn(move || draw(&receiver, &mut output, style, TICK));
        let result = work(&reporter);
        drop(reporter);
        result
    })
}

/// What the line says now.
#[derive(Debug, Clone, Copy, Default)]
struct Shown {
    stage: Option<Stage>,
    answering: Option<bool>,
}

impl Shown {
    /// Take in a stage; true when the line should be drawn at once.
    fn update(&mut self, stage: Stage) -> bool {
        if let Stage::Connection { answering } = stage {
            let changed = self.answering != Some(answering);
            self.answering = Some(answering);
            return changed;
        }
        let same_kind = self.stage.is_some_and(|current| {
            mem::discriminant(&current) == mem::discriminant(&stage)
        });
        if !same_kind {
            // A new stage may be with another server.
            self.answering = None;
        }
        self.stage = Some(stage);
        !same_kind
    }

    fn text(self, elapsed: Duration) -> Option<String> {
        let stage = describe(self.stage?, elapsed);
        Some(match self.answering {
            Some(true) => format!("{stage} (server is answering)"),
            Some(false) => {
                format!("{stage} (server not answering, still waiting)")
            },
            None => stage,
        })
    }
}

/// Draw when something changes and once per `tick`, however fast stages
/// arrive, until the sender is gone or the reader is.
fn draw(
    stages: &Receiver<Stage>,
    output: &mut impl Write,
    style: Style,
    tick: Duration,
) {
    let started = Instant::now();
    let mut shown = Shown::default();
    let mut next_tick = started + tick;

    loop {
        let wait = next_tick.saturating_duration_since(Instant::now());
        let changed = match stages.recv_timeout(wait) {
            Ok(stage) => shown.update(stage),
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        let ticked = Instant::now() >= next_tick;
        if ticked {
            next_tick += tick;
        }
        let text = shown.text(started.elapsed());
        let Some(text) = text.filter(|_| changed || ticked) else {
            continue;
        };
        if write_line(output, style, &text).is_err() {
            // Nobody is reading: keep draining, stop drawing.
            while stages.recv().is_ok() {}
            return;
        }
    }

    if style == Style::OneLine && shown.stage.is_some() {
        let _ = write!(output, "{CLEAR_LINE}");
        let _ = output.flush();
    }
}

fn write_line(
    output: &mut impl Write,
    style: Style,
    text: &str,
) -> io::Result<()> {
    match style {
        Style::Lines => writeln!(output, "progress\t{text}")?,
        Style::OneLine => write!(output, "{CLEAR_LINE}tmite: {text}")?,
    }
    output.flush()
}

fn describe(stage: Stage, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    match stage {
        Stage::Asking | Stage::Connection { .. } => {
            format!("asking the model… {seconds}s")
        },
        Stage::Writing { chars } => {
            format!("the model is writing, {chars} characters… {seconds}s")
        },
        Stage::Ranking => format!("ranking the commands… {seconds}s"),
        Stage::Checking => format!("checking it is safe… {seconds}s"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    use termite::Stage;

    use super::{Style, describe, draw};

    const NEVER: Duration = Duration::from_secs(3600);

    fn drawn(stages: &[Stage], style: Style) -> String {
        let (sender, receiver) = mpsc::channel();
        for stage in stages {
            sender.send(*stage).expect("receiver is here");
        }
        drop(sender);

        let mut output = Vec::new();
        draw(&receiver, &mut output, style, NEVER);
        String::from_utf8(output).expect("our text is UTF-8")
    }

    #[test]
    fn given_a_burst_of_stages_then_each_kind_is_drawn_once() {
        let stages = [
            Stage::Asking,
            Stage::Writing { chars: 1 },
            Stage::Writing { chars: 2 },
            Stage::Writing { chars: 3 },
            Stage::Ranking,
        ];

        let lines = drawn(&stages, Style::Lines);

        assert_eq!(
            lines,
            "progress\tasking the model… 0s\n\
             progress\tthe model is writing, 1 characters… 0s\n\
             progress\tranking the commands… 0s\n"
        );
    }

    #[test]
    fn given_one_line_style_then_the_line_is_cleared_at_the_end() {
        let line = drawn(&[Stage::Checking], Style::OneLine);
        assert_eq!(line, "\r\x1b[2Ktmite: checking it is safe… 0s\r\x1b[2K");
        assert_eq!(drawn(&[], Style::OneLine), "");
    }

    #[test]
    fn given_connection_checks_then_the_line_says_how_the_server_is() {
        let stages = [
            Stage::Asking,
            Stage::Connection { answering: true },
            Stage::Connection { answering: true },
            Stage::Connection { answering: false },
            Stage::Ranking,
        ];

        assert_eq!(
            drawn(&stages, Style::Lines),
            "progress\tasking the model… 0s\n\
             progress\tasking the model… 0s (server is answering)\n\
             progress\tasking the model… 0s (server not answering, still waiting)\n\
             progress\tranking the commands… 0s\n"
        );
    }

    #[test]
    fn given_time_passing_then_the_elapsed_seconds_are_shown() {
        let text = describe(Stage::Asking, Duration::from_secs(12));
        assert_eq!(text, "asking the model… 12s");
    }

    #[test]
    fn given_a_quiet_stage_then_each_tick_redraws_it() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Stage::Asking).expect("receiver is here");
        let waiter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(sender);
        });

        let mut output = Vec::new();
        draw(
            &receiver,
            &mut output,
            Style::Lines,
            Duration::from_millis(10),
        );
        waiter.join().expect("thread ends");

        let lines = output.iter().filter(|byte| **byte == b'\n').count();
        assert!(lines >= 3, "{lines} lines");
    }

    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn given_a_reader_that_left_then_drawing_stops_quietly() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Stage::Asking).expect("receiver is here");
        drop(sender);
        draw(&receiver, &mut Closed, Style::Lines, NEVER);
    }
}
