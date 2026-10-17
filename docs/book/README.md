# The TAM Book

**TAM**은 간결함, 안정성, 시스템 프로그래밍을 기준으로 만든 네이티브 언어다. 가비지 컬렉터도 가상 머신도 없이 LLVM으로 기계어를 만들고, 메모리 안전은 **컴파일 시간의 소유권·빌림 검사**로 지킨다. 필요하면 `unsafe`로 모든 통제권을 쥘 수 있고, `unsafe` 바깥의 안전은 컴파일러가 책임진다.

> 사용자에게 통제권을 모두 준다. 안정성은 컴파일러가 책임진다.

이 책은 TAM을 **처음부터 끝까지** 배우는 안내서다. 프로그래밍 경험(C, Rust, Go, Python 중 하나)이 조금 있다고 가정한다. 모든 코드 예제는 실제 컴파일러(`tamc 1.0`)로 **컴파일하고 실행해** 출력과 종료 코드를 검증했다.

## 차례

1. [시작하기](01-getting-started.md) — 설치, Hello TAM, 프로젝트 만들기
2. [값과 변수](02-values-and-variables.md) — 타입, `let`/`mut`, 연산, 오버플로
3. [함수와 제어 흐름](03-functions-and-control-flow.md) — `fn`, `if`, `while`, `for`
4. [소유권](04-ownership.md) — 이동, 복사, `String`, 자동 해제
5. [참조와 빌림](05-references-and-borrowing.md) — `&`, `&mut`, 빌림 검사기 읽기
6. [구조체와 메서드](06-structs-and-methods.md)
7. [배열과 슬라이스](07-arrays-and-slices.md)
8. [오류 처리](08-error-handling.md) — `Option`, `Result`, `?`, `match`, 패닉
9. [모듈과 프로젝트](09-modules-and-projects.md) — `use`, `pub`, `Tmake.toml`, 워크스페이스
10. [`unsafe`와 시스템 프로그래밍](10-unsafe-and-systems.md) — 포인터, 힙, FFI
11. [여러 대상으로 빌드하기](11-targets.md) — Windows, UEFI, BIOS, AArch64
12. [프로젝트: 줄 단위 합계 도구](12-project.md) — 라이브러리와 실행 파일, 입출력
13. [제네릭](13-generics.md) — 타입 매개변수, 제네릭 함수·구조체, 단형화, 인스턴스 오류 읽기
14. [실전 가이드](14-practice.md) — 오류 읽기, 디버깅, 안전한 `unsafe`, 자주 하는 실수

## 함께 보면 좋은 문서

| 문서 | 용도 |
|---|---|
| [TAM by Example](../by-example/README.md) | 짧은 예제 모음으로 익히기 |
| [The TAM Reference](../reference.md) | 문법과 의미의 **규범적 정의** |
| [The Standard Library API Documentation](../stdlib.md) | `tam.os`, `tam.fmt`, 내장 함수 |
| [Tmake.toml Documentation](../tmake.md) | 프로젝트 파일 |

## 이 책의 표기

- `// exit: N`, `// out: ...`은 예제를 실행했을 때 **검증된** 종료 코드와 출력이다.
- `tamc`가 내는 오류는 실제 출력을 그대로 보여 준다.
- "(안 된다)"라고 적은 코드는 실제로 컴파일 오류가 난다는 것을 문서 검사기가 확인한다.
