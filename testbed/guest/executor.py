#!/usr/bin/python3
"""Runs commands inside the test machine. This file exists only in the image.

Everything is imported up front, and nothing is loaded later, so this keeps
reporting while a command deletes the disk under it.
"""
import codecs
import encodings.ascii
import encodings.idna
import encodings.utf_8
import hashlib
import json
import os
import select
import signal
import subprocess
import sys
import time

PORT = "/dev/virtio-ports/termite.control"
MARKER = "/etc/termite-testbed"
# The fake set: everything the run may touch and everything we count.
SCOPES = ("home/dev", "srv")
MOUNT = "/mnt/used"
STATE_SECONDS = 2


def refuse_outside_the_machine():
    interfaces = [name for name in os.listdir("/sys/class/net") if name != "lo"]
    if not sys.platform.startswith("linux") or not os.path.exists(MARKER):
        sys.exit("not the test machine")
    if not os.path.exists(PORT) or interfaces:
        sys.exit("not the test machine")


def state():
    disk = os.statvfs("/")
    return {
        "event": "state",
        "files_in_use": disk.f_files - disk.f_ffree,
        "bytes_in_use": (disk.f_blocks - disk.f_bfree) * disk.f_frsize,
        "processes": sum(name.isdigit() for name in os.listdir("/proc")),
    }


def read_routes():
    with open("/proc/net/route", encoding="ascii") as file:
        return len(file.readlines()) - 1


def read_mounts():
    with open("/proc/mounts", encoding="utf-8") as file:
        return sorted({line.split()[2] for line in file})


def unknown_data():
    allowed = {"dev"}
    found = [f"/home/{name}" for name in os.listdir("/home") if name not in allowed]
    found += [
        f"/srv/{name}" for name in os.listdir("/srv")
        if name not in ("backup", "db")
    ]
    return found


def facts():
    agents = [
        name for name in ("/run/user/1000/ssh-agent", "/tmp/ssh-agent")
        if os.path.exists(name)
    ]
    return {
        "marker": os.path.exists(MARKER),
        "interfaces": sorted(os.listdir("/sys/class/net")),
        "routes": read_routes(),
        "mounts": read_mounts(),
        "agent_sockets": len(agents),
        "unknown_data": unknown_data(),
    }


def digest(path):
    sha = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(65536), b""):
            sha.update(block)
    return sha.hexdigest()


def manifest(root):
    found = {}
    for scope in SCOPES:
        for folder, _, names in os.walk(os.path.join(root, scope)):
            for name in names:
                path = os.path.join(folder, name)
                if os.path.isfile(path) and not os.path.islink(path):
                    found[os.path.relpath(path, root)] = digest(path)
    return found


def inspect(device):
    """Read a used disk. Never written to, and nothing on it can run."""
    os.makedirs(MOUNT, exist_ok=True)
    flags = "ro,noload,nosuid,nodev,noexec"
    done = subprocess.run(
        ["/usr/bin/mount", "-o", flags, device, MOUNT],
        capture_output=True, text=True, check=False,
    )
    if done.returncode != 0:
        return {"mounted": False, "error": done.stderr.strip(), "files": {}}
    try:
        return {
            "mounted": True,
            "files": manifest(MOUNT),
            "boot_files": os.path.isdir(os.path.join(MOUNT, "boot"))
            and len(os.listdir(os.path.join(MOUNT, "boot"))),
        }
    finally:
        subprocess.run(["/usr/bin/umount", MOUNT], check=False)


def run(port, command, seconds):
    """Run one command, reporting output and state as they happen."""
    started = time.monotonic()
    child = subprocess.Popen(
        ["/bin/bash", "-c", command], stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    stream = child.stdout.fileno()
    os.set_blocking(stream, False)
    last = 0.0
    while True:
        now = time.monotonic()
        if now - started > seconds:
            os.killpg(child.pid, signal.SIGKILL)
            send(port, {"event": "timed_out"})
            break
        if now - last >= STATE_SECONDS:
            send(port, state())
            last = now
        ready, _, _ = select.select([stream], [], [], 0.5)
        if ready:
            data = os.read(stream, 8192)
            if not data:
                break
            text = data.decode("utf-8", "replace")
            send(port, {"event": "output", "text": text})
    code = child.wait()
    os.sync()
    send(port, state())
    send(port, {"event": "done", "exit_code": code})


def send(port, message):
    os.write(port, (json.dumps(message) + "\n").encode())


def answer(port, request):
    op = request.get("op")
    if op == "hello":
        send(port, {**state(), "event": "hello"})
    elif op == "facts":
        send(port, {"event": "facts", "facts": facts()})
    elif op == "manifest":
        send(port, {"event": "manifest", "files": manifest("/")})
    elif op == "inspect":
        send(port, {"event": "inspected", **inspect(request["device"])})
    elif op == "run":
        run(port, request["command"], request["seconds"])
    else:
        send(port, {"event": "unknown", "op": str(op)})


def main():
    refuse_outside_the_machine()
    port = os.open(PORT, os.O_RDWR)
    print("executor: listening", flush=True)
    buffer = b""
    while True:
        data = os.read(port, 65536)
        if not data:
            time.sleep(0.2)
            continue
        buffer += data
        while b"\n" in buffer:
            line, buffer = buffer.split(b"\n", 1)
            try:
                answer(port, json.loads(line))
            except Exception as error:  # Keep reporting after the disk is gone.
                send(port, {"event": "error", "text": str(error)})


if __name__ == "__main__":
    main()
