# 13. 제네릭

같은 일을 여러 타입에 하고 싶을 때가 있다. 두 값 중 큰 것을 고르는 함수를 `i32`, `i64`, `f64`마다 따로 쓰는 대신, **타입을 매개변수로** 받는 코드를 한 번만 쓴다.

TAM의 제네릭은 C++의 템플릿에서 영감을 받았다. 쓰이는 타입마다 컴파일러가 코드를 따로 만든다(**단형화**). 그래서 실행 시간 비용이 없고, 타입에 대해 선언할 것이 적다. 다만 `template <typename T>` 같은 별도 문법 없이 이름 뒤에 `<T>`를 붙이는 TAM의 문법을 쓴다.

## 13.1 제네릭 함수

```tam
// exit: 9
fn max<T>(a: T, b: T) T {
    if a > b { a } else { b }
}

fn main() i32 {
    let a = max(3, 9)                // T = i32
    let b: i64 = max(5, 7)           // T = i64
    let c = max<f64>(1.5, 0.5)       // T를 직접 적었다
    if c != 1.5 { return 100 }
    a
}
```

`fn max<T>(...)`의 `<T>`가 타입 매개변수 선언이다. `T`는 매개변수와 반환 타입, 본문 안에서 타입으로 쓴다.

### 타입 인자는 어떻게 정해지나

호출할 때 컴파일러가 `T`를 다음 순서로 정한다.

1. **직접 적은 것**: `max<f64>(..)`.
2. **인자의 타입**: `max(x, y)`에서 `x`의 타입.
3. **기대되는 반환 타입**: `let z: u8 = zero()`처럼 결과가 놓일 자리.
4. **정수 리터럴만 있으면 `i32`**: `max(3, 9)`.

```tam
// exit: 9
fn zero<T>() T { 0 }

fn first<T>(a: &[T]) T { a[0] }

fn main() i32 {
    let a: i32 = zero()           // 반환 자리에서 T = i32
    let b = zero<i64>()           // 직접 적음
    let arr = [4, 5, 6]
    let c = first(&arr)           // &[i32; 3]에서 T = i32
    a + b as i32 + c + 5
}
```

어느 것으로도 정해지지 않으면 오류다. 이때는 직접 적는다.

```tam,error
// error: cannot infer type parameter `T` of `zero`
fn zero<T>() T { 0 }

fn main() i32 {
    let x = zero()
    0
}
```

타입 매개변수는 여러 개일 수 있다: `fn pick<A, B>(a: A, b: B) A`.

### 비교 연산자와 구별

`f<i32>(x)`와 `a < b`는 모양이 비슷하다. 컴파일러는 `<`로 시작해 `>`로 닫히고 **그 뒤에 `(`, `::`, `{`가 오면** 타입 인자로 읽고, 아니면 비교 연산자로 읽는다. `if a < b { ... }`는 그대로 비교다.

## 13.2 제네릭 구조체

```tam
// exit: 7
struct Pair<A, B> {
    first: A,
    second: B,
}

impl Pair<A, B> {
    fn new(a: A, b: B) Self {
        Pair { first: a, second: b }
    }

    fn swap(self) Pair<B, A> {
        Pair { first: self.second, second: self.first }
    }

    fn get_first(&self) A { self.first }
}

fn main() i32 {
    let p = Pair::new(2, 5)             // Pair<i32, i32>
    let q: Pair<i32, i32> = p.swap()
    let r = Pair<i32, i32> { first: 4, second: 1 }
    q.first + r.get_first() - 2         // 5 + 4 - 2
}
```

- **타입으로 쓸 때는 인자를 항상 준다**: `Pair<i32, f64>`. 인자 없이 `Pair`만 쓰면 오류다.
- **구조체 리터럴**은 필드 값에서 인자를 추론한다: `Pair { first: 1, second: 2.5 }`. 직접 적을 수도 있다: `Pair<i32, f64> { ... }`.
- **연관 함수**는 `Pair::new(..)`로 추론하거나 `Pair<i32, f64>::new(..)`로 적는다.
- `impl Pair<A, B>`에서 `A`, `B`는 그 `impl` 안에서 쓰는 이름이다. 개수만 맞으면 된다.
- `impl` 안의 `Self`는 `Pair<A, B>`다.
- `Result<T, E>`, `Option<T>`도 같은 방식의 제네릭 타입이다.

