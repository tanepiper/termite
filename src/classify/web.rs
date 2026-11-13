//! Requests that can't carry data to another machine: `curl` to this
//! machine, and `gh api` when it only reads from the GitHub host that `gh`
//! is signed in to. Any other address can receive data in the address
//! itself, so `curl` to another machine always counts as leaving.

/// `curl` options that send nothing but the request for the address.
const PLAIN_OPTIONS: &[&str] = &[
    "-I",
    "-L",
    "-S",
    "-f",
    "-fsS",
    "-fsSL",
    "-i",
    "-s",
    "-sL",
    "-sS",
    "-sSL",
    "-v",
    "--compressed",
    "--fail",
    "--head",
    "--include",
    "--location",
    "--show-error",
    "--silent",
    "--verbose",
];

/// `curl` options that take a value and send nothing with it.
const PLAIN_VALUES: &[&str] = &[
    "-m",
    "-w",
    "--connect-timeout",
    "--max-time",
    "--retry",
    "--write-out",
];

/// `curl` options that send what they are given, which is local work when
/// every address is on this machine.
const SENDS_A_VALUE: &[&str] =
    &["-H", "-X", "-d", "--data", "--header", "--request"];

const LOOPBACK: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// `curl` with known options, no file written, and every address on this
/// machine.
pub(super) fn curl_stays_here(arguments: &[String]) -> bool {
    let mut words = arguments.iter();
    let mut addresses = 0;
    while let Some(word) = words.next() {
        let takes_value = PLAIN_VALUES.contains(&word.as_str())
            || SENDS_A_VALUE.contains(&word.as_str());
        let plain = if takes_value {
            words.next().is_some()
        } else if word == "-o" || word == "--output" {
            words.next().is_some_and(|file| file == "/dev/null")
        } else if word.starts_with('-') {
            PLAIN_OPTIONS.contains(&word.as_str())
        } else {
            addresses += 1;
            is_loopback(word)
        };
        if !plain {
            return false;
        }
    }
    addresses > 0
}

/// `gh api` that only reads: no method but GET, and no field or file, since
/// either one makes it a request that changes something.
pub(super) fn gh_api_only_reads(arguments: &[String]) -> bool {
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        let reads = match word.as_str() {
            "-X" | "--method" => words
                .next()
                .is_some_and(|verb| verb.eq_ignore_ascii_case("get")),
            "-q" | "--jq" | "-t" | "--template" => words.next().is_some(),
            "--paginate" | "--slurp" | "-i" | "--include" | "--silent" => true,
            option if option.starts_with('-') => false,
            address => is_written_out(address) && address != "graphql",
        };
        if !reads {
            return false;
        }
    }
    true
}

/// Nothing in the word is only known when the command runs.
fn is_written_out(word: &str) -> bool {
    !word.contains(['$', '`', '*', '{', '@'])
}

pub(super) fn is_loopback(address: &str) -> bool {
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
