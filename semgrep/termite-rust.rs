// Cases for termite-rust.yaml. Not compiled.

fn prints() {
    // ruleid: termite-library-never-prints
    println!("hello");
    // ruleid: termite-library-never-prints
    eprintln!("oops {}", 1);
    // ruleid: termite-library-never-prints
    dbg!(value);
    // ok: termite-library-never-prints
    let text = format!("hello");
}

fn starts_programs() {
    // ruleid: termite-only-setup-starts-programs
    let child = std::process::Command::new("sh").arg("-c").spawn();
    // ruleid: termite-only-setup-starts-programs
    let child = Command::new("rm");
    // ok: termite-only-setup-starts-programs
    let words = shell_words::split("rm -rf /");
}

fn writes_files() {
    // ruleid: termite-files-are-written-privately
    fs::write(&path, text);
    // ruleid: termite-files-are-written-privately
    let file = File::create(&path);
    // ruleid: termite-files-are-written-privately
    let file = OpenOptions::new().write(true).open(&path);
    // ok: termite-files-are-written-privately
    write_private(home.root(), "config.json", bytes, PRIVATE_FILE_MODE);
}

fn uses_unsafe() {
    // ruleid: termite-unsafe-only-in-home
    unsafe { libc::getuid() };
}

fn reads_the_token() {
    // ruleid: termite-decision-token-read-only-in-config
    let token = std::env::var("TERMITE_DECISION_TOKEN");
    // ruleid: termite-decision-token-read-only-in-config
    let token = env::var(DECISION_TOKEN_VAR);
    // ok: termite-decision-token-read-only-in-config
    let token = decision_token();
}

fn shows_the_token() {
    let token = decision_token();
    // ruleid: termite-decision-token-never-shown, termite-library-never-prints
    println!("token: {:?}", token);
    let header = token.unwrap();
    // ruleid: termite-decision-token-never-shown, termite-library-never-prints
    eprintln!("{}", header);
    // ok: termite-decision-token-never-shown
    let request = request.bearer_auth(header);
    // ruleid: termite-decision-token-not-in-format-strings, termite-library-never-prints
    println!("sent {token:?}");
    // ok: termite-decision-token-not-in-format-strings
    let label = format!("{tokens} tokens");
}

fn http_clients() {
    // ruleid: termite-http-client-has-a-timeout
    let client = reqwest::Client::new();
    // ruleid: termite-http-client-has-a-timeout
    let client = reqwest::Client::builder().user_agent("tmite").build();
    // ok: termite-http-client-has-a-timeout
    let client = reqwest::Client::builder().timeout(LIMIT).build();
    // ok: termite-http-client-has-a-timeout
    let client = reqwest::Client::builder()
        .connect_timeout(LIMIT)
        .timeout(LIMIT)
        .build();
}
