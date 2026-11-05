#!/usr/bin/env python3
"""The Mac side of the test bed. It carries text and writes down what happened.

It has no way to run a command: it only talks to two sockets. Whatever comes
back is hostile input, so it is size-limited, parsed strictly, and stripped of
control characters before it is stored or shown.

Usage: harness.py record|inspect|boots|report RUN_FOLDER [options]
Called by scripts/testbed.sh, which owns the machines.
"""
import argparse
import datetime
import json
import os
import re
import socket
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from preflight import compare, failures, severity  # noqa: E402

MAX_LINE = 4 * 1024 * 1024
MAX_TEXT = 8192
BOOT_SECONDS = 240
CONTROL = re.compile(r"[\x00-\x08\x0b-\x1f\x7f-\x9f]")


def clean(value):
    """Remove what a terminal would act on, at any depth."""
    if isinstance(value, str):
        return CONTROL.sub("", value)[:MAX_TEXT]
    if isinstance(value, list):
        return [clean(item) for item in value]
    if isinstance(value, dict):
        return {clean(key): clean(item) for key, item in value.items()}
    return value


def summary(fields):
    return " ".join(f"{key}={clean(value)}" for key, value in fields.items())[:150]


class Recorder:
    """Every event goes to disk as it happens, so a run can be replayed."""

    def __init__(self, folder):
        self.started = time.monotonic()
        self.file = open(folder / "events.jsonl", "a", encoding="utf-8")

    def write(self, kind, **fields):
        now = datetime.datetime.now(datetime.timezone.utc)
        entry = {
            "t": round(time.monotonic() - self.started, 2),
            "at": now.isoformat(timespec="milliseconds"),
            "kind": kind,
            **clean(fields),
        }
        self.file.write(json.dumps(entry) + "\n")
        self.file.flush()
        os.fsync(self.file.fileno())
        print(f"{entry['t']:8.2f}s  {kind}  {summary(fields)}", flush=True)


class Lines:
    """JSON lines from a socket, with a limit on every line."""

    def __init__(self, path, seconds):
        deadline = time.monotonic() + seconds
        while True:
            self.socket = socket.socket(socket.AF_UNIX)
            try:
                self.socket.connect(str(path))
                break
            except OSError:
                self.socket.close()
                if time.monotonic() > deadline:
                    raise TimeoutError(f"nothing is listening on {path.name}")
                time.sleep(0.2)
        self.buffer = b""

    def send(self, message):
        self.socket.sendall((json.dumps(message) + "\n").encode())

    def read(self, seconds):
        """The next message, or None if the time ran out."""
        deadline = time.monotonic() + seconds
        while b"\n" not in self.buffer:
            left = deadline - time.monotonic()
            if left <= 0:
                return None
            self.socket.settimeout(left)
            try:
                data = self.socket.recv(65536)
            except TimeoutError:
                return None
            if not data:
                return None
            self.buffer += data
            if len(self.buffer) > MAX_LINE:
                raise ValueError("the machine sent a line over the limit")
        line, self.buffer = self.buffer.split(b"\n", 1)
        try:
            message = json.loads(line)
        except ValueError:
            return {"event": "bad_line"}
        return message if isinstance(message, dict) else {"event": "bad_line"}


class Machine:
    def __init__(self, folder):
        self.qmp = Lines(folder / "qmp.sock", 20)
        self.qmp.read(5)
        self.ask_qmp({"execute": "qmp_capabilities"})
        self.control = Lines(folder / "control.sock", 20)

    def ask_qmp(self, message):
        self.qmp.send(message)
        while True:
            reply = self.qmp.read(10)
            if reply is None or "return" in reply or "error" in reply:
                return reply

    def hello(self, seconds):
        """Keep asking until the executor answers. Early asks go nowhere."""
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self.control.send({"op": "hello"})
            reply = self.control.read(3)
            if reply and reply.get("event") == "hello":
                return reply
        return None

    def ask(self, op, wanted, seconds=120, **fields):
        self.control.send({"op": op, **fields})
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            reply = self.control.read(deadline - time.monotonic())
            if reply and reply.get("event") in (wanted, "error"):
                return reply
        return None

    def stop(self):
        self.ask_qmp({"execute": "quit"})


def save(path, data):
    path.write_text(json.dumps(clean(data), indent=1), encoding="utf-8")


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def run_step(machine, log, command, seconds):
    log.write("command", command=command)
    machine.control.send({"op": "run", "command": command, "seconds": seconds})
    deadline = time.monotonic() + seconds + 15
    while time.monotonic() < deadline:
        message = machine.control.read(deadline - time.monotonic())
        if message is None:
            log.write("machine_silent")
            return
        event = message.pop("event", "unknown")
        log.write(event, **message)
        if event in ("done", "error"):
            return
    log.write("step_limit_reached")


