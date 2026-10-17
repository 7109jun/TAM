# 2. 값과 변수

## 2.1 변수: `let`과 `mut`

변수는 `let`으로 선언한다. **기본은 불변**이다. 값을 바꾸려면 `mut`으로 선언한다(`let mut`이 아니다).

```tam
// exit: 7
fn main() i32 {
    let a = 5        // 불변
    mut b = 1        // 가변
    b = b + 1
    b += 1           // 복합 대입
    a + b - 1
}
```

불변 변수에 대입하면 컴파일 오류다.

```tam,error
// error: cannot assign to immutable variable `x`
fn main() i32 {
    let x = 5
    x = 6
    x
}
```

변수의 타입은 **추론**된다. 타입을 직접 쓰고 싶으면 이름 뒤에 `: 타입`을 붙인다.

```tam
// exit: 0
fn main() i32 {
    let a = 10          // i32 (정수 리터럴의 기본형)
    let b: u8 = 200     // 명시
    mut c: i64 = 1
    c += 4_000_000_000  // i64라서 들어간다
    if c != 4_000_000_001 { return 1 }
    if (b as i32) + a != 210 { return 2 }
    0
}
```

### 섀도잉

같은 이름으로 다시 `let`하면 이전 변수가 **가려진다.** 타입이 달라도 된다.

```tam
// exit: 6
fn main() i32 {
    let x = 3
    let x = x * 2     // 새 변수. 이전 x는 가려진다
    x
}
```

## 2.2 기본 타입

| 종류 | 타입 | 비고 |
|---|---|---|
| 부호 있는 정수 | `i8` `i16` `i32` `i64` | 정수 리터럴의 기본형은 `i32` |
| 부호 없는 정수 | `u8` `u16` `u32` `u64` | 크기·길이(`.len`)는 `u64` |
| 실수 | `f64` | `f32`는 없다 |
| 참/거짓 | `bool` | `true`, `false` |
| 문자열 조각 | `str` | `"리터럴"`의 타입. 읽기 전용 |
| 소유 문자열 | `String` | 힙에 있다. [4장](04-ownership.md) |

### 정수

정수 리터럴은 `42`, `1_000_000`(밑줄은 구분용), `0xFF`, `0o17`, `0b1010`로 쓴다. 문자 리터럴 `'a'`는 정수 값(97)이다.

**정수끼리의 연산은 같은 타입이어야 한다.** 다른 타입은 `as`로 맞춘다.

```tam
// exit: 0
fn main() i32 {
    let small: i32 = 7
    let big: i64 = 3_000_000_000
    let sum = small as i64 + big
    if sum != 3_000_000_007 { return 1 }
    if 'a' as i32 != 97 { return 2 }
    0
}
```

### 실수와 불리언

```tam
// exit: 17
fn main() i32 {
    let r = 2.5 * 4.0 + 7.0     // 17.0
    let big = 1.5e3             // 1500.0
    let ok = r > 10.0 && big > 1000.0
    if ok { r as i32 } else { 0 }
}
```

`as`로 실수를 정수로 바꾸면 0 방향으로 잘리고, 범위를 넘으면 **포화**한다(`1e20 as i32`는 `i32`의 최댓값). `NaN`은 0이다. 정수를 `bool`로 바꾸는 `as`는 없다(`x != 0`을 쓴다).

### 문자열 조각 `str`

문자열 리터럴은 `str`이다. 길이를 `.len`으로 읽는다(`u64`). **바이트 수**이므로 UTF-8 한글 한 글자는 3이다.

```tam
// exit: 0
fn main() i32 {
    let s = "한글"
    if s.len != 6 { return 1 }
    if s == "한글" && s != "영어" { 0 } else { 2 }
}
```

## 2.3 연산자

산술 `+ - * / %`, 비교 `== != < <= > >=`, 논리 `&& || !`(단락 평가), 비트 `& | ^ ~ << >>`다. `if`의 조건은 **반드시 `bool`**이다(0이 거짓인 규칙은 없다).

우선순위는 Rust와 같다. 비트 연산자가 비교보다 **높다**(C와 다르다).

```tam
// exit: 0
fn main() i32 {
    let flags = 0b1010
    if flags & 0b0010 == 0b0010 { 0 } else { 1 }   // (flags & 2) == 2
}
```

### 정수 나눗셈과 나머지

나눗셈은 0 방향으로 자르고, `%`의 부호는 왼쪽 피연산자를 따른다.

