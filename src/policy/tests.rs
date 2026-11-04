use super::{PolicyRefusal, read_only};
use crate::command::CommandRejected;

fn not_allowed(program: &str) -> PolicyRefusal {
    PolicyRefusal::ProgramNotAllowed {
        program: program.into(),
    }
}

fn subcommand(program: &str, subcommand: &str) -> PolicyRefusal {
    PolicyRefusal::SubcommandNotAllowed {
        program: program.into(),
        subcommand: subcommand.into(),
    }
}

fn option(program: &str, option: &str) -> PolicyRefusal {
    PolicyRefusal::OptionNotAllowed {
        program: program.into(),
        option: option.into(),
    }
}

fn never_ends(program: &str, option: &str) -> PolicyRefusal {
    PolicyRefusal::NeverEnds {
        program: program.into(),
        option: option.into(),
    }
}

fn secret(path: &str) -> PolicyRefusal {
    PolicyRefusal::ReadsSecret { path: path.into() }
}

#[test]
fn given_a_read_only_command_when_checked_then_it_is_allowed() {
    let commands = [
        // Programs allowed with any options.
        "ls -la ~/code",
        "pwd",
        "tree -L 2 src",
        "stat Cargo.toml",
        "file README.md",
        "du -sh ~/code",
        "df -h",
        "wc -l src/lib.rs",
        "which cargo",
        "whoami",
        "uname -a",
        "date +%Y-%m-%d",
        "date",
        "id",
        "uptime",
        // Readers.
        "cat Cargo.toml",
        "head -n 20 src/lib.rs",
        "tail -n 50 build.log",
        "less README.md",
        "bat src/main.rs",
        // Search.
        "grep -rn TODO src",
        "rg -i 'fn main' --pre-glob '*.gz'",
        "fd -e rs src",
        "find . -name '*.rs' -type f",
        // git.
        "git status",
        "git log --oneline -10",
        "git diff HEAD~1",
        "git show HEAD",
        "git branch",
        "git branch -a -v",
        "git remote -v",
        "git rev-parse HEAD",
        "git blame src/lib.rs",
        "git ls-files",
        "git -C ~/code/app status",
        "git --no-pager log -5",
        // docker.
        "docker ps -a",
        "docker images",
        "docker inspect web",
        "docker logs --tail 100 web",
        "docker version",
        "docker info",
        // kubectl.
        "kubectl get pods -A",
        "kubectl -n prod get deployments",
        "kubectl describe pod web-1",
        "kubectl logs web-1 --tail 50",
        "kubectl version",
        "kubectl config current-context",
        // Package tools.
        "brew list",
        "brew info jq",
        "npm list",
        "npm outdated",
        "cargo --version",
        "cargo info serde",
        // Pipes where every stage passes.
        "ls ~/code | wc -l",
        "git log --oneline | head -5",
        "docker ps | grep web | wc -l",
        // Quoted shell characters are plain text.
        "ls ';rm -rf ~'",
        "grep 'a|b' src/lib.rs",
        "grep \"x > y\" notes.txt",
    ];

    for command in commands {
        assert_eq!(read_only(command), Ok(()), "{command:?}");
    }
}