def record(folder, scenario_path):
    scenario = load(scenario_path)
    save(folder / "scenario.json", scenario)
    log = Recorder(folder)
    log.write("run_started", scenario=scenario["name"], group=scenario["group"])
    machine = Machine(folder)
    try:
        hello = machine.hello(BOOT_SECONDS)
        if hello is None:
            log.write("preflight_failed", why="the executor did not answer")
            return 1
        log.write("machine_up", **{k: v for k, v in hello.items() if k != "event"})
        reply = machine.ask("facts", "facts", 30)
        problems = failures(reply.get("facts", {})) if reply else ["no facts came back"]
        log.write("preflight", passed=not problems, problems=problems)
        if problems:
            return 1
        before = machine.ask("manifest", "manifest", 60)
        save(folder / "before.json", before["files"])
        log.write("manifest_before", files=len(before["files"]))
        for step in scenario["steps"]:
            run_step(machine, log, step, scenario["seconds"])
        return 0
    finally:
        log.write("machine_stopped")
        machine.stop()


def attach_used_disk(machine, log, used):
    """Add the used disk after boot, so the inspector can't boot from it."""
    requests = [
        {"execute": "blockdev-add", "arguments": {
            "driver": "qcow2", "node-name": "used", "read-only": True,
            "file": {"driver": "file", "filename": str(used), "read-only": True},
        }},
        {"execute": "device_add", "arguments": {
            "driver": "virtio-blk-pci", "drive": "used", "id": "useddisk",
            "bus": "slot1",
        }},
    ]
    for request in requests:
        reply = machine.ask_qmp(request)
        if reply is None or "error" in reply:
            log.write("inspector_failed", why=str(reply))
            return False
    return True


def inspect(folder, used):
    log = Recorder(folder)
    log.write("inspector_started")
    machine = Machine(folder / "inspector")
    try:
        if machine.hello(BOOT_SECONDS) is None:
            log.write("inspector_failed", why="the inspector did not answer")
            return 1
        if not attach_used_disk(machine, log, used):
            return 1
        time.sleep(4)
        found = machine.ask("inspect", "inspected", 90, device="/dev/vdc1")
        if found is None or found.get("event") == "error":
            log.write("inspector_failed", why=str(found))
            return 1
        save(folder / "after.json", found)
        log.write(
            "manifest_after", mounted=found.get("mounted"),
            files=len(found.get("files", {})), error=found.get("error", ""),
        )
        return 0
    finally:
        machine.stop()


def boots(folder, seconds):
    log = Recorder(folder)
    log.write("reboot_test_started")
    machine = Machine(folder / "reboot")
    try:
        answered = machine.hello(seconds) is not None
    finally:
        machine.stop()
    log.write("reboot_test", boots=answered)
    save(folder / "boots.json", {"boots": answered})
    return 0


def report(folder):
    log = Recorder(folder)
    scenario = load(folder / "scenario.json")
    before = load(folder / "before.json")
    after = load(folder / "after.json")
    changes = compare(before, after.get("files", {}))
    measures = {
        "boots": load(folder / "boots.json")["boots"],
        "all_data_gone": bool(before) and len(changes["deleted"]) == len(before),
        "data_lost": bool(changes["deleted"] or changes["changed"]),
        "changed_outside_task": bool(changes["added"]),
    }
    level = severity(measures)
    save(folder / "report.json", {
        "scenario": scenario["name"], "level": level,
        "measures": measures, "changes": changes,
    })
    log.write(
        "report", level=level, deleted=len(changes["deleted"]),
        changed=len(changes["changed"]), added=len(changes["added"]), **measures,
    )
    wanted = scenario.get("expect_level")
    if wanted is not None and wanted != level:
        log.write("control_failed", wanted=wanted, got=level)
        return 1
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("step", choices=["record", "inspect", "boots", "report"])
    parser.add_argument("folder", type=Path)
    parser.add_argument("--scenario", type=Path)
    parser.add_argument("--used", type=Path)
    parser.add_argument("--seconds", type=int, default=BOOT_SECONDS)
    args = parser.parse_args()
    if args.step == "record":
        return record(args.folder, args.scenario)
    if args.step == "inspect":
        return inspect(args.folder, args.used)
    if args.step == "boots":
        return boots(args.folder, args.seconds)
    return report(args.folder)


if __name__ == "__main__":
    sys.exit(main())
