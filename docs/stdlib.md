# The Standard Library API Documentation

TAM의 표준 라이브러리는 **컴파일러에 내장된 TAM 소스**(`std/`)와 컴파일러가 직접 제공하는 **내장 함수**로 이루어진다. 이 문서는 모든 공개 함수의 시그니처, 동작, 오류, 대상별 차이, 예제를 정의한다.

- 대상 버전: `tamc 1.0`
- 이 문서의 모든 `tam` 코드 블록은 `tools/doccheck.py`가 호스트(Linux)에서 실제로 컴파일하고 실행해 출력과 종료 코드를 검증한다.
- 언어 자체의 규칙은 [The TAM Reference](reference.md)를 본다.

## 목차

1. [개요](#1-개요)
2. [대상별 지원표](#2-대상별-지원표)
3. [`tam.os`](#3-tamos)
4. [`tam.fmt`](#4-tamfmt)
5. [`tam.serial`](#5-tamserial)
6. [내장 함수](#6-내장-함수)
7. [메모리와 할당자](#7-메모리와-할당자)
8. [오류 코드](#8-오류-코드)
9. [레시피](#9-레시피)
10. [표준 라이브러리 없이 쓰기와 한계](#10-표준-라이브러리-없이-쓰기와-한계)

---

## 1. 개요

### 1.1 가져오는 법

```tam,ignore
use tam.os      // 입출력, 파일, 시간, 종료
use tam.fmt     // 문자열 만들기
use tam.serial  // 직렬 포트(x86 펌웨어 전용)
```

`use`의 첫 조각이 `tam`이면 프로젝트의 파일이 아니라 내장 소스를 쓴다. 이후 `os::print(...)`처럼 마지막 조각을 모듈 이름으로 쓴다.

### 1.2 구성 원칙

- **플랫폼 호출은 이 계층에만 있다.** 코어 컴파일러는 OS를 모른다. `tam.os`는 호스트에서 libc(`write`, `read`, `open`…), Windows에서 `kernel32`(`WriteFile`, `ReadFile`, `CreateFileA`…), 펌웨어에서 직렬 포트/UEFI 콘솔/UART를 부른다. **공개 API는 모든 대상에서 같다**(펌웨어는 일부만 제공).
- **오류는 값이다.** 실패할 수 있는 함수는 `Result<T, i32>`를 돌려주며 `i32`는 운영체제 오류 코드(호스트 `errno`, Windows `GetLastError`)다([8장](#8-오류-코드)). 예외도 패닉도 없다.
- **입출력은 바이트 단위**다. 문자열은 UTF-8 바이트 그대로 오간다. 서식(`print_int` 외)은 `tam.fmt`로 만든 문자열을 출력한다.
- 소유권 규칙이 그대로 적용된다. `String`을 돌려주는 함수의 결과는 호출자가 소유하며 스코프가 끝날 때 해제된다.

### 1.3 첫 프로그램

```tam
use tam.os

fn main() i32 {
    os::println("Hello, TAM!")
    0
}
// out: Hello, TAM!
```

---

## 2. 대상별 지원표

| 함수 | host (Linux) | windows | uefi | bios | aarch64 |
|---|:-:|:-:|:-:|:-:|:-:|
| `os::print` `os::println` `os::print_int` | ✔ | ✔ | ✔ (콘솔) | ✔ (COM1) | ✔ (UART) |
| `os::exit` | ✔ | ✔ | ✔ | ✔ | ✔ |
| `os::read_line` | ✔ | ✔ | ✘ | ✘ | ✘ |
| `os::read_file` `os::write_file` | ✔ | ✔ | ✘ | ✘ | ✘ |
| `os::time_ms` | ✔ | ✔ | ✘ | ✘ | ✘ |
| `fmt::*` | ✔ | ✔ | ✔ | ✔ | ✔ |
| `serial::*` | ✘ | ✘ | ✔ | ✔ | ✘ |

✘인 함수를 호출하면 컴파일 오류(`` module `os` has no item `read_line` ``)다. 펌웨어의 표준 출력은 대상마다 다르다.

| 대상 | `print`의 목적지 | `exit(code)` |
|---|---|---|
| host | 파일 디스크립터 1(stdout) | libc `exit` |
| windows | `GetStdHandle(STD_OUTPUT_HANDLE)` | `ExitProcess` |
| uefi | `ConOut->OutputString`(ASCII만, `\n`→`\r\n`) | QEMU `isa-debug-exit`(포트 0xF4)에 쓰고 멈춤. 실제 하드웨어에서는 그냥 멈춤 |
| bios | COM1 직렬 포트(0x3F8) | 위와 같음 |
| aarch64 | PL011 UART(0x09000000) | semihosting `SYS_EXIT_EXTENDED`(QEMU `-semihosting` 필요) |

---

## 3. `tam.os`

```tam,ignore
use tam.os
```

### 3.1 `print`

```
pub fn print(s: str)
```

`s`의 바이트를 표준 출력에 쓴다. 줄바꿈을 붙이지 않는다. 버퍼링이 없으므로 호출마다 쓰기 시스템 호출이 일어난다(많은 작은 쓰기는 `String`으로 모아 한 번에 낸다). 출력 실패는 무시한다(예: 닫힌 파이프).

### 3.2 `println`

```
pub fn println(s: str)
```

`print(s)` 다음에 `"\n"`을 쓴다. Windows에서도 `"\n"`만 쓴다(콘솔은 이를 처리한다). UEFI 콘솔은 `\n`을 `\r\n`으로 바꾼다.

### 3.3 `print_int`

```
pub fn print_int(n: i64)
```

`n`을 10진수로 줄바꿈 없이 쓴다. 음수는 `-` 부호를 붙인다. `i64`의 최솟값 `-9223372036854775808`도 올바르게 출력한다.

```tam
use tam.os

fn main() i32 {
    os::print_int(0)
    os::print("|")
    os::print_int(-42)
    os::print("|")
    os::print_int(9223372036854775807)
    os::print("|")
    let min: i64 = -9223372036854775807 - 1
    os::print_int(min)
    os::println("")
    0
}
// out: 0|-42|9223372036854775807|-9223372036854775808
```

### 3.4 `exit`

```
pub fn exit(code: i32)
```

프로세스를 즉시 끝낸다. **살아 있는 지역 변수의 자동 해제를 실행하지 않는다.** 정상 종료는 `main`이 값을 반환하는 것이므로, `exit`는 깊은 호출 안에서 바로 빠져나와야 할 때만 쓴다. 호스트에서는 종료 코드의 하위 8비트가 쓰인다.

```tam
use tam.os

fn fail_fast(code: i32) {
    os::println("fatal")
    os::exit(code)
}

fn main() i32 {
    fail_fast(3)
    0   // 도달하지 않는다
}
// exit: 3
// out: fatal
```

### 3.5 `read_line`

```
pub fn read_line() String          // host, windows
```

표준 입력에서 **한 줄**을 읽어 줄바꿈을 **제외하고** 돌려준다. 줄 끝의 `\r`도 제거한다(`\r\n`과 `\n`을 같게 취급). 입력이 끝나면(EOF) 그때까지 읽은 내용을 돌려준다. 읽을 것이 없으면 빈 문자열이다. EOF와 빈 줄은 구분되지 않는다. 오류가 나도 지금까지 읽은 것을 돌려준다.

한 바이트씩 읽으므로 대용량 입력에는 느리다.

```tam
use tam.os

fn main() i32 {
    let a = os::read_line()
    let b = os::read_line()
    let c = os::read_line()   // EOF → 빈 문자열
    os::println(as_str(&a))
    os::print_int(b.len as i64)
    os::println("")
    os::print_int(c.len as i64)
    os::println("")
    0
}
// stdin: hello world
// stdin:
// out: hello world
// out: 0
// out: 0
```

### 3.6 `read_file`

```
pub fn read_file(path: str) Result<String, i32>     // host, windows
```

`path`의 파일 **전체**를 읽어 `Ok(내용)`으로 돌려준다. 실패하면 `Err(OS 오류 코드)`다(없는 파일, 권한 없음, 디렉터리 등). 내용은 바이트 그대로(인코딩 변환 없음)이며 NUL을 포함할 수 있다.

- 경로는 `str`이며 내부에서 NUL로 끝나는 복사본을 만든다. 경로 안의 NUL은 지원하지 않는다.
- Windows에서는 ANSI 경로(`CreateFileA`)다. UTF-16 경로는 쓸 수 없다.
- 호스트에서 디렉터리를 읽으면 `open`은 성공하지만 `read`가 실패하므로 `Err(21)`(`EISDIR`)이다.

```tam
use tam.os

fn main() i32 {
    match os::read_file("/definitely/missing") {
        Ok(s) => os::println("?")
        Err(e) => {
            os::print_int(e as i64)   // ENOENT
            os::println("")
        }
    }
    0
}
// out: 2
```

### 3.7 `write_file`

```
pub fn write_file(path: str, data: str) Result<i32, i32>     // host, windows
```

`path`에 `data`를 쓴다. 파일이 없으면 만들고(호스트 권한 0644) **있으면 내용을 지운 뒤 덮어쓴다**. 성공하면 쓴 바이트 수, 실패하면 OS 오류 코드다. 부분 쓰기는 반복해서 끝까지 쓴다(호스트).

```tam
use tam.os

fn run() Result<i32, i32> {
    let n = os::write_file("/tmp/tam-doc-demo.txt", "line1\nline2\n")?
    let text = os::read_file("/tmp/tam-doc-demo.txt")?
    os::println(as_str(&text))
    Ok(n)
}

fn main() i32 {
    match run() {
        Ok(n) => n
        Err(e) => 100 + e
    }
}
// exit: 12
// out: line1
// out: line2
```

### 3.8 `time_ms`

```
pub fn time_ms() u64        // host, windows
```

**단조 증가**하는 시계를 밀리초로 돌려준다. 호스트는 `CLOCK_MONOTONIC`, Windows는 `GetTickCount64`(부팅 후 경과)다. 절대 시각(달력)이 아니므로 **경과 시간을 재는 데** 쓴다. 해상도는 호스트 1ms 미만, Windows 약 10~16ms다.

```tam
use tam.os

fn main() i32 {
    let t0 = os::time_ms()
    mut x: u64 = 0
    for i in 0..1000 { x += 1 }
    let dt = os::time_ms() - t0
    if dt < 1000 && x == 1000 { 0 } else { 1 }
}
```

---

## 4. `tam.fmt`

```tam,ignore
use tam.fmt
```

순수 TAM으로 쓰인 문자열 도구다. 모든 대상에서 같다. 할당은 `String`을 거치므로 소유권 규칙이 적용된다.

### 4.1 `push_str`

```
pub fn push_str(s: &mut String, t: str)
```

`s`의 끝에 `t`의 바이트를 이어 붙인다. 용량이 모자라면 늘린다.

### 4.2 `concat`

```
pub fn concat(a: str, b: str) String
```

`a` 다음에 `b`가 오는 **새** `String`을 만든다. 인자는 `str`이라 소유권이 이동하지 않는다. `String`이 있으면 `as_str(&s)`를 넘긴다.

### 4.3 `int_to_string`

```
pub fn int_to_string(n: i64) String
```

`n`의 10진 문자열. `i64`의 최솟값도 올바르다. 다른 정수형은 `as i64`로 넓혀 넘긴다(`u64`의 `i64` 최댓값 초과 값은 부호가 바뀌니 주의).

```tam
use tam.os
use tam.fmt

fn main() i32 {
    mut s = fmt::concat("x = ", "")
    let n = fmt::int_to_string(-120)
    fmt::push_str(&mut s, as_str(&n))
    fmt::push_str(&mut s, ", y = ")
    let m = fmt::int_to_string(7)
    fmt::push_str(&mut s, as_str(&m))
    os::println(as_str(&s))
    s.len as i32
}
// exit: 15
// out: x = -120, y = 7
```

---

## 5. `tam.serial`

```
pub fn put(b: u8)
pub fn print(s: str)
pub fn println(s: str)
pub fn print_int(n: i64)
```

**uefi와 bios 대상 전용**이다(x86 COM1, 포트 0x3F8). `put`은 송신 버퍼가 빌 때까지(최대 약 100만 번 확인) 기다린 뒤 한 바이트를 보낸다. BIOS 대상의 `tam.os.print`가 이 모듈 위에 만들어져 있다. UEFI에서 화면(콘솔)과 별개로 직렬 로그를 남길 때 쓴다.

```tam,ignore
use tam.serial

fn main() i32 {
    serial::println("booted")
    0
}
```

---

## 6. 내장 함수

`use` 없이 쓰는 예약된 이름이다. 표는 [Reference 14장](reference.md#14-내장-함수)에 있고, 이 절은 동작을 자세히 정의한다.

### 6.1 문자열

#### `string_from(s: str) -> String`

`s`의 바이트를 힙에 복사한 새 `String`을 만든다. 길이 = `s.len`, 용량 ≥ 길이.

#### `string_push(s: &mut String, b: u8)`

`b` 한 바이트를 끝에 붙인다. 용량이 모자라면 **2배로 늘린다**(`realloc`). 그래서 `n`번 `push`의 총 비용은 O(n)이다.

#### `as_str(s: &String) -> str`

`String`의 버퍼를 가리키는 `str`을 돌려준다. 복사가 없다. **`s`를 빌린 것으로 취급**하므로 반환된 `str`이 살아 있는 동안 `s`를 이동하거나 수정(`string_push`, 재대입)할 수 없다. 컴파일러가 이를 검사한다.

```tam
// exit: 5
fn main() i32 {
    mut s = string_from("ab")
    string_push(&mut s, 99)
    string_push(&mut s, 100)
    string_push(&mut s, 101)
    let v = as_str(&s)            // "abcde"
    if v == "abcde" { v.len as i32 } else { 0 }
}
```

### 6.2 실패

#### `panic(msg: str) -> !`

호스트에서 stderr에 `tam: panic: <msg>`를 쓰고 trap(`SIGILL`)한다. 반환하지 않으므로 어떤 타입의 자리에도 쓸 수 있다. 자동 해제는 실행되지 않는다. 다른 대상에서는 메시지 없이 trap한다.

#### `assert(cond: bool)`

`cond`가 거짓이면 `assertion failed`로 panic한다. 릴리스에서도 **제거되지 않는다**(검사를 끄는 옵션이 없다).

```tam,trap
// panic: tam: panic: out of range
fn pick(i: i32) i32 {
    if i < 0 || i > 3 { panic("out of range") }
    i * 2
}
fn main() i32 { pick(9) }
```

### 6.3 감싸는 산술

#### `wrapping_add(a: T, b: T) -> T`, `wrapping_sub`, `wrapping_mul`

정수형 `T`(`i8`…`u64`)에서 오버플로를 **감싸서**(2의 보수 모듈러) 계산한다. 기본 `+ - *`는 오버플로에서 trap하므로, 해시나 난수 생성기 같은 의도된 감싸기에 이 함수를 쓴다. 사용자가 같은 이름의 함수를 정의하면 사용자 정의가 우선한다.

```tam
// exit: 0
fn hash(s: str) u32 {
    mut h: u32 = 2166136261
    for i in 0..s.len {
        let b = unsafe { *(s.ptr + i) } as u32
        h = wrapping_mul(h ^ b, 16777619)   // FNV-1a
    }
    h
}

fn main() i32 {
    if hash("") != 2166136261 { return 1 }
    if hash("a") != 3826002220 { return 2 }
    0
}
```

### 6.4 힙(unsafe)

#### `alloc(n: u64) -> *mut u8` (unsafe)

`n`바이트의 힙 블록을 할당한다. 내용은 **초기화되지 않는다**. 정렬은 최소 16바이트다. 실패(`malloc`이 NULL)는 검사하지 않는다(호스트). BIOS/AArch64의 고정 힙이 바닥나면 trap한다.

#### `free(p: *mut u8)` (unsafe)

`alloc`이 돌려준 블록(또는 `String`의 `.ptr`)을 해제한다. 같은 블록을 두 번 해제하거나 `alloc`이 아닌 포인터를 넘기면 정의되지 않은 동작이다.

```tam
// exit: 9
fn main() i32 {
    let m = unsafe { alloc(8 * 4) }
    let q = m as *mut i64
    let r = unsafe {
        mut i: i64 = 0
        while i < 4 {
            *(q + i) = i * 2
            i += 1
        }
        let s = *q + *(q + 1) + *(q + 2) + *(q + 3) + 3  // 0+2+4+6+3
        free(m)
        s - 6
    }
    r as i32
}
```

### 6.5 하드웨어 접근 (펌웨어 전용, 모두 unsafe)

다른 대상에서 쓰면 컴파일 오류다(`` `mmio_write8` is not available when targeting host ``).

| 함수 | 대상 | 설명 |
|---|---|---|
| `mmio_read8/16/32/64(addr: u64) -> uN` | uefi, bios, aarch64 | 주소에서 volatile 읽기 |
| `mmio_write8/16/32/64(addr: u64, v: uN)` | uefi, bios, aarch64 | volatile 쓰기 |
| `port_in8(port: u16) -> u8`, `port_in32(port: u16) -> u32` | uefi, bios | x86 I/O 포트 읽기 |
| `port_out8(port: u16, v: u8)`, `port_out32(port: u16, v: u32)` | uefi, bios | x86 I/O 포트 쓰기 |
| `halt()` | uefi, bios, aarch64 | CPU를 멈춘다(`hlt`/`wfi`) |
| `semihost(op: u64, arg: u64) -> u64` | aarch64 | ARM semihosting 호출 |
| `ccall0..ccall4(f: *const u8, a.. : u64) -> u64` | uefi | MS x64 ABI로 함수 포인터 호출 |
| `efi_system_table() -> *mut u8`, `efi_image_handle() -> *mut u8` | uefi | 진입점이 받은 인자 |

volatile 접근은 컴파일러가 제거하거나 합치지 않는다. 장치 레지스터는 이것으로만 읽고 쓴다.

---

## 7. 메모리와 할당자

- 모든 힙 할당은 런타임 함수 `tam_alloc`/`tam_realloc`/`tam_free`를 거친다. 호스트는 libc의 `malloc`/`realloc`/`free`, Windows는 `HeapAlloc`/`HeapReAlloc`/`HeapFree`, UEFI는 boot services의 `AllocatePool`/`FreePool`이다.
- BIOS/AArch64는 **고정 영역(각 32MiB)**에서 크기 등급별 free-list + 범프 할당자를 쓴다. 블록 앞 16바이트에 크기를 저장하고, 블록은 합쳐지지 않으며 최대 2배까지 낭비된다. 영역이 바닥나면 trap한다. 해제된 블록은 재사용된다(128MiB를 누적 할당하는 테스트로 확인).
- **사용자가 할당자를 교체하는 문법은 아직 없다.**
- 사용자 함수 이름이 런타임 심볼(`memcpy`, `memset`, `memmove`, `tam_alloc`, `tam_realloc`, `tam_free`, `mainCRTStartup`)과 같으면 Windows 빌드에서 충돌한다.

---

## 8. 오류 코드

`Result<_, i32>`의 오류 값은 **운영체제의 오류 코드**다. 같은 의미라도 숫자가 다를 수 있으니 이식성이 필요하면 의미를 직접 번역한다.

| 의미 | Linux `errno` | Windows `GetLastError` |
|---|---|---|
| 파일/경로 없음 | 2 (`ENOENT`) | 2 (`ERROR_FILE_NOT_FOUND`), 3 (`ERROR_PATH_NOT_FOUND`) |
| 접근 거부 | 13 (`EACCES`) | 5 (`ERROR_ACCESS_DENIED`) |
| 디렉터리를 파일처럼 읽음 | 21 (`EISDIR`) | 5 |
| 이미 있음 | 17 (`EEXIST`) | 80 (`ERROR_FILE_EXISTS`) |
| 디스크 가득 | 28 (`ENOSPC`) | 112 (`ERROR_DISK_FULL`) |
| 너무 많은 열린 파일 | 24 (`EMFILE`) | 4 (`ERROR_TOO_MANY_OPEN_FILES`) |

```tam
use tam.os

// 가장 흔한 "없음"만 의미로 번역하는 예.
fn describe(code: i32) i32 {
    if code == 2 { 1 }        // 없음(Linux와 Windows 모두 2)
    else if code == 13 || code == 5 { 2 }  // 접근 거부
    else { 9 }
}

fn main() i32 {
    match os::read_file("/definitely/missing") {
        Ok(s) => 0
        Err(e) => describe(e)
    }
}
// exit: 1
```

---

## 9. 레시피

### 9.1 한 줄씩 에코

```tam
use tam.os

fn main() i32 {
    mut count = 0
    let first = os::read_line()
    os::print("> ")
    os::println(as_str(&first))
    count += 1
    count
}
// stdin: tam
// out: > tam
// exit: 1
```

### 9.2 파일 복사

```tam
use tam.os

fn copy(from: str, to: str) Result<i32, i32> {
    let data = os::read_file(from)?
    os::write_file(to, as_str(&data))
}

fn main() i32 {
    let w = os::write_file("/tmp/tam-doc-src.txt", "copy me")
    match copy("/tmp/tam-doc-src.txt", "/tmp/tam-doc-dst.txt") {
        Ok(n) => n
        Err(e) => 100 + e
    }
}
// exit: 7
```

### 9.3 정수를 글자로 모아 한 번에 출력

```tam
use tam.os
use tam.fmt

fn main() i32 {
    mut line = string_from("")
    for i in 1..=5 {
        let t = fmt::int_to_string(i as i64 * i as i64)
        fmt::push_str(&mut line, as_str(&t))
        if i < 5 { fmt::push_str(&mut line, ",") }
    }
    os::println(as_str(&line))
    0
}
// out: 1,4,9,16,25
```

### 9.4 경과 시간 재기

```tam
use tam.os

fn work() u64 {
    mut acc: u64 = 0
    for i in 0..100000 { acc = acc + i as u64 }
    acc
}

fn main() i32 {
    let t0 = os::time_ms()
    let r = work()
    let elapsed = os::time_ms() - t0
    if r == 4999950000 && elapsed < 5000 { 0 } else { 1 }
}
```

---

## 10. 표준 라이브러리 없이 쓰기와 한계

- `tam.os` 없이도 프로그램은 컴파일된다(`main`의 반환값이 종료 코드). 호스트에서 `extern "c"`로 직접 libc를 부를 수도 있다([Reference 2.5절](reference.md#25-extern-함수)).
- **없는 것**: 명령줄 인자, 환경 변수, 디렉터리 조작, 프로세스 생성, 네트워크, 스레드, 시각(달력), 난수, 부동소수 서식(`f64` 출력), `Vec`/해시맵 같은 컬렉션, 문자열 검색/분리/변환(숫자 파싱 포함), UTF-8 검증. 필요하면 `extern`과 `unsafe`로 직접 만들 수 있다.
- 호스트 `tam.os`는 Linux(glibc) 기준이다(`__errno_location`, `open` 플래그 값). macOS 등에서는 아직 검증하지 않았다.
- 펌웨어판 `tam.os`는 `print`/`println`/`print_int`/`exit`뿐이다(입력, 파일, 시간 없음). UEFI 콘솔은 ASCII만 지원한다.
- `os::exit`와 trap/패닉은 자동 해제를 실행하지 않는다.
