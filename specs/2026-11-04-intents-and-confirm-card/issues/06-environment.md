# 06 — Environment detection and the container rules

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 03
Spec: [../spec.md](../spec.md) (section 5)

## Details

- `src/environment.rs`: detect container (`/.dockerenv`, `/run/.containerenv`, `container` variable, `/proc/1/cgroup`, an overlay root in `/proc/self/mountinfo`), SSH, CI (`CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, `BUILDKITE`, `JENKINS_URL`, …), root (`geteuid() == 0`), and VM (`systemd-detect-virt` when present). File reads go through a root path, so tests use fixture folders.
- `Context` gains the environment; it goes into the decision state and the card.
- Fixed instructions per environment (ticket 03's module).
- Container rules: the Docker/Podman socket paths, `/proc/1/root`, `/proc/*/ns`, `nsenter`, `mount`, `/sys/fs/cgroup/*/release_agent`, `insmod`/`modprobe`, `chroot` are an output flag in a container (withheld when combined with a write).
- CI and agents (`CLAUDECODE`, `CLAUDE_CODE_ENTRYPOINT`, `CODEX_SANDBOX`, `CURSOR_TRACE_ID`, `AIDER_*`, `GEMINI_CLI`, `TERMITE_AGENT=1`, or no terminal): levels 1, 3, 4 and 5 fail (spec section 2). Root: `run` off.

## Tests first

- AC8: fixture roots for each container marker, and none; variables for SSH and CI; root via an injected uid.
- The container escape list as a table test of commands.
