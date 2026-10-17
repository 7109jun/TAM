# 12. 프로젝트: 줄 합계 도구

지금까지 배운 것을 모아 작은 도구를 만든다. 표준 입력에서 줄마다 정수를 읽어 합계를 낸다. 잘못된 줄은 **멈추지 않고 보고**하고 끝에 종료 코드로 알린다.

```console
$ printf '10\n20\n-5\n' | tamc run
sum = 25
```

설계:

- `parse-int` 라이브러리 패키지: 문자열을 정수로 바꾸는 **안전한** 함수(`Result`). `unsafe`는 여기에만 둔다.
- `app` 실행 패키지: 입력을 읽고 합계를 낸다.

## 12.1 라이브러리: 문자열 → 정수

먼저 어떤 입력이 실패인지 정한다. 오류 코드는 정수로 한다.

| 코드 | 의미 |
|---|---|
| 1 | 빈 문자열이거나 `-`만 있다 |
| 2 | 숫자가 아닌 글자가 있다 |
| 3 | 너무 길다(18자리 초과: `i64`에서 오버플로할 수 있다) |

오버플로는 trap이므로 **계산 전에 길이로 막는다.**

```tam
// exit: 0
fn byte(s: str, i: u64) u8 {
    unsafe { *(s.ptr + i) }          // 호출자가 i < s.len을 보장한다
}

pub fn parse_int(s: str) Result<i64, i32> {
    mut i: u64 = 0
    mut neg = false
    if s.len > 0 && byte(s, 0) == 45 {      // '-'
        neg = true
        i = 1
    }
    if i == s.len { return Err(1) }
    if s.len - i > 18 { return Err(3) }
    mut v: i64 = 0
    while i < s.len {
        let c = byte(s, i)
        if c < 48 || c > 57 { return Err(2) }
        v = v * 10 + ((c - 48) as i64)
        i += 1
    }
    if neg { Ok(0 - v) } else { Ok(v) }
}

fn main() i32 {
    if parse_int("42").unwrap() != 42 { return 1 }
    if parse_int("-7").unwrap() != 0 - 7 { return 2 }
    if parse_int("").is_ok() { return 3 }
    if parse_int("-").is_ok() { return 4 }
    if parse_int("4x").is_ok() { return 5 }
    if parse_int("1234567890123456789").is_ok() { return 6 }
    0
}
```

`byte`는 `pub`이 아니므로 라이브러리 밖에서 쓸 수 없다. `unsafe`가 이 파일 안에 갇힌다. 경계는 `while i < s.len`이 지켜 준다.

## 12.2 앱: 읽고 더하기

`os::read_line()`은 EOF와 빈 줄을 구분하지 않으므로, **빈 줄을 입력의 끝**으로 삼는다.

<!-- project run stdin=10\n20\nabc\n-5\n exit=1 out=sum = 25 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
parse-int = { path = "../parse-int" }
```

```tam,file=app/src/main.tam
use tam.os
use parse_int

fn main() i32 {
    mut sum: i64 = 0
    mut bad: i32 = 0
    mut line_no: i64 = 0

    while true {
        let line = os::read_line()
        if line.len == 0 { break }
        line_no += 1
        match parse_int::parse_int(as_str(&line)) {
            Ok(v) => { sum += v }
            Err(code) => {
                os::print("line ")
                os::print_int(line_no)
                os::print(": error ")
                os::print_int(code as i64)
                os::println("")
                bad += 1
            }
        }
    }

    os::print("sum = ")
    os::print_int(sum)
    os::println("")
    if bad > 0 { 1 } else { 0 }
}
```

```toml,file=parse-int/Tmake.toml
[package]
name = "parse-int"
version = "0.1.0"
kind = "lib"
```

```tam,file=parse-int/src/lib.tam
fn byte(s: str, i: u64) u8 {
    unsafe { *(s.ptr + i) }
}

pub fn parse_int(s: str) Result<i64, i32> {
    mut i: u64 = 0
    mut neg = false
    if s.len > 0 && byte(s, 0) == 45 {
        neg = true
        i = 1
    }
    if i == s.len { return Err(1) }
    if s.len - i > 18 { return Err(3) }
    mut v: i64 = 0
    while i < s.len {
        let c = byte(s, i)
        if c < 48 || c > 57 { return Err(2) }
        v = v * 10 + ((c - 48) as i64)
        i += 1
    }
    if neg { Ok(0 - v) } else { Ok(v) }
}
```
<!-- /project -->

입력 `10`, `20`, `abc`, `-5`로 실행하면:

```console
$ printf '10\n20\nabc\n-5\n' | tamc run
line 3: error 2
sum = 25
$ echo $?
1
```

짚어 볼 점:

- **`?` 대신 `match`**: `main`은 `i32`를 돌려주므로 `?`를 못 쓴다. 오류를 모으며 계속 가려고 일부러 `match`를 썼다.
- **소유권**: `line`은 반복마다 새로 만들어지고 반복이 끝나면 자동으로 해제된다. `as_str(&line)`은 빌림이므로 `line`을 쓰는 동안만 유효하다.
- **`break`**: `while true`를 `break`로 나온다.
- **오류 코드**: 파싱의 실패 사유는 값이다. 버그가 아닌 실패는 trap하지 않는다.

## 12.3 테스트하기

TAM에는 내장 테스트 러너가 없다. 가장 간단한 방법은 라이브러리를 의존성으로 가져와 **점검용 `main`을 가진 별도 패키지**를 두는 것이다. `assert`가 실패하면 trap하고, 모두 통과하면 종료 코드 0이다. 12.1의 `main`이 그 모양이다. `tamc run`의 종료 코드를 CI가 그대로 판정에 쓸 수 있다.

## 12.4 확장해 보기

- 공백으로 구분된 한 줄의 여러 정수를 처리한다.
- 파일 이름을 입력으로 받는다(`os::read_file`).
- 오류 보고에 틀린 줄의 내용도 출력한다.
- `--release`로 빌드해 속도를 비교한다. 이 도구는 한 바이트씩 읽으므로 입력이 크면 입력이 병목이다.

→ [13. 제네릭](13-generics.md)
