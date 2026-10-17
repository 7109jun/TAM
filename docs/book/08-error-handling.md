# 8. 오류 처리

TAM에는 예외가 없다. 실패할 수 있는 일은 **값**으로 표현한다.

| 상황 | 도구 |
|---|---|
| 값이 있을 수도 없을 수도 있다 | `Option<T>` |
| 성공 또는 이유 있는 실패 | `Result<T, E>` |
| 프로그램 버그, 복구 불가능한 상태 | 패닉(`panic`, `assert`, `unwrap`)과 trap |

## 8.1 `Option<T>`

값이 있으면 `Some(값)`, 없으면 `None`이다.

```tam
// exit: 2
fn find(a: &[i32], x: i32) Option<i32> {
    for i in 0..a.len {
        if a[i] == x { return Some(i as i32) }
    }
    None
}

fn main() i32 {
    let a = [5, 6, 7]
    match find(&a, 7) {
        Some(i) => i
        None => 100
    }
}
```

- `Some`, `None`은 **타입이 정해진 자리**(함수 반환, 인자, 타입을 쓴 `let`)에서 쓴다. `Some(5)`는 인자에서 타입을 추론하므로 `let o = Some(5)`도 된다. `None`만 있는 `let n = None`은 타입을 알 수 없어 `let n: Option<i32> = None`으로 쓴다.

## 8.2 `Result<T, E>`

성공이면 `Ok(값)`, 실패면 `Err(오류)`다.

```tam
// exit: 4
fn div(a: i32, b: i32) Result<i32, i32> {
    if b == 0 { return Err(1) }     // 1: 0으로 나눔
    Ok(a / b)
}

fn main() i32 {
    match div(8, 2) {
        Ok(v) => v
        Err(code) => 0 - code
    }
}
```

오류 타입 `E`는 무엇이든 된다. 관례적으로 정수 코드나 구조체를 쓴다.

## 8.3 `match`

```
match 값 {
    Ok(이름) => 식
    Err(이름) => 식
}
```

- 두 팔(`Ok`/`Err` 또는 `Some`/`None`)을 **정확히 한 번씩** 쓴다. 빠지면 컴파일 오류다.
- 각 팔의 값은 같은 타입이어야 하고 `match` 전체가 그 값을 가진다.
- 팔의 본문은 식이거나 `{ }` 블록이다.
- `Ok/Err`와 `Some/None`을 한 `match`에 섞을 수 없다.

```tam,error
// error: `match` is missing the `None` arm
fn main() i32 {
    let o: Option<i32> = None
    match o {
        Some(v) => v
    }
}
```

`match` 대상이 힙 값을 가지면 바인딩이 소유권을 **가져간다.**

```tam
// exit: 3
fn main() i32 {
    let o = Some(string_from("abc"))
    match o {
        Some(s) => s.len as i32     // s가 String을 소유. 이 팔이 끝나면 해제
        None => 0
    }
}
```

## 8.4 `?` 연산자: 오류 전파

`식?`는 `Ok(v)`면 `v`를 꺼내고, `Err(e)`면 **현재 함수에서 즉시 `Err(e)`를 반환**한다. 오류를 일일이 `match`하지 않아도 된다.

```tam
// exit: 25
fn div(a: i32, b: i32) Result<i32, i32> {
    if b == 0 { return Err(1) }
    Ok(a / b)
}

fn calc(a: i32, b: i32, c: i32) Result<i32, i32> {
    let x = div(a, b)?           // 실패하면 여기서 반환
    let y = div(x, c)?
    Ok(x + y)
}

fn main() i32 {
    match calc(100, 5, 4) {      // 20 + 5
        Ok(v) => v
        Err(e) => 100 + e
    }
}
```

규칙:

- 함수의 반환 타입이 `Result<_, E>`이고 **오류 타입 `E`가 정확히 같아야** 한다. 자동 변환(`From`)은 없다.
- `Option`을 돌려주는 함수 안에서는 `Option`에 `?`를 쓴다(`None`이면 즉시 `None` 반환). `Option`과 `Result`를 섞을 수 없다.
- `?`로 일찍 빠져나갈 때도 **살아 있는 지역 변수는 모두 해제**된다.

```tam,error
// error: `?` can only be used
fn f() Result<i32, i32> { Ok(1) }

fn main() i32 {
    let x = f()?
    x
}
```

