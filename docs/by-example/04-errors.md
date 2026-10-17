# 4. 오류 처리

## `Option`: 없을 수도 있다

```tam
// exit: 2
fn find(a: &[i32], x: i32) Option<u64> {
    for i in 0..a.len {
        if a[i] == x { return Some(i) }
    }
    None
}

fn main() i32 {
    let a = [4, 5, 6]
    match find(&a, 6) {
        Some(i) => i as i32
        None => 0 - 1
    }
}
```

## `Result`: 실패에 이유가 있다

```tam
// exit: 5
fn half(n: i32) Result<i32, i32> {
    if n % 2 != 0 { return Err(n) }
    Ok(n / 2)
}

fn main() i32 {
    match half(10) {
        Ok(v) => v
        Err(_bad) => 0
    }
}
```

## `?`로 전파

```tam
// exit: 14
fn parse_digit(c: u8) Result<i32, i32> {
    if c < 48 || c > 57 { return Err(1) }
    Ok((c - 48) as i32)
}

fn two(a: u8, b: u8) Result<i32, i32> {
    let x = parse_digit(a)?
    let y = parse_digit(b)?
    Ok(x * 10 + y)
}

fn main() i32 {
    let ok = two(49, 52).unwrap_or(0)         // 14
    let bad = two(49, 120).unwrap_or(0)       // 0
    ok + bad
}
```

## `unwrap_or`와 `is_*`

```tam
// exit: 0
fn main() i32 {
    let a: Option<i32> = None
    let r: Result<i32, i32> = Ok(3)
    if a.unwrap_or(7) != 7 { return 1 }
    if a.is_some() { return 2 }
    if !r.is_ok() { return 3 }
    0
}
```

## 오류에 소유권 있는 값 담기

```tam
// exit: 3
fn greet(ok: bool) Result<String, String> {
    if ok { Ok(string_from("hey")) } else { Err(string_from("no")) }
}

fn main() i32 {
    match greet(true) {
        Ok(s) => s.len as i32
        Err(e) => e.len as i32 + 100
    }
}
```

## 패닉

복구할 수 없는 버그는 `panic`이나 `assert`로 멈춘다.

```tam,trap
// panic: invalid state
fn main() i32 {
    let state = 3
    if state > 2 { panic("invalid state") }
    0
}
```

## 배열 경계

```tam,trap
// panic: index out of bounds
fn main() i32 {
    let a = [1, 2, 3]
    let i = a.len
    a[i]
}
```

## 0으로 나누기

```tam,trap
// panic: division by zero
fn main() i32 {
    let d = 0
    10 / d
}
```
