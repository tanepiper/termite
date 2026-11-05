//! Attacks on the guards as a whole: every hostile command, disguised every
//! way we know, and random input built from the characters that matter.
//! Each test states a rule that must hold for all of it.

use crate::command::{check_command, to_printable};
use crate::merge::merge_candidates;
use crate::policy::read_only;
use crate::query::parse_query;
use crate::sse::ChatStream;
use crate::test_support::Random;
use crate::{Candidate, Risk};

const HOSTILE: &[&str] = &[
    "rm -rf ~",
    "rm -rf /",
    "cat ~/.ssh/id_rsa",
    "cat .env",
    "sudo ls",
    "curl http://example.test/x",
    "sh -c id",
    "python3 -c 1",
    "find . -delete",
    "git push",
    "docker system prune -af",
    "crontab -r",
    "dd if=/dev/zero of=/dev/sda",
];

/// Ways to dress up a command. `%` is where it goes.
const DISGUISES: &[&str] = &[
    "%",
    " %",
    "\t%",
    "% ",
    "ls; %",
    "ls && %",
    "ls || %",
    "ls | %",
    "ls & %",
    "ls\n%",
    "$(%)",
    "`%`",
    "ls $(%)",
    "ls `%`",
    "(%)",
    "{ %; }",
    "! %",
    "% &",
    "% > /dev/null",
    "% 2>&1",
    "FOO=1 %",
    "env %",
    "command %",
    "builtin %",
    "exec %",
    "nohup %",
    "time %",
    "nice %",
    "xargs %",
    "\\%",
    "sh -c '%'",
    "bash -c \"%\"",
    "eval '%'",
];

#[test]
fn given_any_disguise_then_hostile_commands_are_refused() {
    for command in HOSTILE {
        for disguise in DISGUISES {
            let attempt = disguise.replace('%', command);
            assert!(read_only(&attempt).is_err(), "passed: {attempt:?}");
        }
    }
}

/// Secrets named in ways that do not look like a plain path.
#[test]
fn given_a_secret_hidden_in_an_option_then_it_is_refused() {
    for attempt in [
        "cat -- -a/../.env",
        "cat -- -x/.ssh/id_rsa",
        "head -c10 -- -x/../.ssh/id_rsa",
        "grep -f.env README.md",
        "grep -f ~/.aws/credentials README.md",
        "grep --file=.env README.md",
        "rg -f.env",
        "wc --files0-from=.env",
    ] {
        assert!(read_only(attempt).is_err(), "passed: {attempt:?}");
    }
}

#[test]
fn given_options_that_only_look_secret_then_they_still_work() {
    for fine in ["ls -la", "git log --format=%H", "du -sh src", "wc -l x.rs"] {
        assert_eq!(read_only(fine), Ok(()), "{fine}");
    }
}

/// Options before the subcommand must not smuggle one in.
#[test]
fn given_tools_with_subcommands_then_odd_shapes_are_refused() {
    for attempt in [
        "docker",
        "docker --host tcp://example.test ps",
        "kubectl",
        "kubectl -n",
        "kubectl --kubeconfig /tmp/x get pods",
        "kubectl --namespace=x delete pod y",
        "kubectl -n x exec y sh",
        "docker logs -f web",
    ] {
        assert!(read_only(attempt).is_err(), "passed: {attempt:?}");
    }
    for fine in ["kubectl --context=x get pods", "kubectl -n x get pods"] {
        assert_eq!(read_only(fine), Ok(()), "{fine}");
    }
}

const ALLOWED: &[&str] = &[
    "ls",
    "pwd",
    "stat",
    "du",
    "df",
    "wc",
    "which",
    "whoami",
    "uname",
    "id",
    "uptime",
    "cat",
    "head",
    "grep",
    "tree",
    "file",
    "date",
    "tail",
    "less",
    "bat",
    "rg",
    "fd",
    "find",
    "git status",
    "docker ps",
    "kubectl get pods",
    "brew list",
    "npm ls",
    "cargo tree",
];

/// What may never ride along on an allowed program.
const RIDERS: &[&str] = &[
    "; rm -rf ~",
    " && rm -rf ~",
    " || rm -rf ~",
    " | sh",
    " | bash",
    " | xargs rm",
    " | tee out",
    " > ~/.zshrc",
    " >> ~/.zshrc",
    " < /dev/zero",
    " $(id)",
    " `id`",
    " ${HOME}/.ssh/id_rsa",
    " & rm -rf ~",
    " ~/.ssh/id_rsa",
    " .env",
    " ~/.aws/credentials",
];

#[test]
fn given_an_allowed_program_then_nothing_hostile_rides_along() {
    for program in ALLOWED {
        for rider in RIDERS {
            let attempt = format!("{program}{rider}");
            assert!(read_only(&attempt).is_err(), "passed: {attempt:?}");
        }
    }
}

