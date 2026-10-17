# TAM 문서

| 문서 | 대상 독자 | 내용 |
|---|---|---|
| [The TAM Book](book/README.md) | 처음 배우는 사람 | 14장. 설치부터 소유권, 오류 처리, 모듈, unsafe, 제네릭, 여러 대상, 프로젝트까지 |
| [TAM by Example](by-example/README.md) | 예제로 빠르게 익히려는 사람 | 9개 분야 예제 |
| [The Standard Library API Documentation](stdlib.md) | 함수를 찾는 사람 | `tam.os`, `tam.fmt`, `tam.serial`, 내장 함수 |
| [Tmake.toml Documentation](tmake.md) | 프로젝트를 구성하는 사람 | 패키지, 의존성, 워크스페이스, 프로필, 대상, 잠금 파일 |
| [The TAM Reference](reference.md) | 정확한 의미가 필요한 사람 | 문법(EBNF), 타입, 소유권·빌림 규칙, 수치 의미, 대상, 호환성 약속 |
| [Why TAM](why-tam.md) | TAM이 맞는지 판단하려는 사람 | 설계 철학, C·Rust·Go·Zig와의 비교, 한계, C/Rust 사용자용 번역 |
| [Safety and Memory Model](safety.md) | 안전성의 정확한 범위가 궁금한 사람 | 컴파일 시간·실행 시간 보장, `unsafe`의 책임, 보장하지 않는 것 |
| [Compiler Architecture](compiler.md) | 컴파일러를 고치거나 배우려는 사람 | 파이프라인, 단계별 설명, 대상별 링크, 시험·퍼저, 기여 방법 |
| [Style Guide](style.md) | 코드를 쓰는 사람 | 모양, 이름, 오류 처리, 소유권, `unsafe` 관용구 |
| [Release Notes](changelog.md) | 버전 변화를 보려는 사람 | 1.0.0 변경, 알려진 한계, 개발 이력, 앞으로의 후보 |

## 문서의 품질 보증

문서 안의 코드는 **모두 실제로 실행해서 검증**한다.

```console
$ cargo build
$ python3 tools/doccheck.py docs
```

- ` ```tam `: 컴파일하고 실행해 `// exit:`, `// out:`과 비교한다.
- ` ```tam,error ` / ` ```tam,trap `: 지정한 컴파일 오류, 실행 중 trap을 확인한다.
- ` ```tam,boot=bios|uefi|aarch64 `: QEMU에서 부팅해 직렬 출력과 종료 코드를 확인한다.
- `<!-- project -->` 묶음: 여러 파일의 `Tmake.toml` 프로젝트를 만들어 빌드·실행한다.
- ` ```toml,manifest `: `Tmake.toml` 검증.

문서는 한국어로 쓰고 코드, 식별자, 컴파일러 메시지는 영어 그대로 둔다.
