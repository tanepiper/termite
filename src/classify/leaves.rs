//! Commands that can carry data off this machine. This never moves a level,
//! so a person's commands are treated as before. It only decides what an
//! agent may have, because nobody reads an agent's commands first.

use super::parse::Stage;
use super::programs::{FETCHERS, LANGUAGES, SENDERS, SHELLS, decodes};
use super::walk::Walk;

/// Programs that reach another machine by their nature. A name looked up in
/// DNS or a word in a URL can carry data, so a lookup counts as well.
const REACHES_OUT: &[&str] = &[
    "aria2c",
    "at",
    "aws",
    "az",
    "azcopy",
    "batch",
    "dig",
    "drill",
    "fetch",
    "ftp",
    "gcloud",
    "host",
    "http",
    "https",
    "links",
    "lynx",
    "mail",
    "mailx",
    "mosh",
    "msmtp",
    "mtr",
    "mutt",
    "nc",
    "ncat",
    "netcat",
    "nslookup",
    "open",
    "osascript",
    "pbcopy",
    "pbpaste",
    "ping",
    "ping6",
    "rclone",
    "s3cmd",
    "scp",
    "sendmail",
    "sftp",
    "socat",
    "ssh",
    "ssh-keyscan",
    "telnet",
    "tracepath",
    "traceroute",
    "w3m",
    "whois",
    "wl-copy",
    "wl-paste",
    "xclip",
    "xdg-open",
    "xh",
    "xsel",
];

/// Names that hardly ever appear in a command except to run them, so finding
/// one inside another command's words counts.
const NETWORK_TOOLS: &[&str] = &[
    "curl", "dig", "nc", "ncat", "netcat", "nslookup", "rclone", "scp", "sftp",
    "socat", "ssh", "telnet", "wget",
];

/// Characters that end a word inside another command's text.
const WORD_ENDS: &[char] = &[
    ' ', '\t', '\n', '\'', '"', '`', ';', '|', '&', '(', ')', '{', '}', '=',
    ',', '!',
];

/// Options that run another program.
const RUNS_A_PROGRAM: &[(&str, &[&str])] = &[
    ("gpg", &["--keyserver", "--send-key"]),
    ("rsync", &["--rsh", "-e"]),
    (
        "tar",
        &[
            "--checkpoint-action",
            "--rsh-command",
            "--to-command",
            "--use-compress-program",
        ],
    ),
];

/// A subcommand that uploads or publishes.
const PUBLISHES: &[(&str, &[&str])] = &[
    ("cargo", &["publish"]),
    ("docker", &["login"]),
    ("docker", &["push"]),
    ("docker", &["run"]),
    ("gem", &["push"]),
    ("gh", &["api"]),
    ("gh", &["gist"]),
    ("gh", &["release", "upload"]),
    ("gsutil", &[]),
    ("kubectl", &["cp"]),
    ("kubectl", &["exec"]),
    ("kubectl", &["run"]),
    ("npm", &["publish"]),
    ("pnpm", &["publish"]),
    ("openssl", &["s_client"]),
    ("podman", &["push"]),
    ("podman", &["run"]),
    ("twine", &["upload"]),
    ("yarn", &["publish"]),
];

/// `curl` options that can't send anything anywhere else.
const LOCAL_CURL_OPTIONS: &[&str] = &[
    "-I",
    "-S",
    "-f",
    "-fsS",
    "-i",
    "-s",
    "-sS",
    "-v",
    "--fail",
    "--head",
    "--include",
    "--show-error",
    "--silent",
    "--verbose",
];

const LOOPBACK: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// Git settings that choose where data goes.
const GIT_DESTINATIONS: &[&str] = &[
    "alias.",
    "askpass",
    "credential.",
    "editor",
    "external",
    "filter.",
    "fsmonitor",
    "hookspath",
    "insteadof",
    "pager",
    "pushurl",
    "sshcommand",
];

pub(super) const LEAVES: &str = "can carry data off the machine";

impl Walk<'_> {
    /// What this stage shows about the whole line.
    pub(super) fn notice(
        &mut self,
        name: &str,
        arguments: &[String],
        stage: &Stage,
    ) {
        // `ssh -i key host` uses a key and sends nothing, unless fed by a pipe.
        self.sends |=
            SENDERS.contains(&name) || (name == "ssh" && stage.piped_in);
        let redirected = stage.writes.iter().chain(&stage.reads);
        self.leaves |= leaves_machine(name, arguments)
            || redirected.into_iter().any(|path| is_network_path(path));
        let judged = matches!(name, "command" | "curl" | "echo" | "printf")
            || (name == "git"
                && arguments.first().is_some_and(|verb| verb == "commit"));
        self.names_network |= !judged && names_network(&stage.text);
        self.fetches |= FETCHERS.contains(&name) || decodes(name, arguments);
        let fed = stage.piped_in || stage.has_inner;
        let runs_code = SHELLS.contains(&name)
            || matches!(name, "source" | "." | "eval")
            || LANGUAGES.iter().any(|l| name.starts_with(l));
        self.runs_fed_code |= fed && runs_code;
    }
}

