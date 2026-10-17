# Safety and Memory Model

TAM이 **무엇을 보장하고 무엇을 보장하지 않는지**를 정확히 적은 문서다. 기능의 정의는 [The TAM Reference](reference.md)의 8~12장이고, 이 문서는 그것을 안전성의 관점에서 다시 묶는다.

- 대상 버전: `tamc 1.0`
- 이 문서의 모든 코드는 `tamc 1.0`으로 컴파일하고 실행해(오류와 trap은 메시지까지) 검증했다.

## 1. 약속

> **`unsafe`가 없는 TAM 코드는, 컴파일러가 올바르다면, 메모리를 깨뜨리지 못한다.**
> 위반은 컴파일 오류이거나, 조용한 오동작이 아니라 **즉시 멈춤(trap)** 이다.

세 층으로 나눠 생각한다.

| 층 | 누가 책임지나 | 위반하면 |
|---|---|---|
| 안전한 코드 | 컴파일러 | 컴파일 오류 또는 trap |
| `unsafe` 블록, `unsafe fn`, `extern` 호출 | **사용자** | 정의되지 않은 동작 |
| 컴파일러 자신 | tamc 개발 | 버그(발견하면 고친다) |

## 2. 컴파일 시간에 막는 것

### 2.1 소유권: 해제 후 사용과 이중 해제

모든 힙 값(`String`, 이를 가진 구조체 등)에는 소유자가 하나다. 이동한 뒤에는 쓸 수 없고, 소유자가 스코프를 벗어나면 **한 번** 해제된다.

```tam,error
// error: use of moved value `s`
fn eat(s: String) i32 { s.len as i32 }

fn main() i32 {
    let s = string_from("hello")
    let n = eat(s)
    n + s.len as i32
}
```

분기마다 이동 상태를 합치므로 "어떤 경로에서만 이동"도 잡는다.

```tam,error
// error: use of moved value `s`
fn eat(s: String) i32 { s.len as i32 }

fn main() i32 {
    let s = string_from("hello")
    let c = 1
    if c == 1 { eat(s) } else { 0 }
    s.len as i32
}
```

### 2.2 빌림: 댕글링과 별칭 충돌

참조는 소유자를 **빌린다**. 읽기 참조는 여러 개, 쓰기 참조는 하나만 동시에 살아 있을 수 있고, 참조는 소유자보다 오래 살 수 없다.

```tam,error
// error: cannot return reference to local variable `x`
fn f() &i32 {
    let x = 5
    &x
}

fn main() i32 { *f() }
```

```tam,error
// error: cannot assign to `v` because it is borrowed
fn main() i32 {
    mut v = 1
    let r = &v
    v = 2
    *r
}
```

lifetime을 **쓰지 않는다**. 컴파일러가 본문에서 추론하고, 여러 인자에서 올 수 있으면 보수적으로 모두를 빌린 것으로 본다. 그래서 가끔 올바른 코드가 거부될 수 있다(안전한 쪽으로의 오류).

### 2.3 타입, 초기화, 변경 가능성

- 초기화되지 않은 변수는 없다(`let x`만 쓰는 문법이 없다).
- 불변 변수에는 대입할 수 없다. `&mut`는 `mut` 변수에서만 만든다.
- 정수 사이의 암묵적 변환이 없다. `as`만 쓴다(그리고 `as`는 trap하지 않고 감싸거나 포화한다).
- `unsafe` 없이는 raw 포인터를 역참조하거나 `alloc`/`free`를 부를 수 없다.

```tam,error
// error: needs an `unsafe` block
fn main() i32 {
    let m = alloc(8)
    0
}
```

## 3. 실행 시간에 막는 것(trap)

컴파일 시간에 알 수 없는 위반은 검사 코드를 넣어 **즉시 멈춘다**. 호스트에서는 `tam: panic: <메시지>`를 stderr에 쓰고 `SIGILL`(종료 코드 132)로 끝난다.