#[test]
fn given_a_shell_feature_when_checked_then_it_is_refused() {
    let cases = [
        ("ls ;rm -rf ~", PolicyRefusal::ChainsCommands),
        ("ls; rm x", PolicyRefusal::ChainsCommands),
        ("ls && rm x", PolicyRefusal::ChainsCommands),
        ("ls || rm x", PolicyRefusal::ChainsCommands),
        ("ls & rm x", PolicyRefusal::ChainsCommands),
        ("ls |& wc", PolicyRefusal::ChainsCommands),
        ("ls > out.txt", PolicyRefusal::Redirects),
        ("ls >> out.txt", PolicyRefusal::Redirects),
        ("ls >| out.txt", PolicyRefusal::Redirects),
        ("cat < ~/.ssh/id_rsa", PolicyRefusal::Redirects),
        ("cat << EOF", PolicyRefusal::Redirects),
        ("ls 2>/dev/null", PolicyRefusal::Redirects),
        ("ls $(pwd)", PolicyRefusal::RunsBuiltCode),
        ("ls \"$(pwd)\"", PolicyRefusal::RunsBuiltCode),
        ("ls `pwd`", PolicyRefusal::RunsBuiltCode),
        ("ls \"`pwd`\"", PolicyRefusal::RunsBuiltCode),
        ("cat <(ls)", PolicyRefusal::Redirects),
        ("diff =(ls) b", PolicyRefusal::RunsBuiltCode),
        ("(ls)", PolicyRefusal::RunsBuiltCode),
        ("ls *(e:'rm x':)", PolicyRefusal::RunsBuiltCode),
        ("eval ls", PolicyRefusal::RunsBuiltCode),
        ("source ~/.zshrc", PolicyRefusal::RunsBuiltCode),
        (". ~/.zshrc", PolicyRefusal::RunsBuiltCode),
        ("exec ls", PolicyRefusal::RunsBuiltCode),
        ("ls | xargs rm", PolicyRefusal::RunsBuiltCode),
        ("ls !!", PolicyRefusal::UsesHistory),
        ("ls \"!rm\"", PolicyRefusal::UsesHistory),
        ("ls $'\\x2essh'", PolicyRefusal::EncodedText),
        ("ls $\"x\"", PolicyRefusal::EncodedText),
        ("FOO=bar ls", PolicyRefusal::SetsVariables),
        ("GIT_PAGER=sh git log", PolicyRefusal::SetsVariables),
        (
            "ls | tee out.txt",
            PolicyRefusal::WritesFiles {
                program: "tee".into(),
            },
        ),
        (
            "sudo ls",
            PolicyRefusal::RaisesPrivileges {
                program: "sudo".into(),
            },
        ),
        (
            "doas ls",
            PolicyRefusal::RaisesPrivileges {
                program: "doas".into(),
            },
        ),
        (
            "su -c ls",
            PolicyRefusal::RaisesPrivileges {
                program: "su".into(),
            },
        ),
        (
            "pkexec ls",
            PolicyRefusal::RaisesPrivileges {
                program: "pkexec".into(),
            },
        ),
        (
            "ls\nrm x",
            PolicyRefusal::Unusable(CommandRejected::MultipleLines),
        ),
        ("ls |", PolicyRefusal::Unusable(CommandRejected::Empty)),
        ("| wc", PolicyRefusal::Unusable(CommandRejected::Empty)),
    ];

    for (command, expected) in cases {
        assert_eq!(read_only(command), Err(expected), "{command:?}");
    }
}