```tam
// exit: 4
struct Wrapper<T> {
    v: T,
}

impl Wrapper<T> {
    fn new(v: T) Self { Wrapper { v: v } }
    fn get(&self) &T { &self.v }
}

fn nest<T>(x: T) Wrapper<Wrapper<T>> {
    Wrapper::new(Wrapper::new(x))
}

fn main() i32 {
    let w = nest(string_from("abcd"))   // Wrapper<Wrapper<String>>
    let inner = w.get()
    let s = inner.get()
    s.len as i32                        // String은 자동으로 해제된다
}
```

`String`을 담은 인스턴스도 소유권 규칙은 그대로다. 안쪽 `String`은 `w`가 스코프를 벗어날 때 해제된다.

## 13.3 단형화: 컴파일러가 하는 일

`max(3, 9)`와 `max<f64>(1.5, 0.5)`를 쓰면 컴파일러는 **함수를 두 벌** 만든다.

```
fn max<i32>(a: i32, b: i32) i32 { ... }
fn max<f64>(a: f64, b: f64) f64 { ... }
```

타입마다 따로 컴파일되므로 `a > b`가 정수 비교인지 실수 비교인지 컴파일 시간에 이미 정해져 있다. 호출 때 타입을 고르는 비용은 없다. 대가는 코드 크기다. 많은 타입으로 쓰면 코드가 그만큼 커진다.

같은 인스턴스는 한 번만 만든다(`max(1, 2)`를 백 번 불러도 `max<i32>`는 하나). 구조체도 마찬가지로 `Pair<i32,i32>`는 하나의 타입이다. 오류 메시지에도 `Pair<i32,i32>`라는 이름이 그대로 나온다.

## 13.4 제네릭은 쓸 때 검사된다

TAM에는 트레이트나 "`T`는 `>`를 지원해야 한다" 같은 선언이 **없다**. 대신 제네릭 함수의 본문은 **인스턴스가 만들어질 때** 실제 타입으로 검사된다. `a > b`를 구조체에 쓰면 그 호출이 오류를 낸다.

```tam,error
// error: operator `>` cannot be applied to `P`
struct P { x: i32 }

fn max<T>(a: T, b: T) T {
    if a > b { a } else { b }
}

fn main() i32 {
    let a = P { x: 1 }
    let b = P { x: 2 }
    max(a, b).x
}
```

```console
error: operator `>` cannot be applied to `P`
  --> ex.tam:4:8
   |
 4 |     if a > b { a } else { b }
   |        ^ unsupported operand type
...
10 |     max(a, b).x
   |     --------- while instantiating `max<P>`
```

오류가 **두 곳**을 가리킨다. 위쪽은 문제가 난 코드(제네릭 본문), 아래쪽(`while instantiating`)은 그 인스턴스를 만든 호출이다. 보통은 호출하는 타입이 틀렸거나, 제네릭 함수가 가정하는 연산이 그 타입에 없다는 뜻이다.

주의할 점은, **한 번도 쓰이지 않은 제네릭은 검사되지 않는다**는 것이다(문법만 검사된다). 제네릭 코드를 쓸 예정이라면 작은 호출 한두 개로 확인해 두자.

## 13.5 예: 직접 만드는 `Stack<T>`

제네릭과 [10장](10-unsafe-and-systems.md)의 `unsafe`를 합치면 자료구조를 만들 수 있다. 경계 검사를 직접 넣어 `unsafe`를 안쪽에 가둔다.

```tam
// exit: 37
struct Stack<T> {
    ptr: *mut T,
    len: u64,
    cap: u64,
}

impl Stack<T> {
    fn new(cap: u64) Self {
        let bytes = cap * 8                    // 원소 하나는 최대 8바이트로 가정
        Stack { ptr: unsafe { alloc(bytes) } as *mut T, len: 0, cap: cap }
    }

    fn push(&mut self, v: T) {
        assert(self.len < self.cap)            // 용량 검사는 우리 책임
        unsafe { *(self.ptr + self.len) = v }
        self.len += 1
    }

    fn pop(&mut self) Option<T> {
        if self.len == 0 { return None }
        self.len -= 1
        Some(unsafe { *(self.ptr + self.len) })
    }

    fn release(self) {
        unsafe { free(self.ptr as *mut u8) }
    }
}

fn main() i32 {
    mut a: Stack<i32> = Stack::new(4)
    a.push(10)
    a.push(20)
    a.push(30)
    let top = a.pop().unwrap_or(0)             // 30
    let second = a.pop().unwrap_or(0)          // 20
    a.release()

    mut b: Stack<i64> = Stack::new(4)          // 같은 코드가 i64로 다시 만들어진다
    b.push(7)
    let c = b.pop().unwrap_or(0) as i32
    b.release()
    top + second - 30 + c + 10                 // 30 + 20 - 30 + 7 + 10
}
```

