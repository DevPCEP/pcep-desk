# Estágio 1: compila o painel em modo release.
FROM rust:1.95-slim AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY templates ./templates
COPY src ./src
RUN cargo build --release --locked

# Estágio 2: imagem final enxuta, sem ferramentas de compilação.
FROM debian:stable-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin painel \
    && mkdir -p /app /dados \
    && chown painel:painel /dados

# O painel serve /static a partir do diretório de trabalho (ServeDir("static")).
WORKDIR /app
COPY --from=build /src/target/release/painel /app/painel
COPY static /app/static
ENV PATH="/app:${PATH}" \
    PAINEL_DB="sqlite:///dados/painel.db" \
    PAINEL_ENDERECO="0.0.0.0:8080"

USER painel
VOLUME /dados
EXPOSE 8080
CMD ["painel"]
