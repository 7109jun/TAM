# Style Guide와 관용구

TAM 코드를 읽기 쉽고 안전하게 쓰는 관례를 모았다. **컴파일러가 강제하는 규칙이 아니다.** (공식 포매터는 아직 없다.) 규칙의 정의는 [The TAM Reference](reference.md)에 있다.

- 대상 버전: `tamc 1.0`
- 이 문서의 모든 `tam` 코드는 `tamc 1.0`으로 컴파일하고 실행해 검증했다.

## 1. 모양

- **들여쓰기는 공백 4칸.** 탭을 쓰지 않는다.
- **한 줄에 한 문장.** `;`는 짧은 연관 문장을 한 줄에 쓸 때만 쓴다(`let a = 1; let b = 2`).
- **중괄호는 같은 줄에서 연다.** `if`, `while`, `for`, `loop`, `match`, 함수 모두 그렇다.
- **줄이 길면 연산자나 `,`로 끝내서 이어 쓴다.** 연산자로 시작하는 줄은 이어지지 않는다([Book 3.3](book/03-functions-and-control-flow.md#33-줄바꿈과-문장)).
- **한 줄은 100자 이내**를 권한다.
- 구조체의 필드는 한 줄에 하나씩, 마지막에도 쉼표를 쓴다.

```tam
// exit: 7
struct Config {
    width: i32,
    height: i32,
}

fn area(c: &Config) i32 {
    c.width * c.height
}

fn main() i32 {
    let c = Config { width: 2, height: 3 }
    let total = area(&c) +
        1
    total
}
```

## 2. 이름

| 대상 | 형식 | 예 |
|---|---|---|
| 함수, 메서드, 변수, 모듈 | `snake_case` | `read_file`, `line_count` |
| 구조체 | `PascalCase` | `Config`, `Pair` |
| 타입 매개변수 | 짧은 대문자 | `T`, `A`, `B` |
| 상수 | `UPPER_SNAKE_CASE` | `MAX_LEN` |
| 패키지 이름(`Tmake.toml`) | 소문자와 `-` | `text-tools` |

- 이름은 **무엇인지**를 말한다. `n`, `i`, `p`는 아주 좁은 범위에서만 쓴다.
- 참/거짓을 돌려주는 함수는 `is_`, `has_`로 시작한다: `is_empty`, `has_prefix`.
- 값을 만드는 연관 함수는 `new`, 변환은 `to_`/`from_`을 쓴다.

## 3. 상수

의미 있는 숫자에는 이름을 붙인다. 배열 길이에도 쓴다.

```tam
// exit: 12
const ROWS: i32 = 3
const COLS: i32 = 4
const CELLS: i32 = ROWS * COLS

fn main() i32 {
    let grid = [0; CELLS]       // 길이 12
    grid.len as i32
}
```

## 4. 함수 설계

- **작게 쓴다.** 한 함수는 한 가지 일을 한다.
- **시그니처는 계약이다.** 입력은 가능하면 **빌려서**(`&T`) 받고, 바꿀 때만 `&mut T`, 소비할 때만 값(`self`, `T`)으로 받는다.
- **실패할 수 있으면 `Result`/`Option`을 돌려준다.** `panic`은 "호출 규칙을 어긴 버그"에만 쓴다.

```tam
// exit: 6
struct Counter { n: i32 }

impl Counter {
    fn new() Self { Counter { n: 0 } }
    fn get(&self) i32 { self.n }          // 읽기만: &self
    fn bump(&mut self) { self.n += 1 }    // 바꿈: &mut self
    fn finish(self) i32 { self.n * 2 }    // 소비: self
}

fn main() i32 {
    mut c = Counter::new()
    c.bump()
    c.bump()
    c.get() + c.finish() - 0
}
```

## 5. 오류 처리

| 상황 | 쓰는 것 |
|---|---|
| 입력이나 환경 때문에 실패할 수 있다 | `Result<T, E>` |
| 값이 없는 것이 정상이다 | `Option<T>` |
| 호출 규칙을 어겼다(버그) | `assert`, `panic` |

- 호출 쪽에서 오류를 그대로 전하면 `?`를 쓴다.
- `unwrap`은 "여기서 실패하면 버그"일 때만 쓴다. 입력이 들어오는 곳에서는 `match`로 처리한다.
- 오류 타입은 **하나로 통일**한다(`?`는 `From` 변환이 없다). 오류 코드(`i32`)나 구조체를 쓴다.

```tam
// exit: 14
fn parse_digit(c: u8) Result<i32, i32> {
    if c < 48 || c > 57 { return Err(1) }        // 1: 숫자가 아니다
    Ok((c - 48) as i32)
}

fn parse_two(a: u8, b: u8) Result<i32, i32> {
    let x = parse_digit(a)?
    let y = parse_digit(b)?
    Ok(x * 10 + y)
}

fn main() i32 {
    match parse_two(49, 52) {          // "14"
        Ok(v) => v
        Err(e) => 100 + e
    }
}
```

## 6. 소유권과 빌림

- **빌림을 먼저 생각한다.** `String`을 함수에 넘겨 소유권을 잃기 전에 `&String`이 되는지 본다.
- 참조는 **짧게** 산다. 마지막 사용 뒤에는 다시 쓸 수 있다(NLL).
- 한 값을 여러 곳에서 바꾸려 하지 않는다. 필요하면 구조를 나눈다(서로 다른 필드는 동시에 빌릴 수 있다).
- 이동이 의도라면 이름으로 드러낸다(`finish`, `into_`, `release` 같은 이름).

```tam
// exit: 5
fn length(s: &String) i32 { s.len as i32 }     // 빌려서 읽기만

fn main() i32 {
    let s = string_from("hello")
    let a = length(&s)
    let b = length(&s)                         // s는 여전히 쓸 수 있다
    a + b - 5
}
```

## 7. 반복과 분기

- 범위나 슬라이스를 훑으면 **`for`**를 쓴다. 조건으로 도는 것은 `while`, 끝이 안에서 정해지면 `loop`.
- `break`/`continue`는 **일찍 빠질 때**만. 깊이 중첩되면 함수로 나눈다(레이블이 없다).
- `if`는 값이 있는 식이다. 임시 변수 대신 값으로 쓴다.
- `match`는 팔을 정확히 한 번씩 쓴다. 값을 쓰지 않는 팔은 `_`로 받는다(`Ok(_)`).

```tam
// exit: 10
fn clamp(x: i32, lo: i32, hi: i32) i32 {
    if x < lo { lo } else if x > hi { hi } else { x }
}

fn main() i32 {
    mut sum = 0
    for i in 0..5 { sum += clamp(i, 1, 3) }      // 1 + 1 + 2 + 3 + 3
    sum
}
```

## 8. 제네릭

- **단순하게 시작한다.** 두 타입에 같은 코드를 쓰게 될 때 제네릭으로 바꾼다.
- 트레이트 경계가 없으므로 **문서 주석에 요구하는 연산을 적는다**(`T`는 `>`를 지원해야 한다).
- 만든 제네릭은 **작은 호출로 바로 시험한다.** 쓰이지 않은 제네릭은 검사되지 않는다.
- 타입 인자는 대개 추론에 맡기고, 반환 자리로만 정해질 때(`zero<i32>()`)만 적는다.

```tam
// exit: 9
// T는 `>`를 지원해야 한다 (정수, f64)
fn max<T>(a: T, b: T) T {
    if a > b { a } else { b }
}

fn main() i32 {
    max(3, 9)
}
```

## 9. `unsafe`

1. **작게 가둔다.** `unsafe { ... }` 블록은 가능한 한 한 줄, 길어도 몇 줄이다.
2. **안전한 인터페이스로 감싼다.** 경계 검사와 소유권을 바깥 API가 책임진다.
3. **블록마다 왜 안전한지를 주석으로 적는다.**
4. `alloc`한 것은 짝이 되는 `free`를 어디서 부르는지 정한다.

```tam
// exit: 11
struct Words {
    ptr: *mut i32,
    len: u64,
}

impl Words {
    fn new(len: u64) Self {
        // alloc은 len * 4바이트를 돌려준다. release에서 free한다.
        Words { ptr: unsafe { alloc(len * 4) } as *mut i32, len: len }
    }

    fn set(&mut self, i: u64, v: i32) {
        assert(i < self.len)                  // 경계 검사가 안전을 만든다
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
    mut w = Words::new(2)
    w.set(0, 4)
    w.set(1, 7)
    let r = w.get(0) + w.get(1)
    w.release()
    r
}
```

## 10. 주석

- **왜**를 적는다. 코드가 **무엇**을 하는지는 코드가 말한다.
- `//`는 줄 주석, `/* */`는 블록 주석(중첩된다). 코드를 잠시 끌 때 블록 주석이 편하다.
- 공개 함수에는 한 줄이라도 입력의 조건과 실패 방식을 적는다.

## 11. 프로젝트와 파일

- 패키지: **`src/main.tam`**(실행 파일), **`src/lib.tam`**(라이브러리). 모듈은 `src/geo/shapes.tam`처럼 `use geo.shapes`와 경로를 맞춘다.
- **밖에 보일 것만 `pub`**. 기본은 비공개다.
- 파일 하나는 모듈 하나, 한 주제만 담는다.
- `Tmake.toml`의 `tamc = ">=1.0"`과 `edition = "1"`을 적어 둔다.

## 12. 피할 것

| 피한다 | 대신 |
|---|---|
| 의미 없는 숫자(`if x > 86400`) | `const SECONDS_PER_DAY: i32 = 86400` |
| `unwrap()` 남발 | `match`, `?`, `unwrap_or` |
| 큰 `unsafe` 블록 | 작은 `unsafe` + 안전한 인터페이스 |
| `String`을 값으로 받기만 하는 함수 | `&String` 또는 `str` |
| 깊이 4 이상의 중첩 | 함수로 나누기, 일찍 `return` |
| 단위 없는 정수 | 이름에 단위(`timeout_ms`) 또는 `const` |
