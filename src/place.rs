//! Is this process in a container or a virtual machine? Reads a few files
//! and, on macOS, one kernel flag. It says what it finds and decides nothing:
//! callers set their own rules. Anything unclear counts as not isolated.

use std::fs;
use std::path::Path;

/// What separates this process from the machine underneath.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isolation {
    /// Nothing found: treat it as the real machine.
    None,
    VirtualMachine,
    Container,
}

impl Isolation {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "the host machine",
            Self::VirtualMachine => "a virtual machine",
            Self::Container => "a container",
        }
    }
}

/// Variables that mean an agent or CI started this. The list can only grow.
const UNATTENDED: [&str; 14] = [
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "BUILDKITE",
    "JENKINS_URL",
    "CIRCLECI",
    "TF_BUILD",
    "TEAMCITY_VERSION",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_SANDBOX",
    "CURSOR_TRACE_ID",
    "GEMINI_CLI",
    "TERMITE_AGENT",
];

/// Is nobody there to answer a question? True for any of the variables, set
/// to anything, any `AIDER_` variable, or no terminal. Nothing can turn this
/// off: an agent that hides its variables still has no terminal.
pub fn unattended<I, S>(variables: I, has_terminal: bool) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    !has_terminal
        || variables.into_iter().any(|name| {
            let name = name.as_ref();
            UNATTENDED.contains(&name) || name.starts_with("AIDER_")
        })
}

/// Words that name a container in `/proc/1/cgroup`.
const CONTAINER_GROUPS: [&str; 4] =
    ["docker", "containerd", "kubepods", "libpod"];

/// Words that name a hypervisor in the machine's firmware details.
const VIRTUAL_VENDORS: [&str; 8] = [
    "qemu",
    "kvm",
    "vmware",
    "virtualbox",
    "xen",
    "parallels",
    "virtual machine",
    "apple virtualization",
];

/// Look at this machine.
pub fn isolation() -> Isolation {
    isolation_under(Path::new("/"), hypervisor_flag())
}

/// Look under `root`, given what the kernel's own flag says. A container
/// wins, because Docker Desktop runs containers inside a virtual machine.
pub fn isolation_under(root: &Path, hypervisor: bool) -> Isolation {
    let read = |file: &str| {
        fs::read_to_string(root.join(file))
            .unwrap_or_default()
            .to_lowercase()
    };
    let marked = root.join(".dockerenv").exists()
        || root.join("run/.containerenv").exists();
    let cgroup = read("proc/1/cgroup");
    if marked || CONTAINER_GROUPS.iter().any(|word| cgroup.contains(word)) {
        return Isolation::Container;
    }

    let firmware = format!(
        "{} {}",
        read("sys/class/dmi/id/sys_vendor"),
        read("sys/class/dmi/id/product_name")
    );
    let cpu_flagged = read("proc/cpuinfo")
        .lines()
        .any(|line| line.starts_with("flags") && line.contains(" hypervisor"));
    if hypervisor
        || cpu_flagged
        || VIRTUAL_VENDORS.iter().any(|word| firmware.contains(word))
    {
        return Isolation::VirtualMachine;
    }
    Isolation::None
}

/// macOS sets `kern.hv_vmm_present` to 1 inside a virtual machine.
#[cfg(target_os = "macos")]
fn hypervisor_flag() -> bool {
    let mut value: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>();
    // SAFETY: the name is a valid C string, `value` and `size` outlive the
    // call, and `size` is the size of `value`.
    let status = unsafe {
        libc::sysctlbyname(
            c"kern.hv_vmm_present".as_ptr(),
            (&raw mut value).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    status == 0 && value == 1
}

#[cfg(not(target_os = "macos"))]
fn hypervisor_flag() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn given_each_variable_then_nobody_is_there() {
        for name in super::UNATTENDED {
            assert!(super::unattended([name], true), "{name}");
        }
        assert!(super::unattended(["AIDER_MODEL"], true));
    }

    #[test]
    fn given_no_terminal_then_nobody_is_there() {
        assert!(super::unattended(["HOME"], false));
    }

    #[test]
    fn given_a_terminal_and_no_variable_then_a_person_is_there() {
        assert!(!super::unattended(["HOME", "PATH"], true));
    }

    use std::path::PathBuf;

    use super::*;

    fn root(name: &str, files: &[(&str, &str)]) -> PathBuf {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let path = std::env::temp_dir()
            .join(format!("termite-place-{}-{name}", std::process::id()));
        for (file, text) in files {
            let file = path.join(file);
            fs::create_dir_all(file.parent().expect("parent")).expect("folder");
            // nosemgrep: termite-files-are-written-privately
            fs::write(file, text).expect("file");
        }
        fs::create_dir_all(&path).expect("folder");
        path
    }

    fn check(name: &str, files: &[(&str, &str)], flag: bool, want: Isolation) {
        let path = root(name, files);
        assert_eq!(isolation_under(&path, flag), want, "{name}");
        fs::remove_dir_all(path).ok();
    }

    #[test]
    fn given_nothing_then_it_is_the_host() {
        check("bare", &[], false, Isolation::None);
    }

    #[test]
    fn given_a_container_marker_then_it_is_a_container() {
        check("docker", &[(".dockerenv", "")], false, Isolation::Container);
        check(
            "podman",
            &[("run/.containerenv", "")],
            false,
            Isolation::Container,
        );
        check(
            "cgroup",
            &[("proc/1/cgroup", "0::/docker/abc\n")],
            false,
            Isolation::Container,
        );
    }

    #[test]
    fn given_a_hypervisor_then_it_is_a_virtual_machine() {
        check("flag", &[], true, Isolation::VirtualMachine);
        check(
            "cpu",
            &[("proc/cpuinfo", "flags : fpu hypervisor sse\n")],
            false,
            Isolation::VirtualMachine,
        );
        check(
            "firmware",
            &[("sys/class/dmi/id/sys_vendor", "QEMU\n")],
            false,
            Isolation::VirtualMachine,
        );
    }

    #[test]
    fn given_a_container_on_a_virtual_machine_then_the_container_wins() {
        check("both", &[(".dockerenv", "")], true, Isolation::Container);
    }

    #[test]
    fn given_ordinary_details_then_they_do_not_count() {
        check(
            "laptop",
            &[
                ("proc/1/cgroup", "0::/init.scope\n"),
                ("proc/cpuinfo", "flags : fpu sse\n"),
                ("sys/class/dmi/id/sys_vendor", "Framework\n"),
            ],
            false,
            Isolation::None,
        );
    }
}