```tam
// exit: 0
fn main() i32 {
    if 7 / 2 != 3 { return 1 }
    if -7 / 2 != -3 { return 2 }
    if -7 % 3 != -1 { return 3 }
    0
}
```

## 2.4 오버플로는 멈춘다

C에서 정수 오버플로는 정의되지 않은 동작이고, 많은 언어는 값을 조용히 감싼다. **TAM은 멈춘다**(trap). 실행 중 `+ - *`가 범위를 넘으면 프로그램이 `integer overflow`로 끝난다. 0으로 나누거나 `MIN / -1`도 마찬가지다. 조용히 틀린 결과를 만드는 것보다 낫다는 철학이다.

```tam,trap
// panic: integer overflow
fn main() i32 {
    let a: i32 = 2147483647
    a + 1
}
```

```console
$ tamc run overflow.tam
tam: panic: integer overflow
error: the program was terminated by signal 4
```

**감싸기를 원하면 명시한다.** 해시나 난수 생성기처럼 모듈러 산술이 의도일 때 `wrapping_add`, `wrapping_sub`, `wrapping_mul`을 쓴다.

```tam
// exit: 0
fn main() i32 {
    let a: u8 = 250
    let b = wrapping_add(a, 10)      // 260 mod 256 = 4
    if b == 4 { 0 } else { 1 }
}
```

`as` 변환은 trap하지 않고 감싸거나 포화한다(`300 as u8`은 44).

```tam
// exit: 0
fn main() i32 {
    let big = 300
    if big as u8 != 44 { return 1 }
    0
}
```

## 2.5 정수 리터럴의 타입 결정

리터럴은 **놓인 자리의 기대 타입**을 따른다. 기대 타입이 없으면 `i32`다. 값이 들어가지 않으면 컴파일 오류다.

```tam,error
// error: integer literal out of range for `u8`
fn main() i32 {
    let x: u8 = 300
    0
}
```

가장 큰 `u64` 값은 `0xFFFF_FFFF_FFFF_FFFF`로 쓰거나 `~0`(비트 반전)을 쓴다. 각 타입의 최솟값(`-128` for `i8`)도 리터럴로 쓸 수 있다.

## 2.6 상수: `const`

바뀌지 않는 값에는 전역 `const`를 쓴다. **타입을 반드시 쓰고**, 값은 **컴파일 시간에 계산**된다.

```tam
// exit: 9
const WIDTH: i32 = 3
const HEIGHT: i32 = 2
const AREA: i32 = WIDTH * HEIGHT       // 다른 상수와 연산
const LIMIT: u64 = 1 << 20
const GREETING: str = "hi"

fn main() i32 {
    if LIMIT != 1048576 { return 1 }
    AREA + GREETING.len as i32 + 1      // 6 + 2 + 1
}
```

- 정수형, `f64`, `bool`, `str`만 상수가 될 수 있다.
- 쓸 수 있는 것은 리터럴, 다른 상수, 연산자뿐이다(함수 호출은 안 된다).
- 값이 타입에 안 들어가면 컴파일 오류다: `const A: u8 = 256`.
- 상수에는 대입할 수 없다. 같은 이름의 지역 변수는 상수를 가린다.
- 다른 모듈에서 쓰려면 `pub const`로 선언한다.
- **배열 길이**에 쓸 수 있다. 그래서 크기를 한 곳에서 정한다.

```tam
// exit: 10
const N: u64 = 5

fn main() i32 {
    let a = [2; N]                      // [i32; 5]
    mut s = 0
    for i in 0..N { s += a[i] }
    s
}
```

상수는 정해진 타입을 가진다. 정수 리터럴처럼 문맥에 따라 타입이 변하지 않으므로 다른 정수형이 필요하면 `as`를 쓴다.

## 2.7 연습

1. `let x = 250`; `let y: u8 = x`는 왜 오류인가?
2. `1 << 33`은 `i32`에서 얼마인가? ([Reference 4.4절](../reference.md#44-비트-연산자)에 규칙이 있다.)
3. 다음 중 trap하는 것은? `5 / 0`, `5.0 / 0.0`, `200u8 + 100`(리터럴 접미사는 없다 — `let a: u8 = 200; a + 100`).

(답: 1) `x`는 `i32`이고 `u8`로의 암묵 변환은 없다. 2) 시프트 양은 31로 마스크되므로 `1 << 1` = 2. 3) `5 / 0`과 `a + 100`만 trap한다. 실수 나눗셈은 IEEE 규칙대로 무한대가 나온다.)

→ [3. 함수와 제어 흐름](03-functions-and-control-flow.md)
