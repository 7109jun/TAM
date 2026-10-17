# TAM 1.0.0 — tamc 컴파일러 이미지
#   docker build -t tam:1.0.0 .
#   docker run --rm -v "$PWD":/work tam:1.0.0 run hello.tam
#   docker run --rm -v "$PWD":/work tam:1.0.0 build        # Tmake.toml 프로젝트

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY std ./std
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends clang llvm lld \
 && rm -rf /var/lib/apt/lists/* \
 && (command -v llc >/dev/null || ln -s "$(ls /usr/bin/llc-* | head -1)" /usr/local/bin/llc) \
 && (command -v lld-link >/dev/null || ln -s "$(ls /usr/bin/lld-link-* | head -1)" /usr/local/bin/lld-link)
COPY --from=build /src/target/release/tamc /usr/local/bin/tamc
COPY docs /usr/share/doc/tam
COPY examples /usr/share/doc/tam/examples
WORKDIR /work
ENTRYPOINT ["tamc"]
CMD ["--help"]
