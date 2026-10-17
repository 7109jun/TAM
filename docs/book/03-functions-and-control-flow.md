# 3. 함수와 제어 흐름

## 3.1 함수

```tam
// exit: 7
fn add(a: i32, b: i32) i32 {
    a + b
}

fn main() i32 {
    add(3, 4)
}
```

- 매개변수와 반환 타입은 **반드시 쓴다**(함수 시그니처는 TAM에서 추론하지 않는다).
- 반환 타입은 `)` 뒤에 화살표 없이 쓴다.
- 본문 블록의 **마지막 식**이 반환값이다. 그 줄에 세미콜론도 `return`도 붙이지 않는다.
- 반환 타입을 생략한 함수는 값을 돌려주지 않는다.
- 함수는 파일 안 어디에 정의해도 보인다(호출보다 뒤에 있어도 된다).

### `return`은 조기 반환

```tam
// exit: 3
fn sign(x: i32) i32 {
    if x < 0 { return -1 }
    if x == 0 { return 0 }
    1
}

fn main() i32 {
    sign(-5) + sign(0) + sign(9) + 3
}
```

### 매개변수는 불변이다

값을 바꿔야 하면 본문에서 가변 변수로 다시 묶는다.

```tam
// exit: 10
fn count_to(limit: i32) i32 {
    mut i = 0
    while i < limit { i += 1 }
    i
}

fn main() i32 { count_to(10) }
```

값이 없는 함수는 반환 타입을 쓰지 않는다. 그 호출 결과를 변수에 묶을 수는 없다.

```tam
// exit: 0
fn note(x: i32) {
    if x > 100 { return }
}

fn main() i32 {
    note(5)
    0
}
```

### 재귀

```tam
// exit: 55
fn fib(n: i32) i32 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

fn main() i32 { fib(10) }
```

## 3.2 `if`는 표현식이다

`if`는 값을 가진다. 양쪽 가지의 타입이 같아야 한다. 조건은 `bool`이어야 한다.

```tam
// exit: 1
fn main() i32 {
    let x = 5
    let kind = if x > 3 { 1 } else { 2 }
    kind
}
```

`else if`로 이어 쓸 수 있고, `else` 없는 `if`는 값이 없는 문장이다.

```tam
// exit: 20
fn main() i32 {
    let n = 15
    mut r = 0
    if n < 10 {
        r = 10
    } else if n < 20 {
        r = 20
    } else {
        r = 30
    }
    r
}
```

## 3.3 줄바꿈과 문장

TAM은 **줄바꿈이 문장의 끝**이다. 한 줄에 여러 문장을 쓰려면 `;`로 구분한다. 줄 끝이 연산자나 `,`, `(`, `[`, `{`이면 다음 줄로 이어지고, 다음 줄이 `.`으로 시작해도 이어진다.

```tam
// exit: 6
fn main() i32 {
    let a = 1; let b = 2
    let total = a +
        b +
        3
    total
}
```

주의: **다음 줄이 연산자로 시작하면 이어지지 않는다.** 연산자는 줄 끝에 둔다.

```tam,error
// error: expected an expression, found `+`
fn main() i32 {
    let x = 1
        + 2
    x
}
```

### 주석

`//`는 줄 끝까지, `/* ... */`는 블록 주석이며 **중첩된다**. 여러 줄에 걸친 블록 주석은 줄바꿈 하나로 취급한다.

```tam
// exit: 7
/* 블록 주석 /* 중첩 */ 도 된다 */
fn main() i32 {
    let x = 4 /* 문장 중간의 주석 */ + 3
    x
}
```

## 3.4 반복: `while`

```tam
// exit: 45
fn main() i32 {
    mut i = 0
    mut sum = 0
    while i < 10 {
        sum += i
        i += 1
    }
    sum
}
```

`break`는 가장 안쪽 루프를 빠져나가고 `continue`는 다음 반복으로 간다.

```tam
// exit: 16
fn main() i32 {
    mut i = 0
    mut s = 0
    while i < 10 {
        i += 1
        if i % 2 == 0 { continue }   // 짝수는 건너뛴다
        if i > 7 { break }           // 7을 넘으면 끝
        s += i                       // 1 + 3 + 5 + 7
    }
    s
}
```

## 3.5 반복: `for`

```
for i in 0..5 { }     // 0, 1, 2, 3, 4 (끝 제외)
for i in 0..=5 { }    // 0 … 5 (끝 포함)
```

```tam
// exit: 15
fn main() i32 {
    mut s = 0
    for i in 1..=5 { s += i }
    s
}
```

- 루프 변수 `i`는 **불변**이다(바꿀 수 없다).
- 끝점이 정수 리터럴이면 `i`는 `i32`다. 다른 정수형이 필요하면 끝점을 그 타입의 변수로 쓴다.
- 슬라이스와 배열에도 쓴다(`for x in &a[..]`, [7장](07-arrays-and-slices.md)).
- 이터레이터는 없다. 범위와 슬라이스만 된다.

### `for`와 정수형

```tam
// exit: 10
fn main() i32 {
    let n: u64 = 5
    mut s: u64 = 0
    for i in 0..n { s += i }      // i는 u64
    s as i32
}
```

### 중첩 루프에서의 `break`

`break`는 **안쪽** 루프만 끝낸다. 레이블은 없다.

```tam
// exit: 6
fn main() i32 {
    mut found = 0
    for i in 0..4 {
        for j in 0..4 {
            if j > i { break }       // 안쪽 루프만 끝
            found += 1
        }
    }
    found - 4
}
```

### 무한 반복: `loop`

`loop { ... }`는 `while true { ... }`와 같다. `break`나 `return`으로 빠져나간다.

```tam
// exit: 13
fn find() i32 {
    mut i = 0
    loop {
        i += 1
        if i == 5 { return i }
    }                              // break가 없는 loop는 끝나지 않으므로 뒤에 값이 필요 없다
}

fn main() i32 {
    mut n = 0
    loop {
        n += 2
        if n > 7 { break }
    }
    n + find()                     // 8 + 5
}
```

본문에 `break`가 하나라도 있으면(이 루프를 끝내는 것) `loop` 뒤로 흐르는 길이 생기므로, 값을 돌려주는 함수라면 그 뒤에 최종 식이 있어야 한다.

## 3.6 값을 돌려주는 패턴

TAM에는 `break 값`이 없다. 값을 찾는 루프는 변수와 함께 쓴다.

```tam
// exit: 5
fn first_square_over(limit: i32) i32 {
    mut n = 1
    while n * n <= limit { n += 1 }
    n
}

fn main() i32 { first_square_over(20) }   // 5*5 = 25 > 20
```

## 3.7 연습

1. 1부터 100까지 3 또는 5의 배수의 합을 구하는 프로그램을 쓰라. (답: 2418)
2. 두 수의 최대공약수를 재귀로 구하는 `fn gcd(a: i64, b: i64) i64`를 쓰라.

```tam
// exit: 6
fn gcd(a: i64, b: i64) i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn main() i32 { gcd(48, 18) as i32 }
```

```tam
// exit: 0
fn main() i32 {
    mut s = 0
    for i in 1..=100 {
        if i % 3 == 0 || i % 5 == 0 { s += i }
    }
    if s == 2418 { 0 } else { 1 }
}
```

→ [4. 소유권](04-ownership.md)
