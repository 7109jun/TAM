# TAM 1.0.0

TAM 언어 컴파일러 `tamc`(Rust, LLVM 백엔드). 문서는 `docs/`(한국어)에 있다: [docs/README.md](docs/README.md).

## 소스에서 빌드

필요: Rust, `clang`, `llc`(LLVM), `lld`(Windows/펌웨어 대상용 `lld-link`).

```console
$ cargo build --release
$ ./target/release/tamc run examples/ret42.tam
```

## Docker

```console
$ docker build -t tam:1.0.0 .
$ docker run --rm -v "$PWD":/work tam:1.0.0 run examples/ret42.tam
```

이미지에는 `tamc`와 백엔드(clang, llc, lld)가 들어 있고, 문서는 `/usr/share/doc/tam`에 있다.
