# 6. 구조체와 메서드

## 6.1 구조체

관련된 값을 묶는다. 필드는 쉼표나 줄바꿈으로 구분한다.

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

- 리터럴은 **모든 필드를 한 번씩**, 순서 없이 쓴다.
- 필드는 항상 공개다(필드 단위 `pub`은 없다).
- 구조체는 다른 구조체, 배열, `String`, 참조, 포인터, `Result`/`Option`을 필드로 가질 수 있다. 자기 자신을 값으로 포함할 수는 없다.

```tam
// exit: 6
struct Point { x: i32, y: i32 }

struct Rect {
    min: Point
    max: Point
}

fn area(r: &Rect) i32 {
    (r.max.x - r.min.x) * (r.max.y - r.min.y)
}

fn main() i32 {
    let r = Rect { min: Point { x: 0, y: 0 }, max: Point { x: 3, y: 2 } }
    area(&r)
}
```

### 가변 필드

필드를 바꾸려면 구조체 변수가 `mut`여야 한다.

```tam
// exit: 9
struct Counter { n: i32 }

fn main() i32 {
    mut c = Counter { n: 4 }
    c.n += 5
    c.n
}
```

```tam,error
// error: cannot assign to a field of immutable variable `c`
struct Counter { n: i32 }

fn main() i32 {
    let c = Counter { n: 4 }
    c.n = 5
    c.n
}
```

### 이동 타입

구조체는 **이동 타입**이다. 복사하려면 필드를 읽어 새로 만든다.

```tam
// exit: 6
struct P { x: i32, y: i32 }

fn copy(p: &P) P {
    P { x: p.x, y: p.y }
}

fn main() i32 {
    let a = P { x: 1, y: 2 }
    let b = copy(&a)         // a는 그대로 쓸 수 있다
    a.x + a.y + b.x + b.y
}
```

### 조건 안의 구조체 리터럴

`if`, `while`, `match`의 조건 자리에서는 `Name {`를 리터럴로 읽지 않는다(블록과 구분하기 위해). 필요하면 괄호로 감싼다.

```tam
// exit: 1
struct P { x: i32 }

fn main() i32 {
    let p = P { x: 2 }
    if (P { x: 2 }).x == p.x { 1 } else { 0 }
}
```

## 6.2 `impl`: 메서드와 연관 함수

`impl 타입 { ... }` 안에 함수를 둔다. 첫 매개변수가 `&self`, `&mut self`, `self`이면 **메서드**이고, 아니면 **연관 함수**(생성자 등)다. 타입 자신은 `Self`로 쓴다.

```tam
// exit: 25
struct Rect {
    w: i32,
    h: i32,
}

impl Rect {
    fn new(w: i32, h: i32) Self {        // 연관 함수
        Rect { w: w, h: h }
    }

    fn area(&self) i32 {                 // 읽기 메서드
        self.w * self.h
    }

    fn grow(&mut self, d: i32) {         // 수정 메서드
        self.w += d
        self.h += d
    }

    fn into_perimeter(self) i32 {        // 값을 소비하는 메서드
        2 * (self.w + self.h)
    }
}

fn main() i32 {
    mut r = Rect::new(2, 3)
    r.grow(1)                            // 3 x 4
    let a = r.area()                     // 12
    a + r.into_perimeter() - 1           // 12 + 14 - 1
}
```

호출 규칙:

| 형태 | 호출 | 하는 일 |
|---|---|---|
| `&self` | `r.area()` | 자동으로 `&r`을 빌린다 |
| `&mut self` | `r.grow(1)` | 자동으로 `&mut r`을 빌린다(`r`이 `mut`여야 함) |
| `self` | `r.into_perimeter()` | `r`를 **이동**한다. 이후 `r`는 쓸 수 없다 |
| 연관 함수 | `Rect::new(2, 3)` | 값 없이 타입으로 부른다 |

메서드는 `Rect::area(&r)`처럼 함수 형태로도 부를 수 있다.

```tam
// exit: 8
struct A { x: i32 }
impl A {
    fn get(&self) i32 { self.x }
}

fn main() i32 {
    let a = A { x: 4 }
    a.get() + A::get(&a)
}
```

### `self`(값) 메서드는 이동한다

```tam,error
// error: use of moved value `b`
struct Buf { data: String }
impl Buf {
    fn into_len(self) i32 { self.data.len as i32 }
}

fn main() i32 {
    let b = Buf { data: string_from("x") }
    let a = b.into_len()
    a + b.into_len()
}
```

### 빌림 검사는 메서드에도 그대로

`&self` 메서드가 돌려준 참조가 살아 있는 동안 `&mut self` 메서드를 부를 수 없다.

```tam,error
// error: cannot borrow `b` as mutable because it is also borrowed as immutable
struct Buf { data: String }
impl Buf {
    fn view(&self) str { as_str(&self.data) }
    fn clear(&mut self) { self.data = string_from("") }
}

fn main() i32 {
    mut b = Buf { data: string_from("xyz") }
    let v = b.view()
    b.clear()
    v.len as i32
}
```

### `impl`의 제약

- `impl`은 **같은 파일에 있는 구조체**에만 붙인다. 제네릭은 [13장](13-generics.md)에서, 트레이트는 아직 없다.
- 임시값(`make().get()`)에 `&self` 메서드를 바로 부를 수 없다. 변수에 먼저 담는다.
- `pub`은 `impl`이 아니라 **함수마다** 붙인다.

## 6.3 힙을 가진 구조체의 메서드

```tam
// exit: 50
struct Buf {
    data: String,
    n: i32,
}

impl Buf {
    fn new(s: str) Self { Buf { data: string_from(s), n: 0 } }
    fn add(&mut self, c: u8) { string_push(&mut self.data, c); self.n += 1 }
    fn size(&self) i32 { self.data.len as i32 + self.n }
    fn into_len(self) i32 { self.data.len as i32 }
    fn view(&self) str { as_str(&self.data) }
}

fn main() i32 {
    mut b = Buf::new("ab")
    b.add(99)
    b.add(100)
    let k = b.view().len as i32        // 4
    let s = b.size()                   // 4 + 2
    k * 10 + s + b.into_len()          // 40 + 6 + 4
}
```

`Buf`가 스코프를 벗어나거나 `into_len`에서 소비될 때 안의 `String`도 자동으로 해제된다.

## 6.4 메서드 체인

줄을 `.`으로 시작하면 앞 줄에 이어진다.

```tam
// exit: 9
struct Acc { n: i32 }
impl Acc {
    fn add(&mut self, k: i32) { self.n += k }
    fn get(&self) i32 { self.n }
}

struct Wrap { a: Acc }

fn main() i32 {
    mut w = Wrap { a: Acc { n: 1 } }
    w.a.add(3)
    w.a.add(5)
    w.a.get()
}
```

## 6.5 연습

`Stack`을 만들어 보자. 고정 용량 배열과 길이를 가진다.

```tam
// exit: 6
struct Stack {
    items: [i32; 8],
    len: u64,
}

impl Stack {
    fn new() Self { Stack { items: [0; 8], len: 0 } }

    fn push(&mut self, v: i32) {
        self.items[self.len] = v        // 경계를 넘으면 trap
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
    s.push(2)
    s.push(3)
    let a = s.pop().unwrap_or(0)      // 3
    let b = s.pop().unwrap_or(0)      // 2
    a + b + s.pop().unwrap_or(0)      // 3 + 2 + 1
}
```

(`Option`은 [8장](08-error-handling.md)에서 자세히 본다.)

→ [7. 배열과 슬라이스](07-arrays-and-slices.md)
