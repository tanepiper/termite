use super::*;

const KEY: &[u8] = b"a test key, not a secret";

struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let folder = std::env::temp_dir()
            .join(format!("termite-log-{tag}-{}", std::process::id()));
        // Left over from an earlier failed run.
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("folder");
        Self(folder)
    }

    fn log(&self) -> Log {
        Log::at(&self.0)
    }

    fn file(&self) -> PathBuf {
        self.0.join(PROVENANCE_FILE)
    }

    fn rewrite(&self, change: impl Fn(&mut Vec<String>)) {
        let text = fs::read_to_string(self.file()).expect("log");
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        change(&mut lines);
        fs::write(self.file(), lines.join("\n") + "\n").expect("rewrite");
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // A test that failed may leave it behind; nothing depends on it.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn installed(path: &str, bytes: &[u8]) -> Record {
    Record {
        event: Event::Installed,
        path: path.to_owned(),
        sha256: sha256_hex(bytes),
        flags: Vec::new(),
        by: "test".to_owned(),
        verdict: None,
    }
}

fn three(tag: &str) -> Temp {
    let temp = Temp::new(tag);
    for name in ["one", "two", "three"] {
        temp.log()
            .append(KEY, installed(&format!("bin/{name}"), name.as_bytes()))
            .expect("appends");
    }
    temp
}

#[test]
fn given_known_text_then_the_hash_matches_the_published_value() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn given_entries_then_the_chain_verifies_and_the_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let temp = three("good");

    assert_eq!(temp.log().verify(KEY).expect("verifies"), 3);
    let mode = fs::metadata(temp.file()).expect("log").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn given_no_log_then_it_verifies_as_empty() {
    assert_eq!(Temp::new("empty").log().verify(KEY).expect("verifies"), 0);
}

#[test]
fn given_a_removed_edited_or_moved_entry_then_verify_names_the_line() {
    type Change = fn(&mut Vec<String>);
    let cases: [(&str, Change, usize); 4] = [
        ("removed", |lines| drop(lines.remove(1)), 2),
        (
            "edited",
            |lines| lines[1] = lines[1].replace("bin/two", "bin/owt"),
            2,
        ),
        ("moved", |lines| lines.swap(0, 1), 1),
        ("garbled", |lines| lines[2] = "not json".to_owned(), 3),
    ];

    for (name, change, expected) in cases {
        let temp = three(name);
        temp.rewrite(change);

        let error = temp.log().verify(KEY).expect_err(name);

        let ProvenanceError::Broken { line, .. } = error else {
            panic!("{name}: {error}");
        };
        assert_eq!(line, expected, "{name}");
    }
}

#[test]
fn given_the_wrong_key_then_verify_fails() {
    let temp = three("key");

    assert!(temp.log().verify(b"another key").is_err());
}

#[test]
fn given_a_forged_entry_with_a_good_chain_then_the_signature_catches_it() {
    let temp = three("forged");
    let text = fs::read_to_string(temp.file()).expect("log");
    let last = text.lines().last().expect("line");
    let forged = Entry {
        seq: 4,
        prev: sha256_hex(last.as_bytes()),
        mac: "00".repeat(32),
        ..temp.log().entries().expect("entries")[2].clone()
    };
    let line = serde_json::to_string(&forged).expect("json");
    temp.rewrite(|lines| lines.push(line.clone()));

    assert!(temp.log().verify(KEY).is_err());
}

#[test]
fn given_a_broken_log_then_nothing_is_added() {
    let temp = three("append");
    temp.rewrite(|lines| drop(lines.remove(0)));

    assert!(temp.log().append(KEY, installed("bin/x", b"x")).is_err());
}

#[test]
fn given_files_then_their_status_follows_the_last_entry() {
    let temp = three("status");
    let log = temp.log();

    let status = |path, bytes: &[u8]| log.status(path, bytes).expect("status");
    assert_eq!(status("bin/one", b"one"), FileStatus::Unchanged);
    assert_eq!(status("bin/one", b"edited"), FileStatus::Changed);
    assert_eq!(status("bin/other", b"x"), FileStatus::Unknown);

    let mut accepted = installed("bin/one", b"edited");
    accepted.event = Event::ChangedAccepted;
    log.append(KEY, accepted).expect("appends");
    assert_eq!(
        log.status("bin/one", b"edited").expect("status"),
        FileStatus::Unchanged
    );

    let mut removed = installed("bin/one", b"edited");
    removed.event = Event::Removed;
    log.append(KEY, removed).expect("appends");
    assert_eq!(
        log.status("bin/one", b"edited").expect("status"),
        FileStatus::Unknown
    );
}

#[test]
fn given_alerts_then_no_request_text_is_stored() {
    let temp = Temp::new("alert");
    for request in ["delete my secret-project", "ünïcode reqüest", "a\nb", "x"]
    {
        let record = Record::alert(
            request,
            vec!["wrecks".to_owned()],
            "withheld",
            "test",
        );
        temp.log().append(KEY, record).expect("appends");

        let text = fs::read_to_string(temp.file()).expect("log");
        assert!(!text.contains(request), "{request}");
    }
}

#[test]
fn given_seconds_then_the_time_is_a_utc_date() {
    for (seconds, expected) in [
        (0, "1970-01-01T00:00:00Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (1_793_906_400, "2026-11-05T19:20:00Z"),
    ] {
        assert_eq!(utc_time(seconds), expected);
    }
}
