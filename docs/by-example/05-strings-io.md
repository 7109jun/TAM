# 5. 문자열과 입출력

## `str`과 `String`

`str`은 빌린 문자열(리터럴 포함), `String`은 힙에 소유한 문자열이다.

```tam
// exit: 5
fn main() i32 {
    let lit: str = "hello"
    let owned = string_from(lit)       // 힙에 복사
    if lit.len != owned.len { return 0 }
    owned.len as i32
}
```

## 글자 덧붙이기

```tam
// out: abc
use tam.os

fn main() i32 {
    mut s = string_from("")
    string_push(&mut s, 97)
    string_push(&mut s, 98)
    string_push(&mut s, 99)
    os::println(as_str(&s))
    0
}
```

## `tam.fmt`: 이어 붙이기

```tam
// out: x = 42
use tam.os
use tam.fmt

fn main() i32 {
    mut s = fmt::concat("x = ", "")
    let n = fmt::int_to_string(42)
    fmt::push_str(&mut s, as_str(&n))
    os::println(as_str(&s))
    0
}
```

## 숫자 출력

```tam
// out: -7
use tam.os

fn main() i32 {
    os::print_int(0 - 7)
    os::println("")
    0
}
```

## 바이트 세기

문자열은 바이트 열이다. 한 바이트씩 읽으려면 `unsafe`가 필요하다.

```tam
// exit: 3
fn count(s: str, b: u8) i32 {
    mut n = 0
    for i in 0..s.len {
        if unsafe { *(s.ptr + i) } == b { n += 1 }
    }
    n
}

fn main() i32 { count("banana", 97) }          // 'a'
```

## 줄 읽기

```tam
// stdin: hello world
// out: you said: hello world
use tam.os

fn main() i32 {
    let line = os::read_line()
    os::print("you said: ")
    os::println(as_str(&line))
    0
}
```

## 입력이 끝날 때까지

빈 줄(또는 EOF)을 끝으로 본다.

```tam
// stdin: a
// stdin: bb
// stdin: ccc
// exit: 6
use tam.os

fn main() i32 {
    mut total = 0
    while true {
        let line = os::read_line()
        if line.len == 0 { break }
        total += line.len as i32
    }
    total
}
```

## 파일 쓰기와 읽기

```tam
// exit: 5
use tam.os

fn main() i32 {
    let path = "/tmp/tam_by_example.txt"
    match os::write_file(path, "hello") {
        Ok(_n) => {}
        Err(e) => { return e }
    }
    match os::read_file(path) {
        Ok(text) => text.len as i32
        Err(e) => e
    }
}
```

## 없는 파일

```tam
// exit: 2
use tam.os

fn main() i32 {
    match os::read_file("/no/such/file") {
        Ok(_t) => 0
        Err(e) => e              // ENOENT
    }
}
```

## 걸린 시간 재기

```tam
// exit: 0
use tam.os

fn main() i32 {
    let t0 = os::time_ms()
    mut x: u64 = 0
    for i in 0..1000 { x += i as u64 }
    let t1 = os::time_ms()
    if t1 < t0 { return 1 }
    if x != 499500 { return 2 }
    0
}
```
