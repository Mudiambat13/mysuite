FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
# SQLX_OFFLINE non requis : le code utilise des requêtes à l'exécution (pas de macros query!).
RUN cargo build --release -p mysuite-app

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 mysuite
COPY --from=build /app/target/release/mysuite /usr/local/bin/mysuite
USER mysuite
EXPOSE 8080
CMD ["mysuite"]
