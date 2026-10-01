# syntax=docker/dockerfile:1
# Build em duas etapas: a imagem final não leva toolchain Rust nem código-fonte.
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY truco_core ./truco_core
COPY truco_server ./truco_server
COPY migrations ./migrations
RUN cargo build --release -p truco_server

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/*
# fonts-dejavu-core existe para que os caracteres Unicode de carta (U+1F0A1…) tenham glifo
# quando alguém abrir o app num navegador dentro de container.
COPY --from=build /app/target/release/truco_server /usr/local/bin/truco_server
COPY static /srv/static
RUN useradd --system --uid 10001 truco && mkdir -p /dados && chown truco:truco /dados
USER truco
ENV TRUCO_STATIC=/srv/static \
    DATABASE_URL=sqlite:///dados/truco.db \
    PORT=8080 \
    RUST_LOG=info,truco_server=debug
EXPOSE 8080
HEALTHCHECK --interval=3s --timeout=3s --retries=30 --start-period=3s \
  CMD curl -fsS http://127.0.0.1:8080/api/saude || exit 1
CMD ["truco_server"]