(`main`은 `i32`를 돌려주므로 `?`를 쓸 수 없다. 최상위에서는 `match`로 처리한다.)

### 오류 타입이 다르면

오류 타입을 맞춰 주는 변환 함수를 직접 쓴다.

```tam
// exit: 12
fn parse(n: i32) Result<i32, i32> {      // 오류 코드 i32
    if n < 0 { Err(1) } else { Ok(n) }
}

fn check(n: i32) Result<i32, i64> {      // 오류 코드 i64
    match parse(n) {
        Ok(v) => Ok(v * 2)
        Err(e) => Err(e as i64)
    }
}

fn main() i32 {
    match check(6) { Ok(v) => v, Err(e) => e as i32 }
}
```

### 오류에도 소유권이 있다

`Result<String, i32>`처럼 힙 값을 담으면 `Result` 자체가 이동 타입이다. `?`가 성공 값을 꺼낼 때 소유권이 넘어온다.

```tam
// exit: 5
fn make(ok: bool) Result<String, i32> {
    if !ok { return Err(9) }
    Ok(string_from("hello"))
}

fn length() Result<i32, i32> {
    let s = make(true)?            // String의 소유권을 받는다
    Ok(s.len as i32)               // s는 함수가 끝날 때 해제
}

fn main() i32 {
    match length() { Ok(n) => n, Err(e) => e }
}
```

## 8.5 편의 메서드

| 메서드 | `Option<T>` | `Result<T, E>` |
|---|:-:|:-:|
| `is_some()` / `is_none()` | ✔ | |
| `is_ok()` / `is_err()` | | ✔ |
| `unwrap()` | `None`이면 패닉 | `Err`이면 패닉 |
| `unwrap_or(기본)` | ✔ | ✔ |

```tam
// exit: 0
fn main() i32 {
    let a: Option<i32> = Some(4)
    let b: Option<i32> = None
    if a.unwrap() != 4 { return 1 }
    if b.unwrap_or(9) != 9 { return 2 }
    if !a.is_some() || !b.is_none() { return 3 }
    let r: Result<i32, i32> = Err(3)
    if r.is_ok() { return 4 }
    if r.unwrap_or(0) != 0 { return 5 }
    0
}
```

`is_*`는 힙을 가진 내용(`Option<String>`)에는 쓸 수 없다(값을 소비하기 때문). `match`를 쓴다. `unwrap`은 값을 꺼내 소유권을 가져온다.

`map`, `and_then` 같은 조합 메서드는 아직 없다(클로저가 없다).

## 8.6 패닉: 복구하지 않는 실패

버그나 깨진 불변식은 `Result`로 돌려줄 일이 아니다. 프로그램을 즉시 멈춘다.

```tam,trap
// panic: tam: panic: negative input
fn root(n: i32) i32 {
    if n < 0 { panic("negative input") }
    n
}

fn main() i32 { root(-1) }
```

```tam,trap
// panic: assertion failed
fn main() i32 {
    let x = 2
    assert(x + x == 5)
    0
}
```

```tam,trap
// panic: called `unwrap()` on a `None` value
fn none() Option<i32> { None }

fn main() i32 { none().unwrap() }
```

호스트에서 패닉은 stderr에 `tam: panic: 메시지`를 쓰고 `SIGILL`로 끝난다. 다음도 같은 방식으로 멈춘다: 정수 오버플로, 0 나누기, 배열 경계 위반. 자동 해제는 실행되지 않는다.

### 언제 무엇을 쓰나

- **입력이나 환경 때문에 실패할 수 있다**(파일이 없다, 파싱 실패) → `Result`.
- **값이 없는 것이 정상이다**(찾았더니 없다) → `Option`.
- **내가 호출 규칙을 어겼다**(범위 밖 인덱스, 불가능한 상태) → `assert`/`panic`.

## 8.7 예: 파일 읽기

`tam.os`의 함수는 실패를 `Result<_, i32>`(OS 오류 코드)로 돌려준다.

```tam
// exit: 2
use tam.os

fn load(path: str) Result<i32, i32> {
    let text = os::read_file(path)?
    Ok(text.len as i32)
}

fn main() i32 {
    match load("/definitely/missing") {
        Ok(n) => n
        Err(code) => code            // ENOENT = 2
    }
}
```

→ [9. 모듈과 프로젝트](09-modules-and-projects.md)
