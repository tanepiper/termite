//! Requests that only read. A plain fetch brings data to this machine, and
//! the only thing it sends is the address, which is written in the command
//! for anyone to read. A request that carries a body, a header, a file or
//! a value that is only known when it runs can carry data away, so it is
//! not a plain fetch unless every address is on this machine.

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

/// `curl` that only fetches: known options, no file written, and every
/// address written out in full. A request that sends a body, a header or
/// a method counts only when every address is on this machine.
pub(super) fn curl_only_reads(arguments: &[String]) -> bool {
    let mut words = arguments.iter();
    let mut addresses = Vec::new();
    let mut sends = false;
    while let Some(word) = words.next() {
        let plain = if PLAIN_VALUES.contains(&word.as_str()) {
            words.next().is_some()
        } else if SENDS_A_VALUE.contains(&word.as_str()) {
            sends = true;
            words.next().is_some()
        } else if word == "-o" || word == "--output" {
            words.next().is_some_and(|file| file == "/dev/null")
        } else if word.starts_with('-') {
            PLAIN_OPTIONS.contains(&word.as_str())
        } else {
            addresses.push(word.as_str());
            true
        };
        if !plain {
            return false;
        }
    }
    let local = addresses.iter().all(|one| is_loopback(one));
    let written_out = addresses.iter().all(|one| is_written_out(one));
    !addresses.is_empty() && (local || (!sends && written_out))
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
