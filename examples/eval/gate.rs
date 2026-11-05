//! Where is this running, and may it test a model here? Every model test
//! asks first. Each check tries the real thing or reads the real machine, so
//! setting a variable can't talk its way past. Anything unclear is a no.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use termite::{Isolation, isolation};

/// Keys that must never be visible to a model test.
const SECRET_VARIABLES: [&str; 3] = [
    "HUGGINGFACE_API",
    "SONARQUBE_TOKEN",
    "SONARQUBE_ADMIN_TOKEN",
];
const SOURCE: &str = "src";
const SECRETS_FILE: &str = ".env";

/// How much separation a job needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// At least the sandbox: safe work, and scoring models.
    Sandbox,
    /// A real container or virtual machine: dangerous work.
    Isolated,
}

/// What the checks found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub isolation: Isolation,
    pub source_locked: bool,
    pub secrets_away: bool,
    pub internet_blocked: bool,
}

impl Report {
    pub fn read() -> Self {
        Self {
            isolation: isolation(),
            source_locked: source_locked(),
            secrets_away: secrets_away(),
            internet_blocked: internet_blocked(),
        }
    }

    /// What's stopping `need`, in words. Empty means go.
    pub fn missing(&self, need: Need) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if need == Need::Isolated && self.isolation == Isolation::None {
            missing.push("this isn't a container or a virtual machine");
        }
        if !self.source_locked {
            missing.push("the source folder can be written to");
        }
        if !self.secrets_away {
            missing.push("secrets can be read from here");
        }
        if !self.internet_blocked {
            missing.push("the internet can be reached from here");
        }
        missing
    }
}

/// Stop unless this place is good enough for `need`.
pub fn require(need: Need) -> Result<(), String> {
    let missing = Report::read().missing(need);
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "refused: {}. Use scripts/team.sh, which runs this in the right place",
        missing.join(", ")
    ))
}

/// `eval gate [blue|red]`: print the checks. With a team, fail if it can't
/// work here.
pub fn run(words: &[String]) -> Result<(), String> {
    let report = Report::read();
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    println!("Running in {}", report.isolation.label());
    println!("{} Source folder locked", mark(report.source_locked));
    println!("{} Secrets out of reach", mark(report.secrets_away));
    println!("{} Internet blocked", mark(report.internet_blocked));
    let blue = report.missing(Need::Sandbox).is_empty();
    let red = report.missing(Need::Isolated).is_empty();
    println!("{} Blue team can work here", mark(blue));
    println!("{} Red team can work here", mark(red));
    match words.first().map(String::as_str) {
        None => Ok(()),
        Some("blue") if blue => Ok(()),
        Some("red") if red => Ok(()),
        Some(team @ ("blue" | "red")) => {
            Err(format!("{team} team can't work here"))
        },
        Some(other) => Err(format!("unknown team {other}")),
    }
}

/// Try to create a file in the source. Only a refusal counts: a missing
/// folder proves nothing.
fn source_locked() -> bool {
    let probe = format!("{SOURCE}/.termite-gate-{}", std::process::id());
    // Empty, and removed at once. Nothing reads it.
    // nosemgrep: termite-files-are-written-privately
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            fs::remove_file(&probe).ok();
            false
        },
        Err(error) => matches!(
            error.kind(),
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
        ),
    }
}

fn secrets_away() -> bool {
    fs::File::open(SECRETS_FILE).is_err()
        && SECRET_VARIABLES
            .iter()
            .all(|name| std::env::var_os(name).is_none())
}

/// Try TCP, then UDP. Blocking one and not the other isn't blocked.
fn internet_blocked() -> bool {
    let address = SocketAddr::from(([1, 1, 1, 1], 443));
    TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_err()
        && !udp_answers()
}

/// Ask a public name server for a name that doesn't exist. Any answer at all
/// means UDP gets out.
fn udp_answers() -> bool {
    const QUESTION: &[u8] =
        b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x04gate\x04test\x00\x00\x01\x00\x01";
    let Ok(socket) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return false;
    };
    socket.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut reply = [0_u8; 512];
    socket.send_to(QUESTION, ("1.1.1.1", 53)).is_ok()
        && socket.recv(&mut reply).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(isolation: Isolation) -> Report {
        Report {
            isolation,
            source_locked: true,
            secrets_away: true,
            internet_blocked: true,
        }
    }

    #[test]
    fn given_the_sandbox_then_blue_may_work_and_red_may_not() {
        let sandbox = report(Isolation::None);

        assert!(sandbox.missing(Need::Sandbox).is_empty());
        assert_eq!(sandbox.missing(Need::Isolated).len(), 1);
    }

    #[test]
    fn given_a_locked_down_container_or_vm_then_red_may_work() {
        for place in [Isolation::Container, Isolation::VirtualMachine] {
            assert!(report(place).missing(Need::Isolated).is_empty());
        }
    }

    #[test]
    fn given_any_gap_then_being_in_a_container_is_not_enough() {
        let gaps = [
            Report {
                source_locked: false,
                ..report(Isolation::Container)
            },
            Report {
                secrets_away: false,
                ..report(Isolation::Container)
            },
            Report {
                internet_blocked: false,
                ..report(Isolation::Container)
            },
        ];
        for gap in gaps {
            assert_eq!(gap.missing(Need::Isolated).len(), 1, "{gap:?}");
            assert_eq!(gap.missing(Need::Sandbox).len(), 1, "{gap:?}");
        }
    }

    #[test]
    fn given_a_bare_host_then_nothing_may_work() {
        let host = Report {
            isolation: Isolation::None,
            source_locked: false,
            secrets_away: false,
            internet_blocked: false,
        };

        assert_eq!(host.missing(Need::Sandbox).len(), 3);
        assert_eq!(host.missing(Need::Isolated).len(), 4);
    }
}