#[test]
fn given_a_program_rule_when_broken_then_it_is_refused() {
    let cases = [
        ("rm -rf build", not_allowed("rm")),
        ("ls | sh", not_allowed("sh")),
        ("./ls", not_allowed("./ls")),
        ("/bin/rm x", not_allowed("/bin/rm")),
        ("env ls", not_allowed("env")),
        ("echo hi", not_allowed("echo")),
        ("tree -o out.txt", option("tree", "-o")),
        ("file -C -m magic", option("file", "-C")),
        ("date -s 12:00", option("date", "-s")),
        ("date 010112002026", option("date", "010112002026")),
        ("tail -f build.log", never_ends("tail", "-f")),
        ("tail -nF 5 build.log", never_ends("tail", "-nF")),
        ("tail --follow=name x", never_ends("tail", "--follow=name")),
        ("tail --fol x", never_ends("tail", "--fol")),
        ("less -o log.txt README.md", option("less", "-o")),
        ("less '+!rm x' README.md", option("less", "+!rm x")),
        ("bat --pager=sh x", option("bat", "--pager=sh")),
        ("rg --pre sh x", option("rg", "--pre")),
        ("fd -x rm", option("fd", "-x")),
        ("fd -HX rm", option("fd", "-HX")),
        ("fd --exec rm", option("fd", "--exec")),
        ("find . -delete", option("find", "-delete")),
        ("find . -exec rm {} +", option("find", "-exec")),
        ("find . -execdir rm {} +", option("find", "-execdir")),
        ("find . -ok rm {} ;", PolicyRefusal::ChainsCommands),
        ("find . -okdir rm {} +", option("find", "-okdir")),
        ("find . -fprint out", option("find", "-fprint")),
        ("find . -fprintf out x", option("find", "-fprintf")),
        ("find . -fls out", option("find", "-fls")),
        ("git push", subcommand("git", "push")),
        ("git checkout main", subcommand("git", "checkout")),
        ("git", subcommand("git", "")),
        ("git -c core.pager=sh log", option("git", "-c")),
        (
            "git --exec-path=/tmp log",
            option("git", "--exec-path=/tmp"),
        ),
        (
            "git diff --output=x.patch",
            option("git", "--output=x.patch"),
        ),
        ("git branch -D main", option("git", "-D")),
        ("git branch new-feature", option("git", "new-feature")),
        ("git remote add x y", option("git", "add")),
        ("docker rm web", subcommand("docker", "rm")),
        ("docker restart web", subcommand("docker", "restart")),
        ("docker -H tcp://x ps", option("docker", "-H")),
        ("docker logs -f web", never_ends("docker", "-f")),
        ("docker logs --follow web", never_ends("docker", "--follow")),
        ("kubectl delete pod web", subcommand("kubectl", "delete")),
        ("kubectl apply -f x.yaml", subcommand("kubectl", "apply")),
        ("kubectl get secrets", option("kubectl", "secrets")),
        (
            "kubectl get secret/db -o yaml",
            option("kubectl", "secret/db"),
        ),
        (
            "kubectl get pods,secrets",
            option("kubectl", "pods,secrets"),
        ),
        ("kubectl get pods -w", never_ends("kubectl", "-w")),
        ("kubectl logs -f web", never_ends("kubectl", "-f")),
        (
            "kubectl config use-context prod",
            subcommand("kubectl", "config"),
        ),
        ("kubectl --token=x get pods", option("kubectl", "--token=x")),
        ("brew install jq", subcommand("brew", "install")),
        ("npm install", subcommand("npm", "install")),
        ("npm", subcommand("npm", "")),
        ("cargo build", subcommand("cargo", "build")),
        ("cargo --version --list", option("cargo", "--list")),
    ];

    for (command, expected) in cases {
        assert_eq!(read_only(command), Err(expected), "{command:?}");
    }
}

#[test]
fn given_a_secret_path_when_read_then_it_is_refused() {
    let cases = [
        ("cat ~/.ssh/id_ed25519", "~/.ssh/id_ed25519"),
        ("ls /Users/me/.ssh", "/Users/me/.ssh"),
        ("cat $HOME/.ssh/config", "$HOME/.ssh/config"),
        ("cat ~/.ss''h/id_rsa", "~/.ssh/id_rsa"),
        ("cat server.pem", "server.pem"),
        ("cat tls/private.key", "tls/private.key"),
        ("cat .env", ".env"),
        ("cat app/.env.local", "app/.env.local"),
        ("cat ~/.aws/credentials", "~/.aws/credentials"),
        ("cat ~/.config/gh/hosts.yml", "~/.config/gh/hosts.yml"),
        ("cat ~/.netrc", "~/.netrc"),
        ("cat ~/.docker/config.json", "~/.docker/config.json"),
        ("cat vault.kdbx", "vault.kdbx"),
        ("cat ~/.s*/id_rsa", "~/.s*/id_rsa"),
        ("grep -r token ~/.aws", "~/.aws"),
        ("bat --file-name=x ~/.netrc", "~/.netrc"),
        ("grep --file=~/.ssh/config x", "~/.ssh/config"),
        ("ls | grep x .env", ".env"),
    ];

    for (command, path) in cases {
        assert_eq!(read_only(command), Err(secret(path)), "{command:?}");
    }
}

#[test]
fn given_a_lookalike_of_a_secret_path_when_read_then_it_is_allowed() {
    let commands = [
        "cat ~/code/sshd.md",
        "cat keys.txt",
        "cat docs/environment.md",
        "ls ~/.config/nvim",
        "cat src/key.rs",
        "ls *.rs",
    ];

    for command in commands {
        assert_eq!(read_only(command), Ok(()), "{command:?}");
    }
}
