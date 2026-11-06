//! What this machine can run. Contract: only reads. The only programs it
//! starts are `sysctl` and `nvidia-smi`, to ask how much memory there is.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use super::installed::find_on_path;

/// The graphics hardware a model can run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gpu {
    /// Apple silicon: the GPU shares main memory.
    Unified,
    /// A card with its own memory, in bytes.
    Card {
        memory: u64,
    },
    None,
}

/// The parts of a machine that decide which models fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Machine {
    pub(crate) os: String,
    pub(crate) arch: String,
    /// Main memory in bytes, or 0 when it could not be read.
    pub(crate) memory: u64,
    pub(crate) gpu: Gpu,
}

impl Machine {
    pub(crate) fn detect() -> Self {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH.to_owned();
        match os {
            "macos" => Self {
                os: "macos".to_owned(),
                gpu: if arch == "aarch64" {
                    Gpu::Unified
                } else {
                    Gpu::None
                },
                memory: mac_memory(),
                arch,
            },
            _ => Self {
                os: os.to_owned(),
                arch,
                memory: fs::read_to_string("/proc/meminfo")
                    .ok()
                    .and_then(|text| parse_meminfo(&text))
                    .unwrap_or(0),
                gpu: linux_gpu(),
            },
        }
    }

    /// Bytes models may use, leaving room for everything else. A card counts
    /// only if it beats what the processor could use.
    pub(crate) fn model_budget(&self) -> u64 {
        let main = self.memory / 10 * 6;
        match self.gpu {
            Gpu::Card { memory } => (memory / 10 * 9).max(self.memory / 2),
            Gpu::Unified => main,
            Gpu::None => self.memory / 2,
        }
    }

    /// How to get llama.cpp here.
    pub(crate) fn llama_advice(&self) -> &'static str {
        if self.os == "macos" {
            "Install llama.cpp (`brew install llama.cpp`), then run `tmite setup` again."
        } else {
            "Install llama.cpp from your package manager or from \
             https://github.com/ggml-org/llama.cpp/releases, then run \
             `tmite setup` again."
        }
    }

    pub(crate) fn report(&self) -> Value {
        let gpu = match self.gpu {
            Gpu::Unified => json!({ "kind": "unified" }),
            Gpu::Card { memory } => json!({ "kind": "card", "memory": memory }),
            Gpu::None => json!({ "kind": "none" }),
        };
        json!({
            "os": self.os,
            "arch": self.arch,
            "memory": self.memory,
            "gpu": gpu,
            "model_budget": self.model_budget(),
        })
    }
}

/// Asks the system's own `sysctl`, named in full so `PATH` can't change it.
fn mac_memory() -> u64 {
    Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout).trim().parse().ok()
        })
        .unwrap_or(0)
}

/// `MemTotal` from `/proc/meminfo`, in bytes.
pub(crate) fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes * 1024)
}

/// The largest card in `nvidia-smi` output, one number of MiB per line.
pub(crate) fn parse_nvidia_memory(text: &str) -> Option<u64> {
    text.lines()
        .filter_map(|line| line.trim().parse::<u64>().ok())
        .max()
        .map(|mebibytes| mebibytes * 1024 * 1024)
}

fn linux_gpu() -> Gpu {
    nvidia_memory()
        .or_else(|| amd_memory(Path::new("/sys/class/drm")))
        .map_or(Gpu::None, |memory| Gpu::Card { memory })
}

fn nvidia_memory() -> Option<u64> {
    let tool = find_on_path("nvidia-smi")?;
    let output = Command::new(tool)
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .and_then(|text| parse_nvidia_memory(&text))
}

/// The largest AMD card, which reports its memory in `sysfs`.
pub(crate) fn amd_memory(drm: &Path) -> Option<u64> {
    fs::read_dir(drm)
        .ok()?
        .flatten()
        .filter_map(|card| {
            let file = card.path().join("device/mem_info_vram_total");
            fs::read_to_string(file).ok()?.trim().parse::<u64>().ok()
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::{Gpu, Machine, parse_meminfo, parse_nvidia_memory};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn machine(os: &str, memory: u64, gpu: Gpu) -> Machine {
        Machine {
            os: os.to_owned(),
            arch: "aarch64".to_owned(),
            memory,
            gpu,
        }
    }

    #[test]
    fn given_meminfo_then_total_memory_is_read() {
        let text = "MemFree: 12 kB\nMemTotal:       16384000 kB\n";
        assert_eq!(parse_meminfo(text), Some(16384000 * 1024));
        assert_eq!(parse_meminfo("nothing"), None);
    }

    #[test]
    fn given_several_cards_then_the_largest_counts() {
        assert_eq!(parse_nvidia_memory("8192\n24576\n"), Some(24 * GIB));
        assert_eq!(parse_nvidia_memory("[N/A]\n"), None);
        assert_eq!(parse_nvidia_memory(""), None);
    }

    #[test]
    fn given_hardware_then_the_budget_fits_it() {
        let apple = machine("macos", 32 * GIB, Gpu::Unified);
        let cpu = machine("linux", 32 * GIB, Gpu::None);
        let card = machine("linux", 16 * GIB, Gpu::Card { memory: 24 * GIB });
        let small_card =
            machine("linux", 64 * GIB, Gpu::Card { memory: 4 * GIB });

        assert!(apple.model_budget() > cpu.model_budget());
        assert!(card.model_budget() > 20 * GIB);
        assert_eq!(small_card.model_budget(), 32 * GIB);
    }

    #[test]
    fn given_no_memory_reading_then_the_budget_is_nothing() {
        assert_eq!(machine("linux", 0, Gpu::None).model_budget(), 0);
    }

    #[test]
    fn this_machine_can_be_read_without_changing_anything() {
        let found = Machine::detect();
        assert!(!found.os.is_empty());
        assert!(found.report()["model_budget"].is_u64());
    }

    #[test]
    fn advice_fits_the_system() {
        assert!(
            machine("macos", 0, Gpu::None)
                .llama_advice()
                .contains("brew")
        );
        assert!(
            !machine("linux", 0, Gpu::None)
                .llama_advice()
                .contains("brew")
        );
    }
}