| 위반 | 메시지 |
|---|---|
| 배열·슬라이스 경계 밖 접근 | `index out of bounds` |
| 정수 `+ - *`, 단항 `-` 오버플로 | `integer overflow` |
| `/`, `%`의 0 나누기 | `division by zero` |
| `MIN / -1` | `integer overflow in division` |
| `assert(거짓)` | `assertion failed` |
| `None`/`Err`에 `unwrap` | ``called `unwrap()` on a `None` value`` 등 |
| `panic("...")` | 그 메시지 |

```tam,trap
// panic: index out of bounds
fn get(a: &[i32], i: u64) i32 { a[i] }

fn main() i32 {
    let a = [1, 2, 3]
    get(&a, 3)
}
```

```tam,trap
// panic: integer overflow
fn main() i32 {
    let a: i32 = 2147483647
    a + 1
}
```

```tam,trap
// panic: division by zero
fn main() i32 {
    let z = 0
    10 / z
}
```

**검사는 빌드 모드와 무관하다.** `--release`로도 끌 수 없다. 오버플로가 의도라면 `wrapping_add`, `wrapping_sub`, `wrapping_mul`로 명시한다.

```tam
// exit: 0
fn main() i32 {
    let a: i32 = 2147483647
    if wrapping_add(a, 1) != -2147483647 - 1 { return 1 }
    0
}
```

## 4. 자동 해제의 정확한 의미

- 스코프가 끝나면 **선언의 역순**으로 해제한다.
- `return`, `?`, `break`, `continue`로 나갈 때도 살아 있는 값을 모두 해제한다.
- 이동 여부는 변수마다 둔 런타임 플래그로 판단해, **조건부 이동**도 한 번만 해제된다.
- 재대입은 이전 값을 먼저 해제한다.

```tam
// exit: 3
struct User { name: String, age: i32 }

fn consume(u: User) i32 { u.age + u.name.len as i32 }

fn main() i32 {
    let u = User { name: string_from("ab"), age: 1 }
    consume(u)        // u는 여기서 이동했다. main이 끝날 때 다시 해제되지 않는다
}
```

### 해제되지 **않는** 경우

- `os::exit`는 지역 변수를 해제하지 않고 즉시 끝낸다.
- trap과 패닉은 해제를 실행하지 않는다. 프로세스가 끝나므로 운영체제가 회수한다.
- 순환이나 `unsafe`로 직접 만든 구조는 컴파일러가 모른다. `alloc`한 것은 `free`로 직접 돌려준다.

사용자 정의 소멸자(`Drop`)는 없다.

## 5. `unsafe`: 사용자의 책임

`unsafe`가 필요한 것은 raw 포인터 역참조와 산술, `unsafe fn`과 `extern` 호출, `alloc`/`free`와 하드웨어 내장 함수(`mmio_*` 등)의 호출이다.

`unsafe` 안에서 컴파일러가 **검사하지 않는** 것:

- 포인터가 가리키는 곳이 유효한가(범위, 정렬, 수명)
- 같은 메모리에 대한 별칭과 동시 접근
- `free`를 두 번 부르는가, `free` 뒤에 쓰는가
- 호출한 C 함수의 시그니처가 맞는가

이를 어기면 **정의되지 않은 동작**이다. 그러나 `unsafe` 바깥의 안전한 코드는 계속 컴파일러가 지킨다. 그래서 권장하는 방식은 `unsafe`를 작은 함수 안에 가두고, 그 함수가 **안전한 인터페이스**(경계 검사, 소유권)를 제공하게 하는 것이다.

```tam
// exit: 30
struct Buf {
    ptr: *mut i32,
    len: u64,
}

impl Buf {
    fn new(len: u64) Self {
        Buf { ptr: unsafe { alloc(len * 4) } as *mut i32, len: len }
    }

    // 안전한 인터페이스: 경계를 검사한다
    fn set(&mut self, i: u64, v: i32) {
        assert(i < self.len)
        unsafe { *(self.ptr + i) = v }
    }

    fn get(&self, i: u64) i32 {
        assert(i < self.len)
        unsafe { *(self.ptr + i) }
    }

    fn release(self) {
        unsafe { free(self.ptr as *mut u8) }
    }
}

fn main() i32 {
    mut b = Buf::new(4)
    b.set(0, 10)
    b.set(3, 20)
    let r = b.get(0) + b.get(3)
    b.release()
    r
}
```

