# The container for red team work. See scripts/container.sh for how it's run.
# Only the compiled tool goes in: no source, no secrets, no shell history.
FROM rust:1-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY examples examples
COPY shell shell
COPY bash bash
COPY prompts prompts
RUN cargo build --example eval

FROM rust:1-bookworm
RUN apt-get update \
  && apt-get install --yes --no-install-recommends socat \
  && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/debug/examples/eval /usr/local/bin/eval
# An empty, unwritable src folder, so the gate has a real answer.
RUN mkdir -p /work/src /work/eval/results/team && chmod 555 /work/src
WORKDIR /work
