# TAM 문서

| 문서 | 대상 독자 | 내용 |
|---|---|---|
| [The TAM Book](book/README.md) | 처음 배우는 사람 | 14장. 설치부터 소유권, 오류 처리, 모듈, unsafe, 제네릭, 여러 대상, 프로젝트까지 |
| [TAM by Example](by-example/README.md) | 예제로 빠르게 익히려는 사람 | 9개 분야 예제 |
| [The Standard Library API Documentation](stdlib.md) | 함수를 찾는 사람 | `tam.os`, `tam.fmt`, `tam.serial`, 내장 함수 |
| [Tmake.toml Documentation](tmake.md) | 프로젝트를 구성하는 사람 | 패키지, 의존성, 워크스페이스, 프로필, 대상, 잠금 파일 |
| [The TAM Reference](reference.md) | 정확한 의미가 필요한 사람 | 문법(EBNF), 타입, 소유권·빌림 규칙, 수치 의미, 대상 |

## 문서의 품질 보증

문서 안의 코드 예제는 릴리스 전에 모두 실제 `tamc 1.0`으로 컴파일·실행해 출력과 종료 코드를 검증했다. 표기는 다음과 같다.

- ` ```tam `: 컴파일하고 실행. `// exit:`, `// out:`이 기대값.
- ` ```tam,error ` / ` ```tam,trap `: 지정한 컴파일 오류, 실행 중 trap.
- ` ```tam,boot=bios|uefi|aarch64 `: QEMU 부팅 예제.

문서는 한국어로 쓰고 코드, 식별자, 컴파일러 메시지는 영어 그대로 둔다.
