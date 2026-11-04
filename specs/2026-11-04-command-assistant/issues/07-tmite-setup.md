# 07 — `tmite setup`: find servers, pick models, fetch what is missing

Status: resolved
Type: task
Model: Opus 5.5 (claude-opus-5-5)
Blocked by: 03, 05
Spec: [../spec.md](../spec.md) (Setup)

## Details

- `src/bin/tmite/setup/discover.rs`: probe 127.0.0.1:8080 (llama.cpp), 1234 (LM Studio), 11434 (Ollama), the llama-server ports found in `ps` (child servers of the Llama app's router are skipped), and the URLs in the config. Tell the kinds apart by `owned_by: llamacpp`, `/api/version` and `/api/v0/models`. Decision models: the router's `decision` tag or `decisions` output modality, or Ollama's `decision` capability from `/api/show`. Leave out embedding, rerank and image models. List installed servers that are not running.
- `src/bin/tmite/setup/hub.rs`: the suggested models (Kev-4B Q8_0 for decisions, Qwen2.5-Coder-7B Q4_K_M for chat, `qwen2.5-coder:7b` on Ollama), file sizes from the Hugging Face tree API, and the `hf download` and `llama-server -hf` commands.
- `src/bin/tmite/setup/mod.rs`: the questions, the fixes for a missing model (each printed in full with its size, run only after `y`), a detached llama-server with a log file and a readiness wait, the swap warning, writing the config, and a first query.
- `Config::path()` is public so setup writes where `load` reads.

## Comments

**2026-11-04, Claude Code — done.** All 69 tests pass (38 lib, 24 bin, 7 doc); fmt and clippy are clean.

Tried on the owner's machine (Llama app router on 9931 with `--models-max 1`, Ollama 0.40 on 11434, LM Studio installed, not running):

- Found both servers and all their models; the Llama app router's Kev-4B and Ollama's `tev1:4b` are listed as decision models, the rest as chat.
- The swap warning shows for the router; with one model loaded at a time, a `?x` takes 16–40 s, mostly loading.
- `? list the files in this folder` → `find . -maxdepth 1 -type f`; `?x show disk usage of this folder` → runs `du -sh .` (only_reads 0.99); `?x delete the target folder` → `rm -rf target`, asks first (model labelled it destructive).

Found while testing, and fixed:

- Qwen3.8-27B reasons for ~70 s before a one-line answer and hit the 120 s limit. The chat request now sends `chat_template_kwargs: {enable_thinking: false}` (9 s, same answer; other servers ignore it).
- Ollama's `/v1/systemone` requires a text description for each `noul` answer. The safety questions now send one; Kev-4B separates better with them (read-only ≥ 0.98, others ≤ 0.27), so the thresholds hold. `tev1:4b` answers `no_secrets` badly (0.29 for `ls`), so with it `?x` would always ask: safe, but not useful.
- An early run used a binary built before the no-terminal rule and wrote `~/.config/termite/config.json` (the folder was empty, so nothing was overwritten). Without a terminal, setup now writes nothing.

Not tried: the download and llama-server start paths (both need a missing model and a `y`), and LM Studio while running.
