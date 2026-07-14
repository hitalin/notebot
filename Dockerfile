# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
WORKDIR /build

# ビルド対象の example (自作 bot はこのリポジトリの examples/ に置くか、
# 自分のクレートで同様の Dockerfile を書く)
ARG EXAMPLE=echo

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples

# keyring はコンテナ内で使えない (Docker の seccomp が keyctl を塞ぐ) ため
# 無効化。トークンは notecli.db (volume) に保管される。
RUN cargo build --release --example "${EXAMPLE}" --no-default-features

# 運用用 CLI (login / accounts / doctor)。lib と同じ rev・同じく keyring 無効。
RUN cargo install --git https://github.com/hitalin/notecli.git \
    --rev f1931af84a384c749efa2cd5b9aa478d22d19393 \
    --no-default-features notecli

FROM debian:bookworm-slim
ARG EXAMPLE=echo
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home bot \
    && mkdir /data && chown bot:bot /data

# dirs::data_dir() がここを見る → /data/notecli/notecli.db
ENV XDG_DATA_HOME=/data

COPY --from=builder "/build/target/release/examples/${EXAMPLE}" /usr/local/bin/bot
COPY --from=builder /usr/local/cargo/bin/notecli /usr/local/bin/notecli

USER bot
VOLUME /data
CMD ["bot"]