`Stack<i32>`와 `Stack<i64>`는 **서로 다른 타입**이다. `a`에 `i64`를 넣으려 하면 컴파일 오류다.

## 13.6 다른 모듈의 제네릭

제네릭 함수와 구조체도 `pub`을 붙이면 다른 모듈(패키지)에서 쓴다. 구조체는 `모듈::Stack<i32>`, 연관 함수는 `모듈::Stack<i32>::new(4)`다.

<!-- project run exit=7 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
util = { path = "../util" }
```

```tam,file=app/src/main.tam
use util

fn main() i32 {
    let p = util::Pair<i32, i32> { first: 3, second: 4 }
    util::sum(&p)
}
```

```toml,file=util/Tmake.toml
[package]
name = "util"
version = "0.1.0"
kind = "lib"
```

```tam,file=util/src/lib.tam
pub struct Pair<A, B> {
    first: A,
    second: B,
}

pub fn sum<T>(p: &Pair<T, T>) T {
    p.first + p.second
}
```
<!-- /project -->

## 13.7 한계

TAM의 제네릭은 의도적으로 작다.

- **트레이트(경계)가 없다.** 타입에 대한 요구는 인스턴스 오류로만 알려진다.
- **메서드가 자기만의 타입 매개변수를 가질 수 없다.** `impl Pair<A, B>`의 `A`, `B`만 쓴다. 메서드에 별도의 타입이 필요하면 일반 제네릭 함수로 쓴다.
- **상수 제네릭**(`[T; N]`의 `N`을 매개변수로)은 없다.
- 제네릭 `main`과 `extern`은 불가능하다.
- 자기 자신을 점점 큰 타입으로 인스턴스화하는 재귀는 `generic instantiation limit exceeded`로 멈춘다(깊이 64, 인스턴스 5000개).
- 힙을 가진 필드를 `self.field`로 꺼내 이동할 수 없다는 규칙은 제네릭에도 같다. `T`가 `String`일 때 `fn into_inner(self) T { self.v }`는 그 인스턴스에서 오류다. 참조를 돌려주는 `get(&self) &T`를 쓴다.

### C++ 템플릿과 비교

| | C++ 템플릿 | TAM 제네릭 |
|---|---|---|
| 구현 | 단형화 | 단형화 |
| 검사 시점 | 인스턴스화 때 | 인스턴스화 때 |
| 선언 | `template <typename T>` | `fn f<T>`, `struct S<T>` |
| 제약 | concepts(C++20) | 없음(인스턴스 오류) |
| 오류 메시지 | 길다 | 제네릭 본문 + 인스턴스를 만든 호출 두 곳 |
| 특수화 | 있다 | 없다 |
| 인스턴스 한도 | 컴파일러마다 다름 | 깊이 64, 5000개 |

## 13.8 연습

1. `swap<T>(a: &mut T, b: &mut T)`를 만들어라. 힌트: 값을 꺼낼 때 `let t = *a`.
2. `Cell<T>`에 `set(&mut self, v: T)`와 `get(&self) T`를 달아라.
3. `Stack<T>`에 `len(&self) u64`를 추가하라.

```tam
// exit: 21
fn swap<T>(a: &mut T, b: &mut T) {
    let t = *a
    *a = *b
    *b = t
}

struct Cell<T> { v: T }

impl Cell<T> {
    fn new(v: T) Self { Cell { v: v } }
    fn get(&self) T { self.v }
    fn set(&mut self, v: T) { self.v = v }
}

fn main() i32 {
    mut x = 1
    mut y = 2
    swap(&mut x, &mut y)                 // x = 2, y = 1
    mut c = Cell::new(x)
    let v = c.get() * 10
    c.set(v)                             // 20
    c.get() + y                          // 21
}
```

→ [14. 더 나아가기](14-practice.md)