## 6. 안전한 코드에서도 일어날 수 있는 것

"안전"은 "아무 일도 안 일어난다"가 아니다. 다음은 **정의되지 않은 동작이 아니지만 프로그램이 멈추거나 실패할 수 있는** 경우다. 솔직히 적어 둔다.

| 상황 | 결과 |
|---|---|
| 깊은 재귀로 스택 초과 | 호스트에서 운영체제가 `SIGSEGV`(종료 코드 139)로 끝낸다. 컴파일러는 검사하지 않는다 |
| 힙 할당 실패(`malloc`이 NULL) | **검사하지 않는다.** 메모리가 바닥나면 이후 접근에서 비정상 종료할 수 있다 |
| BIOS/AArch64의 힙 소진 | trap(각 32MiB) |
| 펌웨어 대상에서 예외 | 예외 핸들러가 없어 리셋하거나 멈춘다 |
| trap이 일어난 프로그램 | 해제가 실행되지 않는다 |

스택 초과를 직접 확인한 예다(검증 가능한 종료 코드는 운영체제에 따라 다르므로 코드로는 싣지 않는다).

```console
$ cat so.tam
fn f(n: i64) i64 { f(n + 1) + 1 }
fn main() i32 { f(0) as i32 }
$ tamc run so.tam
error: the program was terminated by signal 11
```

## 7. 컴파일러가 보수적인 곳

안전을 위해 올바른 코드를 거절하는 경우가 있다. 거절되면 코드를 조금 고치면 된다.

- 배열 원소는 서로 구분하지 않는다. `&mut a[0]`과 `&a[1]`은 충돌한다.
- 참조 변수에 다시 대입해도 이전 빌림이 풀리지 않는다.
- 힙 필드를 `self.field`로 꺼내 이동할 수 없다.
- `break`/`continue` 이후 경로는 모두 도달 가능하다고 본다.

반대 방향의 오류(안전하지 않은데 통과)는 버그다. 발견하면 최소 재현 코드와 함께 보고해 달라.

## 8. 어떻게 확인하는가

이 약속은 말이 아니라 검증으로 뒷받침한다(개발 저장소 기준).

- **회귀 테스트**: 소유권·빌림·trap·수치 경계마다 통과해야 하는 프로그램과 거절되어야 하는 프로그램을 모두 시험한다.
- **차분 퍼저**: 임의로 만든 정수 식, 변수·배열·반복·함수가 섞인 프로그램, 힙/소유권이 섞인 프로그램, 제네릭 프로그램을 만들어 파이썬 참조 구현과 종료 코드를 비교한다.
- **valgrind**: 힙 퍼저의 프로그램을 valgrind 아래에서 실행해 누수, 이중 해제, 잘못된 접근이 0임을 본다.
- **크래시 퍼저**: 깨진 입력을 컴파일러에 넣어 컴파일러가 패닉하거나 스택이 넘치지 않고 오류로 끝나는지 본다.
- **문서의 모든 코드**: 컴파일하고 실행해 출력, 종료 코드, 오류 메시지가 문서와 같은지 확인한다.

퍼저가 찾은 대표적인 버그는 [Release Notes](changelog.md)에 있다.

## 9. 요약

- 안전한 코드: 해제 후 사용, 이중 해제, 누수, 댕글링, 경계 밖, 오버플로를 컴파일 오류 또는 trap으로 막는다.
- `unsafe`: 책임은 사용자에게 있다. 작게 가두고 안전한 인터페이스로 감싼다.
- 한계: 스택 초과와 할당 실패는 검사하지 않는다.
