//! `confine --write DIR -- COMMAND`: run a command that can't reach the
//! internet and can only write under the folders named. For hosts that won't
//! let a job cut its own network, such as Hugging Face Jobs.
//!
//! Landlock stops every TCP connection and bind, and every write outside
//! `--write`. It doesn't cover UDP, so a seccomp filter leaves only TCP and
//! Unix sockets. Together that leaves Unix sockets, which is how the model
//! server and the eval tool talk. Both last across `exec` and into every child,
//! and neither can be undone. If either can't be applied, nothing runs.
//!
//! This is its own tool, and not part of the eval tool, because the evaluation
//! never runs anything. It only ever starts the command in the job's script,
//! never something a model wrote.

use std::process::ExitCode;

fn main() -> ExitCode {
    // Words from the job's own script; they are only compared and passed on.
    let words: Vec<String> = std::env::args().skip(1).collect();
    match run(&words) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        },
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run(words: &[String]) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let (writable, command) = read_words(words)?;
    let (program, arguments) =
        command.split_first().ok_or("confine: no command given")?;
    linux::landlock(&writable)?;
    linux::seccomp()?;
    let error = Command::new(program).args(arguments).exec();
    Err(format!("confine: {program}: {error}"))
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run(_words: &[String]) -> Result<(), String> {
    Err("confine needs Linux on x86-64".to_owned())
}

