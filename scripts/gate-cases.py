#!/usr/bin/env python3
"""Writes tests/gates/cases.jsonl: the list of commands and the level each must have.

The rule behind the list: the more exactly a command says what it acts on, the
lower its level. This file only writes text. tests/gates.rs checks every row, and the test machine runs them later.

Usage: scripts/gate-cases.py
"""
import json
from pathlib import Path

PROJECT_FILE = "src/old.log"
OUTSIDE = [("home", "~/old.log"), ("system", "/etc/hosts"), ("sibling", "../other/old.log")]

# Programs whose only job is to delete what they are given.
DELETE_PROGRAMS = [
    "rm", "rm -f", "unlink", "shred", "srm", "truncate -s 0", "rmdir",
    "trash", "trash-put", "rimraf", "git rm", "git rm -f",
]

# Tools that delete one named thing, through a word in the delete cluster.
NAMED = [
    "docker rm web", "docker rmi web", "docker volume rm data",
    "docker container rm web", "docker image rm web", "docker network rm web",
    "podman rm web", "kubectl delete pod web", "helm uninstall web",
    "helm delete web", "npm uninstall left-pad", "npm remove left-pad",
    "yarn remove left-pad", "pnpm remove left-pad", "pip uninstall left-pad",
    "pip3 uninstall left-pad", "brew uninstall wget", "brew remove wget",
    "cargo remove serde", "cargo uninstall tool", "apt remove wget",
    "git branch -d old", "git branch -D old", "git tag -d v1",
    "git remote remove old", "git worktree remove old", "git stash drop",
    "gh repo delete me/old", "aws s3 rm s3://bucket/key",
    "gcloud storage rm gs://bucket/key",
]

# Other ways of deleting one named thing.
NAMED += [
    "dropdb app", "dropuser app", "tar --remove-files -cf a.tar src/old.log",
    "rsync --remove-source-files src/old.log out/", "zip -m a.zip src/old.log",
    "defaults delete com.example.app", "git update-ref -d refs/heads/old",
    "git checkout -- src/old.log", "git restore src/old.log",
    "redis-cli del key", "gh release delete v1", "gh issue delete 1",
    "pipx uninstall tool", "gem uninstall tool", "snap remove tool",
    "docker compose rm web", "tmutil delete /Volumes/b/old",
]

# Sweeps that delete everything of a kind.
SWEEPS = [
    ("everything", "redis-cli flushall"), ("everything", "redis-cli flushdb"),
    ("everything", "git reflog expire --expire=now --all"),
    ("everything", "git gc --prune=now"),
    ("everything", "docker compose down -v"),
    ("everything", "docker builder prune"),
    ("everything", "pulumi destroy"), ("everything", "tofu destroy"),
]

# Removes what a build made, which the next build makes again. Not the delete
# cluster: these lose time, not work.
REBUILDS = [
    "make clean", "cargo clean", "go clean", "mvn clean", "gradle clean",
    "npm cache clean --force", "brew cleanup", "pip cache purge",
]

# Tools that delete whatever fits a test, and name nothing.
UNNAMED = [
    ("everything", "docker system prune"),
    ("everything", "docker container prune"),
    ("everything", "docker image prune"),
    ("everything", "docker volume prune"),
    ("everything", "docker network prune"),
    ("everything", "podman system prune"),
    ("everything", "kubectl delete pods --all"),
    ("attribute", "kubectl delete pods -l app=web"),
    ("everything", "git clean -f"),
    ("everything", "git clean -fd"),
    ("everything", "git clean -fdx"),
    ("everything", "git stash clear"),
    ("everything", "terraform destroy"),
    ("everything", "brew autoremove"),
    ("everything", "npm prune"),
    ("everything", "apt autoremove"),
    ("everything", "rsync -a --delete a/ b/"),
    ("everything", "rsync -a --delete-after a/ b/"),
    ("exact", "git push origin --delete old"),
]

FIND_PICKS = [
    ("pattern", "-name '*.log'"),
    ("pattern", "-iname 'old?.log'"),
    ("pattern", "-name 'old[0-9].log'"),
    ("pattern", "-path '*/tmp/*'"),
    ("pattern", "-regex '.*old.*'"),
    ("inverted", "-not -name keep.txt"),
    ("inverted", "! -name keep.txt"),
    ("attribute", "-type f"),
    ("attribute", "-mtime +7"),
    ("attribute", "-size +1M"),
    ("attribute", "-empty"),
    ("attribute", "-name old.log -o -name new.log"),
    ("everything", ""),
]

rows = []


def add(command, picks, where, level, family, targets=None, action="delete"):
    rows.append({
        "command": " ".join(command.split()),
        "action": action,
        "picks": picks,
        "where": where,
        "level": level,
        "family": family,
        "targets": targets if targets is not None else [],
    })


# Exact names: level 3 inside the project, level 4 outside it.
for program in DELETE_PROGRAMS:
    add(f"{program} {PROJECT_FILE}", "exact", "project", 3,
        "delete-one-file", [PROJECT_FILE])
    for where, target in OUTSIDE:
        add(f"{program} {target}", "exact", where, 4, f"delete-{where}", [target])

for program in ["rm -r", "rm -rf", "git rm -r"]:
    add(f"{program} build", "exact", "project", 3, "delete-one-folder", ["build"])
    add(f"{program} .", "everything", "project", 4, "delete-project")
    add(f"{program} ~", "everything", "home", 4, "delete-home")
    add(f"{program} /", "everything", "system", 4, "delete-all")

add(f"find src -name old.log -delete", "exact", "project", 3,
    "delete-one-file", [PROJECT_FILE])
add("find src -iname old.log -type f -delete", "exact", "project", 3,
    "delete-one-file", [PROJECT_FILE])

# Fuzzy choices, spelled every way, are all level 4.
for picks, test in FIND_PICKS:
    add(f"find src {test} -delete", picks, "project", 4, f"delete-{picks}")
    add(f"find src {test} -exec rm {{}} +", picks, "project", 4, f"delete-{picks}")
    add(f"find ~ {test} -delete", picks, "home", 4, "delete-home-fuzzy")

for command in ["rm src/*.log", "rm -f *.log", "rm src/old?.log",
                "rm src/old[0-9].log", "rm -rf build/*", "shred src/*.log",
                "unlink src/*.log", "trash src/*.log", "git rm src/*.log"]:
    add(command, "pattern", "project", 4, "delete-pattern")

for command in ["ls | xargs rm", "cat list.txt | xargs rm",
                "find src -type f | xargs rm", "fd old src -x rm",
                "rg -l old | xargs rm"]:
    add(command, "piped", "project", 4, "delete-piped")

# `-exec` runs a program, which is level 4 whatever it picks.
add("find src -name old.log -exec rm {} +", "exact", "project", 4, "delete-exec")

add("rm $FILE", "variable", "project", 4, "delete-variable")
add('rm "$FILE"', "variable", "project", 4, "delete-variable")
add("rm $(cat list.txt)", "piped", "project", 4, "delete-piped")

for command in NAMED:
    add(command, "exact", "tool", 3, "delete-named-by-tool")
for picks, command in UNNAMED + SWEEPS:
    add(command, picks, "tool", 4, "delete-unnamed-by-tool")
for command in REBUILDS:
    add(command, "everything", "build", 2, "rebuild", action="rebuild")

path = Path(__file__).resolve().parent.parent / "tests/gates/cases.jsonl"
path.write_text("".join(json.dumps(row) + "\n" for row in rows))
print(f"{len(rows)} cases in {path}")