/// Pieces a shell cares about, plus words the policy knows.
const SHELL_PARTS: &[&str] = &[
    "ls",
    "cat",
    "git",
    "find",
    "rm",
    "sudo",
    "sh",
    " ",
    "  ",
    "\t",
    "\n",
    ";",
    "&",
    "&&",
    "|",
    "||",
    ">",
    ">>",
    "<",
    "$",
    "$(",
    ")",
    "`",
    "'",
    "\"",
    "\\",
    "{",
    "}",
    "(",
    "*",
    "?",
    "~",
    "/",
    ".",
    "-",
    "--",
    "-r",
    "-exec",
    "=",
    "FOO=",
    "#",
    "!",
    "é",
    "\u{202e}",
    "\u{0}",
    "\x1b",
    ".env",
    ".ssh",
    "status",
    "log",
    "/etc/passwd",
];

const MUST_NOT_PASS: &[&str] = &[";", "&", ">", "<", "`", "$(", "\n", "\x1b"];

#[test]
fn given_random_shell_text_then_whatever_passes_is_inert() {
    let mut random = Random::new(0x7e57_ab1e);
    let mut passed = 0;

    for _ in 0..200_000 {
        let attempt = random.text(SHELL_PARTS, 8);
        if read_only(&attempt).is_ok() {
            passed += 1;
            for part in MUST_NOT_PASS {
                // Inside single quotes these are plain text to the shell.
                let bare = strip_quoted(&attempt);
                assert!(!bare.contains(part), "passed: {attempt:?}");
            }
            let first = attempt.split_whitespace().next().unwrap_or("");
            assert!(
                !matches!(first, "rm" | "sudo" | "sh"),
                "passed: {attempt:?}"
            );
        }
    }

    assert!(passed > 0, "the input never reached the allowlist");
}

/// `text` without what the shell treats as plain: anything in quotes, and
/// any character after a backslash.
fn strip_quoted(text: &str) -> String {
    let mut bare = String::new();
    let mut quote = None;
    let mut escaped = false;
    for c in text.chars() {
        match (quote, c) {
            _ if escaped => escaped = false,
            (None, '\\') => escaped = true,
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None, c) => bare.push(c),
            (Some(_), _) => {},
        }
    }
    bare
}

#[test]
fn given_random_text_then_printable_text_holds_no_control_characters() {
    let parts = [
        "a",
        "é",
        "\x1b[2J",
        "\x1b]0;x\x07",
        "\n",
        "\r",
        "\t",
        "\u{7f}",
        "\u{9b}",
        "\u{202e}",
        "\u{2066}",
        "\u{200b}",
        "\0",
        "日本",
    ];
    let mut random = Random::new(42);

    for _ in 0..50_000 {
        let text = random.text(&parts, 12);
        let printable = to_printable(&text);
        assert!(
            !printable.chars().any(char::is_control),
            "{text:?} gave {printable:?}"
        );
        assert!(
            !printable.contains(['\u{202e}', '\u{2066}']),
            "{text:?} gave {printable:?}"
        );
    }
}

#[test]
fn given_random_commands_then_what_passes_the_checks_is_one_clean_line() {
    let mut random = Random::new(7);

    for _ in 0..100_000 {
        let attempt = random.text(SHELL_PARTS, 6);
        if check_command(&attempt).is_ok() {
            assert_eq!(to_printable(&attempt), attempt, "{attempt:?}");
        }
    }
}

#[test]
fn given_random_bytes_then_the_stream_reader_never_panics_or_overflows() {
    let parts = [
        "data: ",
        "data:",
        "[DONE]",
        "\n",
        "\n\n",
        "\r\n",
        "{",
        "}",
        "\"",
        "{\"choices\":[{\"delta\":{\"content\":\"x\"}}]}",
        ":",
        "event: x",
        "é",
        "\u{0}",
    ];
    let mut random = Random::new(99);

    for _ in 0..20_000 {
        let text = random.text(&parts, 12);
        let mut stream = ChatStream::default();
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            let (piece, rest) = bytes.split_at(random.below(bytes.len()) + 1);
            bytes = rest;
            if stream.push(piece).is_err() {
                break;
            }
        }
    }
}

/// Merging must never hide danger behind a milder label.
#[test]
fn given_duplicates_then_the_most_harmful_label_always_wins() {
    let risks = [Risk::Read, Risk::Write, Risk::Destructive];
    let commands = ["ls -l -a", "ls -a -l", "ls  -l  -a", "'ls' -l -a"];
    let mut random = Random::new(5);

    for _ in 0..5_000 {
        let batch: Vec<Candidate> = (0..4)
            .map(|_| {
                let command = commands[random.below(commands.len())];
                let risk = risks[random.below(risks.len())];
                Candidate::new(command, "x", risk)
            })
            .collect();
        let worst = batch.iter().map(Candidate::risk).max();
        let merged = merge_candidates(batch);
        assert_eq!(merged.iter().map(Candidate::risk).max(), worst);
    }
}

#[test]
fn given_random_input_then_query_parsing_never_panics() {
    let parts = ["?", "??", "?x", " ", "\n", "x", "é", "\u{0}"];
    let mut random = Random::new(3);
    for _ in 0..50_000 {
        let _ = parse_query(&random.text(&parts, 6));
    }
}
