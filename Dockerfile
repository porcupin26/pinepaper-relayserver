FROM rust:1.96-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /build/target/release/pinepaper-exchange /usr/local/bin/pinepaper-exchange
USER 65532:65532
ENV PINEPAPER_EXCHANGE_BIND=0.0.0.0:8787
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/pinepaper-exchange"]
