# 9. 상수, 제네릭, 루프

## 상수

```tam
// exit: 14
const N: i32 = 4
const MASK: u32 = 0xFF & 0x0F
const DOUBLE: i32 = N * 2

fn main() i32 {
    let a = [1; N]                    // 길이 4
    let m = MASK as i32               // 15
    DOUBLE + a[3] + m - 10            // 8 + 1 + 15 - 10
}
```

## 상수로 배열 길이

```tam
// exit: 10
const LEN: i32 = 5

fn sum(a: &[i32; LEN]) i32 {
    mut s = 0
    for x in &a[..] { s += x }
    s
}

fn main() i32 {
    let a = [2; LEN]
    sum(&a)
}
```

## `loop`

```tam
// exit: 8
fn main() i32 {
    mut n = 0
    loop {
        n += 2
        if n > 7 { break }
    }
    n
}
```

## 블록 주석

```tam
// exit: 3
/* 앞 /* 중첩 */ 뒤 */
fn main() i32 { 1 /* 중간 */ + 2 }
```

## 제네릭 함수

```tam
// exit: 9
fn max<T>(a: T, b: T) T {
    if a > b { a } else { b }
}

fn main() i32 {
    let x: i64 = max(5, 7)
    max<f64>(1.5, 0.5) as i32 + max(3, 9) + x as i32 - 8
}
```

## 제네릭 구조체

```tam
// exit: 7
struct Pair<A, B> {
    first: A,
    second: B,
}

impl Pair<A, B> {
    fn new(a: A, b: B) Self { Pair { first: a, second: b } }
    fn swap(self) Pair<B, A> { Pair { first: self.second, second: self.first } }
}

fn main() i32 {
    let p = Pair::new(3, 4)
    let q = p.swap()
    q.first + q.second - 4 + 4
}
```

## 제네릭과 `String`

```tam
// exit: 5
struct Holder<T> { v: T }

impl Holder<T> {
    fn new(v: T) Self { Holder { v: v } }
    fn get(&self) &T { &self.v }
}

fn main() i32 {
    let h = Holder::new(string_from("hello"))
    let s = h.get()
    s.len as i32
}
```

## 인스턴스 오류

```tam,error
// error: operator `>` cannot be applied to `P`
struct P { x: i32 }

fn max<T>(a: T, b: T) T { if a > b { a } else { b } }

fn main() i32 {
    max(P { x: 1 }, P { x: 2 }).x
}
```