/// A web address that isn't this machine, or a network tool's name, anywhere
/// in the words. It only counts for a command that isn't a plain read.
fn names_network(text: &str) -> bool {
    let tool = text.split(WORD_ENDS).any(|word| {
        NETWORK_TOOLS.contains(&word.rsplit('/').next().unwrap_or(word))
    });
    tool || text.split(WORD_ENDS).any(|word| {
        word.split_once("://")
            .is_some_and(|(scheme, _)| scheme != "file" && !is_loopback(word))
    })
}

/// Can this stage move data to another machine?
pub(super) fn leaves_machine(name: &str, arguments: &[String]) -> bool {
    let runs_a_program = RUNS_A_PROGRAM.iter().any(|(program, options)| {
        *program == name
            && arguments
                .iter()
                .any(|word| options.iter().any(|o| word.starts_with(o)))
    });
    runs_a_program || leaves_by_name(name, arguments)
}

fn leaves_by_name(name: &str, arguments: &[String]) -> bool {
    match name {
        "curl" => !only_loopback(arguments),
        "wget" => true,
        "rsync" => arguments.iter().any(|word| names_a_host(word)),
        "git" => git_leaves(arguments),
        _ => {
            REACHES_OUT.contains(&name)
                || PUBLISHES.iter().any(|(program, verb)| {
                    *program == name && starts_with(arguments, verb)
                })
        },
    }
}

/// A write or read through bash's network paths.
pub(super) fn is_network_path(path: &str) -> bool {
    path.starts_with("/dev/tcp/") || path.starts_with("/dev/udp/")
}

fn starts_with(arguments: &[String], verb: &[&str]) -> bool {
    let words = arguments.iter().filter(|word| !word.starts_with('-'));
    verb.iter().zip(words).filter(|(a, b)| *a == b).count() == verb.len()
}

/// `curl` with only plain options, and every address on this machine.
fn only_loopback(arguments: &[String]) -> bool {
    !arguments.is_empty()
        && arguments.iter().all(|word| {
            if word.starts_with('-') {
                LOCAL_CURL_OPTIONS.contains(&word.as_str())
            } else {
                is_loopback(word)
            }
        })
}

fn is_loopback(address: &str) -> bool {
    let rest = address
        .strip_prefix("http://")
        .or_else(|| address.strip_prefix("https://"))
        .unwrap_or(address);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    };
    LOOPBACK.contains(&host)
}

/// `host:path` or `user@host:path`, but not `./a:b`.
fn names_a_host(word: &str) -> bool {
    !word.starts_with(['-', '.', '/'])
        && word
            .split_once(':')
            .is_some_and(|(host, _)| !host.contains('/'))
}

fn git_leaves(arguments: &[String]) -> bool {
    let mut words = arguments.iter().map(String::as_str);
    let sets_destination = arguments.iter().any(|word| {
        let lower = word.to_lowercase();
        GIT_DESTINATIONS.iter().any(|key| lower.contains(key))
    });
    let verb = words.find(|word| !word.starts_with('-'));
    let others: Vec<&str> = words.filter(|w| !w.starts_with('-')).collect();
    let by_option =
        |prefix: &str| arguments.iter().any(|word| word.starts_with(prefix));
    sets_destination
        || match verb {
            Some("push") => {
                by_option("--repo")
                    || by_option("--receive-pack")
                    || others.first().is_some_and(|to| is_url(to))
            },
            Some("remote") => {
                matches!(others.first(), Some(&"add" | &"set-url"))
            },
            Some("send-email") => true,
            Some("archive") => by_option("--remote"),
            _ => false,
        }
}

/// Anything that isn't a plain remote name such as `origin`.
fn is_url(word: &str) -> bool {
    word.contains([':', '/', '@'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(command: &str) -> bool {
        let words: Vec<String> =
            command.split_whitespace().map(str::to_owned).collect();
        leaves_machine(&words[0], &words[1..])
    }

    #[test]
    fn given_a_way_off_the_machine_then_it_leaves() {
        for command in [
            "curl https://example.test",
            "curl localhost.example.test",
            "curl http://localhost@example.test",
            "curl -x example.test http://localhost",
            "curl -K settings http://localhost",
            "curl -d @notes http://127.0.0.1 https://example.test",
            "wget http://localhost",
            "rsync -a . me@example.test:backup",
            "dig example.test",
            "aws s3 cp notes s3://bucket/",
            "gh release upload v1 notes",
            "git push https://example.test/x.git main",
            "git push git@example.test:x.git",
            "git push --repo=https://example.test/x.git",
            "git remote add out https://example.test/x.git",
            "git config remote.origin.pushurl https://example.test/x.git",
            "git -c core.sshCommand=other push origin",
            "npm publish",
            "pbcopy",
        ] {
            assert!(leaves(command), "{command}");
        }
    }

    #[test]
    fn given_work_that_stays_here_then_it_does_not_leave() {
        for command in [
            "curl http://localhost:3000/health",
            "curl -sS http://127.0.0.1:8080",
            "curl -i http://[::1]:8080/x?y=1",
            "rsync -a src/ backup/",
            "rsync -a ./a:b backup/",
            "git push origin main",
            "git remote -v",
            "git status",
            "npm install",
            "cargo build",
            "docker ps",
            "gh pr view",
        ] {
            assert!(!leaves(command), "{command}");
        }
    }
}
