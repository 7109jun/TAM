# 2. 데이터

## 배열

```tam
// exit: 6
fn main() i32 {
    let a = [1, 2, 3]
    let z = [0; 4]
    a[0] + a[1] + a[2] + z[3]
}
```

## 2차원 배열

```tam
// exit: 6
fn main() i32 {
    mut g = [[0; 3]; 2]
    for r in 0..2 {
        for c in 0..3 { g[r][c] = r * 3 + c }
    }
    g[1][2] + 1
}
```

(`g`의 원소는 `i32`다.)

## 슬라이스로 합 구하기

```tam
// exit: 15
fn sum(a: &[i32]) i32 {
    mut s = 0
    for x in a { s += x }
    s
}

fn main() i32 {
    let a = [1, 2, 3, 4, 5]
    sum(&a)
}
```

## 부분 슬라이스

```tam
// exit: 90
fn sum(a: &[i32]) i32 {
    mut s = 0
    for i in 0..a.len { s += a[i] }
    s
}

fn main() i32 {
    let a = [10, 20, 30, 40, 50]
    sum(&a[1..4])               // 20+30+40
}
```

## 슬라이스에 쓰기

```tam
// exit: 0
fn zero(a: &mut [i32]) {
    for i in 0..a.len { a[i] = 0 }
}

fn main() i32 {
    mut a = [1, 2, 3]
    zero(&mut a[..])
    a[0] + a[1] + a[2]
}
```

## 구조체

```tam
// exit: 7
struct Point {
    x: i32,
    y: i32,
}

fn main() i32 {
    let p = Point { x: 3, y: 4 }
    p.x + p.y
}
```

## 중첩 구조체

```tam
// exit: 14
struct Point { x: i32, y: i32 }
struct Line { a: Point, b: Point }

fn main() i32 {
    let l = Line { a: Point { x: 1, y: 2 }, b: Point { x: 5, y: 6 } }
    l.a.x + l.a.y + l.b.x + l.b.y
}
```

## 메서드와 생성자

```tam
// exit: 24
struct Rect { w: i32, h: i32 }

impl Rect {
    fn new(w: i32, h: i32) Self { Rect { w: w, h: h } }
    fn area(&self) i32 { self.w * self.h }
    fn scale(&mut self, k: i32) { self.w *= k; self.h *= k }
}

fn main() i32 {
    mut r = Rect::new(1, 2)
    r.scale(2)                  // 2 x 4
    r.area() * 3                // 8 * 3
}
```

## 값으로 소비하는 메서드

```tam
// exit: 10
struct Money { cents: i64 }

impl Money {
    fn into_cents(self) i64 { self.cents }
}

fn main() i32 {
    let m = Money { cents: 10 }
    m.into_cents() as i32
}
```

## 구조체 안의 배열: 고정 용량 스택

```tam
// exit: 3
struct Stack {
    items: [i32; 4],
    len: u64,
}

impl Stack {
    fn new() Self { Stack { items: [0; 4], len: 0 } }
    fn push(&mut self, v: i32) {
        self.items[self.len] = v
        self.len += 1
    }
    fn pop(&mut self) Option<i32> {
        if self.len == 0 { return None }
        self.len -= 1
        Some(self.items[self.len])
    }
}

fn main() i32 {
    mut s = Stack::new()
    s.push(1)
    s.push(3)
    s.pop().unwrap_or(0)
}
```
