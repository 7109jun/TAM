# 1. 기초

## Hello, world

```tam
// out: Hello, world!
use tam.os

fn main() i32 {
    os::println("Hello, world!")
    0
}
```

`main`은 `i32`를 반환하고, 그 값이 종료 코드다. 마지막 식이 반환값이다.

## 변수

`let`은 불변, `mut`는 가변이다. 같은 이름을 다시 선언(섀도잉)할 수 있다.

```tam
// exit: 12
fn main() i32 {
    let a = 5
    mut b = a
    b += 3
    let b = b + 4        // 새 변수(섀도잉)
    b
}
```

## 정수형과 변환

암묵적 변환은 없다. `as`로 명시한다.

```tam
// exit: 44
fn main() i32 {
    let big: i64 = 300
    let small = big as u8      // 300 mod 256 = 44
    small as i32
}
```

## 산술과 비트 연산

```tam
// exit: 0
fn main() i32 {
    if 7 / 2 != 3 { return 1 }          // 정수 나눗셈
    if 7 % 3 != 1 { return 2 }
    if (0 - 7) / 2 != 0 - 3 { return 3 } // 0 방향으로 자른다
    if (6 & 3) != 2 { return 4 }
    if (6 | 3) != 7 { return 5 }
    if (6 ^ 3) != 5 { return 6 }
    if (1 << 4) != 16 { return 7 }
    0
}
```

## 부동소수점

```tam
// exit: 7
fn main() i32 {
    let x: f64 = 3.5
    let y = x * 2.0
    y as i32
}
```

## if는 식이다

```tam
// exit: 9
fn max(a: i32, b: i32) i32 {
    if a > b { a } else { b }
}

fn main() i32 { max(4, 9) }
```

## while과 break/continue

```tam
// exit: 25
fn main() i32 {
    mut i = 0
    mut sum = 0
    while true {
        i += 1
        if i > 10 { break }
        if i % 2 == 0 { continue }
        sum += i                 // 1+3+5+7+9
    }
    sum
}
```

## for와 범위

`a..b`는 `a`부터 `b` 직전까지다.

```tam
// exit: 10
fn main() i32 {
    mut s = 0
    for i in 0..5 { s += i }
    s
}
```

## 함수와 재귀

```tam
// exit: 120
fn fact(n: i64) i64 {
    if n <= 1 { return 1 }
    n * fact(n - 1)
}

fn main() i32 { fact(5) as i32 }
```

## 논리 연산과 단락 평가

`&&`, `||`는 왼쪽만으로 결정되면 오른쪽을 평가하지 않는다.

```tam
// exit: 1
fn boom() bool { panic("not reached") }

fn main() i32 {
    if false && boom() { return 0 }
    if true || boom() { return 1 }
    0
}
```

## 오버플로는 trap한다

```tam,trap
// panic: integer overflow
fn main() i32 {
    mut x: i32 = 2147483647
    x += 1
    x
}
```
