# 10 — Red-team sandbox: Colima profile, Compose, hostile model, c2, dns, envs

Status: ready-for-agent
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: —
Spec: [../spec.md](../spec.md) (section 6)

## Details

- `redteam/README.md`: `colima start termite-sandbox --mount none --cpu 4 --memory 8` (owner approved) and how to run the suite; never on the default profile, which mounts the home folder.
- `redteam/compose.yaml`: `internal: true` network; services `runner`, `env-debian`, `env-alpine`, `env-fedora`, `env-ubuntu-sudo`, `hostile-model`, `c2`, `dns`, and optionally `llama-cpu` (llama.cpp CPU server with small GGUF models in a read-only volume). Every service: `cap_drop: [ALL]`, `security_opt: [no-new-privileges:true]`, `read_only: true` with `tmpfs` for writable paths, `pids_limit`, memory and CPU limits, no Docker socket.
- `hostile-model`: a tiny OpenAI-compatible server (in Rust, as a workspace example or a separate small binary) that answers from a scripted attack file.
- `c2` and `dns`: record every connection and lookup to a shared results volume.
- Target environments: fixture home with canary secrets (canary files read-tracked with `inotify` or `fanotify` inside the container) and, on `env-ubuntu-sudo`, a cached sudo timestamp.
- The harness refuses to start unless: inside a container, `TERMITE_REDTEAM_SANDBOX` is set by the image, and a connection to an outside address fails.
- Models: the sandbox serves CPU models itself; a `--real-models` option adds a `model-gateway` that only forwards `/v1/chat/completions`, `/v1/systemone` and `GET /v1/models` to the host, for benchmark runs (owner: try everything).

## Tests first

- A smoke run: `compose up`, the harness checks it is sandboxed, a benign request round-trips through `hostile-model`, a scripted `curl c2` attempt is recorded by `c2`, and outside access fails.
- The harness refuses to start on the host and in a container without the marker.
