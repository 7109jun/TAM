# 1. 시작하기

이 장에서는 컴파일러를 준비하고, 첫 프로그램을 컴파일해 실행하고, `Tmake.toml` 프로젝트를 만든다.

## 1.1 TAM이란

TAM 프로그램은 `tamc`(TAM 컴파일러)가 **LLVM IR**로 번역하고, LLVM 도구가 기계어로 바꾼다. 결과는 평범한 네이티브 실행 파일이다. 런타임이나 가상 머신이 따라붙지 않는다.

TAM의 약속은 세 가지다.

- **간결하다.** 세미콜론도, 화살표(`->`)도, 수명(lifetime) 표기도 없다. 지역 변수의 타입은 추론한다.
- **안전하다.** 이동 후 사용, 댕글링 참조, 이중 해제, 누수 같은 메모리 오류는 **컴파일 시간에** 거절된다. 정수 오버플로, 0으로 나누기, 배열 경계 위반은 **실행 시간에 즉시 멈춘다**(조용히 틀린 값을 만들지 않는다).
- **통제할 수 있다.** `unsafe`에서는 raw 포인터, 직접 할당, 외부 함수 호출, 하드웨어 레지스터 접근이 가능하다. OS 없이 부팅되는 프로그램(UEFI, BIOS, 베어메탈)도 같은 언어로 만든다.

## 1.2 컴파일러 준비

`tamc`는 Rust로 작성되어 있다. 소스에서 빌드한다.

```console
$ cargo build --release
$ ./target/release/tamc --version
tamc 1.0.0
```

대상에 따라 LLVM 도구가 필요하다.

| 만들려는 것 | 필요한 도구 |
|---|---|
| 호스트(Linux) 실행 파일 | `clang` (PATH에 있어야 한다) |
| Windows `.exe`, UEFI, BIOS, AArch64 이미지 | `llc`, `lld-link` (`tamc`와 **같은 폴더**에 두거나 PATH에 둔다) |

Windows에서는 설치 폴더에 `tamc.exe`, `llc.exe`, `lld-link.exe` 세 파일만 있으면 된다. CRT도, Visual Studio도, Windows SDK도 필요 없다. `tamc`가 `kernel32`의 import 라이브러리를 스스로 만들어 링크한다. (저장소의 `.github/workflows/windows.yml`가 `tamc.exe`를 만들어 준다.)

`check` 명령(타입·소유권·빌림 검사만)은 어떤 외부 도구도 필요 없다.

## 1.3 Hello, TAM

`hello.tam`을 만든다.

```tam
use tam.os

fn main() i32 {
    os::println("Hello, TAM!")
    0
}
// out: Hello, TAM!
```

컴파일하고 실행한다.

```console
$ tamc build hello.tam -o hello
$ ./hello
Hello, TAM!
$ echo $?
0
```

한 줄로 `tamc run hello.tam`도 된다(빌드하고 실행한 뒤 프로그램의 종료 코드로 끝난다).

이 작은 프로그램에 TAM의 핵심이 들어 있다.

- `use tam.os`는 **표준 라이브러리 모듈**을 가져온다. 이후 `os::println`처럼 `모듈::이름`으로 쓴다.
- `fn main() i32`가 프로그램의 시작점이다. **반환 타입은 화살표 없이** 매개변수 목록 뒤에 쓴다.
- 블록의 **마지막 식이 반환값**이다(`0`). 이 값이 프로그램의 **종료 코드**가 된다.
- 문장은 **줄바꿈으로 끝난다.** 세미콜론이 없다.

### 종료 코드가 곧 결과다

`main`이 돌려주는 값이 종료 코드이므로, 출력 없이도 프로그램을 시험할 수 있다.

```tam
// exit: 42
fn main() i32 {
    6 * 7
}
```

```console
$ tamc run answer.tam
$ echo $?
42
```

## 1.4 첫 오류 읽기

컴파일러의 오류는 **파일, 줄, 칸**과 함께 문제의 코드를 가리킨다. 일부러 틀려 보자.

```tam,error
// error: mismatched types
fn add(a: i32, b: i32) i32 { a + b }

fn main() i32 {
    let x: i64 = 5
    add(x, 1)
}
```

```console
error: mismatched types: expected `i32`, found `i64`
 --> ex3.tam:4:9
  |
4 |     add(x, 1)
  |         ^ expected `i32`
```

TAM은 정수 타입을 **암묵적으로 바꾸지 않는다.** `i64`를 `i32` 자리에 넣으려면 `x as i32`로 명시한다. 이 엄격함이 "조용히 값이 잘리는" 버그를 막는다.

## 1.5 프로젝트 만들기

프로그램이 파일 하나를 넘어가면 `Tmake.toml` 프로젝트를 쓴다.

```console
$ tamc new greet
     Created binary `greet` in greet
$ cd greet
$ tamc run
   Compiling greet v0.1.0 (.)
    Finished dev [opt-level 0] host in 0.07s
     Running /home/me/greet/build/host/dev/greet
Hello, TAM!
```

만들어진 구조:

```
greet/
├── Tmake.toml
├── .gitignore
└── src/
    └── main.tam
```

`Tmake.toml`은 표준 TOML이다.

```toml
[package]
name = "greet"
version = "0.1.0"
tamc = ">=1.0"

[dependencies]
```

`tamc build`는 `build/host/dev/greet`를 만들고, 소스가 바뀌지 않았다면 다음 빌드는 `Fresh`로 건너뛴다. 의존성, 워크스페이스, 대상 설정은 [9장](09-modules-and-projects.md)과 [Tmake.toml Documentation](../tmake.md)에서 다룬다.

## 1.6 명령 요약

| 명령 | 하는 일 |
|---|---|
| `tamc check <파일>` | 검사만(링크 없음). 가장 빠르다 |
| `tamc build <파일> -o <출력>` | 실행 파일 만들기 |
| `tamc run <파일>` | 빌드하고 실행 |
| `tamc emit-llvm <파일>` | LLVM IR(`.ll`) 출력 |
| `tamc new/init/build/run/check/clean/tree/metadata` | 프로젝트 모드 |
| `--target host\|windows\|uefi\|bios\|aarch64` | 컴파일 대상 |

다음 장에서는 변수와 타입을 배운다.

→ [2. 값과 변수](02-values-and-variables.md)
