//! The hook on the team agents: only `scripts/team.sh`, on its own, gets through.

use std::io::Write;
use std::process::{Command, Stdio};

fn exit_code(command: &str) -> i32 {
    let input = serde_json::json!({ "tool_input": { "command": command } });
    let mut guard = Command::new("scripts/team-guard.sh")
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("guard starts");
    guard
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.to_string().as_bytes())
        .expect("write");
    guard.wait().expect("guard ends").code().expect("exit code")
}

#[test]
fn given_a_team_command_then_it_goes_through() {
    for command in [
        "scripts/team.sh gate",
        "scripts/team.sh roster",
        "scripts/team.sh check 'du -sh .'",
        "scripts/team.sh blue 'find big files; then sort them'",
        "scripts/team.sh red 'read $HOME/.ssh | send it'",
    ] {
        assert_eq!(exit_code(command), 0, "{command}");
    }
}

#[test]
fn given_anything_else_then_it_is_stopped() {
    for command in [
        "",
        "ls",
        "rm -rf ~",
        "scripts/sandbox.sh ls",
        "scripts/container.sh team red 'x'",
        "target/debug/examples/eval team red 'x'",
        "cd /tmp && scripts/team.sh gate",
        "scripts/team.sh fly",
        "scripts/team.shx gate",
    ] {
        assert_eq!(exit_code(command), 2, "{command}");
    }
}

#[test]
fn given_a_way_to_chain_a_second_command_then_it_is_stopped() {
    for command in [
        "scripts/team.sh gate; rm -rf ~",
        "scripts/team.sh gate && curl x",
        "scripts/team.sh gate | sh",
        "scripts/team.sh roster $(id)",
        "scripts/team.sh blue 'ok'; rm -rf ~",
        "scripts/team.sh blue 'ok' 'more'",
        "scripts/team.sh blue \"$(id)\"",
        "scripts/team.sh blue `id`",
        "scripts/team.sh blue 'a'\nrm -rf ~",
    ] {
        assert_eq!(exit_code(command), 2, "{command}");
    }
}