/// Split `--write DIR ... -- COMMAND`.
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]
fn read_words(words: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let mut writable = Vec::new();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--write" => writable
                .push(words.next().ok_or("--write needs a folder")?.clone()),
            "--" => return Ok((writable, words.cloned().collect())),
            other => return Err(format!("confine: unknown option {other}")),
        }
    }
    Err("confine: no command given".to_owned())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux {
    use std::ffi::{CString, c_void};
    use std::io::Error;

    const CREATE_RULESET: libc::c_long = 444;
    const ADD_RULE: libc::c_long = 445;
    const RESTRICT_SELF: libc::c_long = 446;
    const GET_VERSION: libc::c_uint = 1;
    const RULE_PATH_BENEATH: libc::c_uint = 1;
    /// The first version that can restrict TCP.
    const NEEDS_VERSION: libc::c_long = 4;

    /// Every right that changes a file: all but execute, read file, read dir.
    const WRITES: u64 = 0x7FF2;
    const TCP_BIND_AND_CONNECT: u64 = 3;

    #[repr(C)]
    struct Ruleset {
        handled_fs: u64,
        handled_net: u64,
    }

    #[repr(C, packed)]
    struct PathBeneath {
        allowed: u64,
        parent_fd: i32,
    }

    fn failed(step: &str) -> String {
        format!("confine: {step}: {}", Error::last_os_error())
    }

    fn no_new_privileges() -> Result<(), String> {
        // Safe: a plain flag with no pointers.
        let result =
            unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if result == 0 {
            Ok(())
        } else {
            Err(failed("no_new_privs"))
        }
    }

    pub fn landlock(writable: &[String]) -> Result<(), String> {
        // Safe: a null attribute with this flag only asks for the version.
        let version = unsafe {
            libc::syscall(
                CREATE_RULESET,
                std::ptr::null::<c_void>(),
                0,
                GET_VERSION,
            )
        };
        if version < NEEDS_VERSION {
            return Err(format!(
                "confine: Landlock version {version}, needs {NEEDS_VERSION}"
            ));
        }
        let ruleset = Ruleset {
            handled_fs: WRITES,
            handled_net: TCP_BIND_AND_CONNECT,
        };
        // Safe: the pointer and size describe `ruleset`.
        let fd = unsafe {
            libc::syscall(
                CREATE_RULESET,
                &raw const ruleset,
                size_of::<Ruleset>(),
                0,
            )
        };
        if fd < 0 {
            return Err(failed("create ruleset"));
        }
        for folder in writable {
            allow_writes(fd, folder)?;
        }
        no_new_privileges()?;
        // Safe: `fd` is the ruleset made above.
        if unsafe { libc::syscall(RESTRICT_SELF, fd, 0) } != 0 {
            return Err(failed("restrict"));
        }
        Ok(())
    }

    fn allow_writes(ruleset: libc::c_long, folder: &str) -> Result<(), String> {
        let path = CString::new(folder).map_err(|e| e.to_string())?;
        // Safe: `path` is a valid C string for the length of the call.
        let parent_fd = unsafe {
            libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC)
        };
        if parent_fd < 0 {
            return Err(failed(folder));
        }
        let rule = PathBeneath {
            allowed: WRITES,
            parent_fd,
        };
        // Safe: the pointer describes `rule`, and `parent_fd` is open.
        let result = unsafe {
            libc::syscall(
                ADD_RULE,
                ruleset,
                RULE_PATH_BENEATH,
                &raw const rule,
                0,
            )
        };
        // Safe: `parent_fd` is ours, and the rule no longer needs it.
        unsafe { libc::close(parent_fd) };
        if result == 0 {
            Ok(())
        } else {
            Err(failed(folder))
        }
    }

    /// Allow only Unix and TCP sockets, and no `io_uring`, which could open
    /// sockets without calling `socket`.
    pub fn seccomp() -> Result<(), String> {
        const LOAD: u16 = 0x20;
        const IF_EQUAL: u16 = 0x15;
        const IF_AT_LEAST: u16 = 0x35;
        const AND: u16 = 0x54;
        const RETURN: u16 = 0x06;
        const ALLOW: u32 = 0x7FFF_0000;
        const REFUSE: u32 = 0x0005_0001;
        const KILL: u32 = 0x8000_0000;
        const X86_64: u32 = 0xC000_003E;
        const X32_CALLS: u32 = 0x4000_0000;
        const IO_URING_SETUP: u32 = 425;
        const SOCKET: u32 = 41;
        // Offsets in `seccomp_data`.
        const NUMBER: u32 = 0;
        const ARCH: u32 = 4;
        const FIRST_ARGUMENT: u32 = 16;
        const SECOND_ARGUMENT: u32 = 24;
        // The low bits of the type; the rest are flags.
        const TYPE_MASK: u32 = 0xF;

        let step = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
        // Each jump counts the steps to skip.
        let mut steps = [
            step(LOAD, 0, 0, ARCH),
            step(IF_EQUAL, 1, 0, X86_64),
            step(RETURN, 0, 0, KILL),
            step(LOAD, 0, 0, NUMBER),
            step(IF_AT_LEAST, 11, 0, X32_CALLS),
            step(IF_EQUAL, 10, 0, IO_URING_SETUP),
            step(IF_EQUAL, 1, 0, SOCKET),
            step(RETURN, 0, 0, ALLOW),
            step(LOAD, 0, 0, FIRST_ARGUMENT),
            step(IF_EQUAL, 5, 0, libc::AF_UNIX as u32),
            step(IF_EQUAL, 1, 0, libc::AF_INET as u32),
            step(IF_EQUAL, 0, 4, libc::AF_INET6 as u32),
            step(LOAD, 0, 0, SECOND_ARGUMENT),
            step(AND, 0, 0, TYPE_MASK),
            step(IF_EQUAL, 0, 1, libc::SOCK_STREAM as u32),
            step(RETURN, 0, 0, ALLOW),
            step(RETURN, 0, 0, REFUSE),
        ];
        let program = libc::sock_fprog {
            len: steps.len() as u16,
            filter: steps.as_mut_ptr(),
        };
        no_new_privileges()?;
        // Safe: `program` points at `steps`, which outlive the call.
        let result = unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER,
                &raw const program,
                0,
                0,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(failed("seccomp"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_words;

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn given_folders_and_a_command_then_both_are_read() {
        let (writable, command) =
            read_words(&words("--write /a --write /b -- sh -c x")).unwrap();

        assert_eq!(writable, ["/a", "/b"]);
        assert_eq!(command, ["sh", "-c", "x"]);
    }

    #[test]
    fn given_no_separator_then_nothing_runs() {
        assert!(read_words(&words("--write /a")).is_err());
    }
}
