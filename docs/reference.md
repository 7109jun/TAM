# The TAM Reference

이 문서는 TAM 언어의 **규범적 정의**다. 문법, 타입, 평가 규칙, 소유권과 빌림 규칙, 오류와 trap의 동작을 빠짐없이 적는다. 언어를 처음 배운다면 [The TAM Book](book/README.md)을, 예제로 익히려면 [TAM by Example](by-example/README.md)을 먼저 보라.

- 대상 버전: `tamc 1.0`
- 이 문서의 모든 `tam` 코드 블록은 `tools/doccheck.py`가 실제로 컴파일하고 실행해 검증한다(출력과 종료 코드 포함). 블록 아래의 `// exit:`, `// out:`, `// error:` 주석이 그 기대값이다.
- "구현이 보장한다"고 적은 동작은 테스트로 고정되어 있다. "정해지지 않았다"고 적은 것은 바뀔 수 있으니 기대지 말 것.

## 목차

1. [소스 파일과 어휘 구조](#1-소스-파일과-어휘-구조)
2. [항목(Items)](#2-항목items)
3. [타입](#3-타입)
4. [표현식](#4-표현식)
5. [문장](#5-문장)
6. [`match`와 패턴](#6-match와-패턴)
7. [이름 해석과 모듈](#7-이름-해석과-모듈)
8. [소유권, 이동, 자동 해제](#8-소유권-이동-자동-해제)
9. [빌림 규칙](#9-빌림-규칙)
10. [수치 의미: 오버플로, 변환, 시프트](#10-수치-의미-오버플로-변환-시프트)
11. [`unsafe`와 raw 포인터](#11-unsafe와-raw-포인터)
12. [프로그램 실행: 진입점, 종료, 패닉, trap](#12-프로그램-실행-진입점-종료-패닉-trap)
13. [컴파일 대상](#13-컴파일-대상)
14. [내장 함수](#14-내장-함수)
15. [구현 한계](#15-구현-한계)
16. [문법 요약(EBNF)](#16-문법-요약ebnf)
17. [아직 없는 것](#17-아직-없는-것)
18. [호환성 약속: TAM 1](#18-호환성-약속-tam-1)

---

## 1. 소스 파일과 어휘 구조

### 1.1 소스 파일

- 소스 파일의 확장자는 `.tam`, 인코딩은 **UTF-8**이다. 유효하지 않은 UTF-8은 `cannot read ...: stream did not contain valid UTF-8` 오류다.
- 파일 맨 앞의 UTF-8 BOM(`U+FEFF`)은 무시한다.
- 줄 끝은 `\n` 또는 `\r\n`이다.
- 파일 하나가 모듈 하나다([7장](#7-이름-해석과-모듈)).

### 1.2 주석

주석은 두 가지다.

- `//`에서 줄 끝까지.
- `/* ... */` 블록 주석. **중첩할 수 있다**(`/* a /* b */ c */`는 하나의 주석). 닫히지 않으면 `unterminated block comment` 오류다.

블록 주석은 공백으로 취급한다. 단, **줄이 바뀌는 블록 주석은 문장을 끝내는 줄바꿈 하나**로 취급한다(문장은 줄바꿈으로 끝나기 때문이다, [1.7절](#17-줄바꿈과-문장의-끝)).

```tam
// 이것은 주석이다.
/* 이것도 /* 중첩된 */ 주석이다. */
fn main() i32 {
    let x = 1 // 줄 끝 주석
    let y = 2 /* 문장 안의 주석 */ + 3
    /* 여러 줄에
       걸친 주석 */
    x + y - 6
}
```

### 1.3 식별자

식별자는 ASCII 영문자 또는 `_`로 시작하고 ASCII 영문자, 숫자, `_`가 이어진다. 대소문자를 구분한다. 관례는 함수·변수 `snake_case`, 구조체 `PascalCase`다(컴파일러가 강제하지는 않는다).

### 1.4 키워드

```
as break const continue else extern false fn for if impl in let
loop match mut pub return struct true unsafe use while
```

`const`는 전역 상수 선언([2.6절](#26-const-전역-상수))과 포인터 타입(`*const T`)에 쓰인다.

`Ok`, `Err`, `Some`, `None`, `Result`, `Option`, `Self`, `self`와 [14장](#14-내장-함수)의 내장 함수 이름은 예약된 이름이다. 내장 함수와 같은 이름의 함수를 정의하면 `` `string_from` is a built-in function and cannot be redefined`` 같은 오류가 난다(단, `wrapping_add`/`wrapping_sub`/`wrapping_mul`은 사용자 정의가 우선한다).

### 1.5 리터럴

| 종류 | 예 | 타입 |
|---|---|---|
| 10진 정수 | `42`, `1_000_000` | 문맥에서 정해지는 정수형. 정해지지 않으면 `i32` |
| 16/8/2진 정수 | `0xFF`, `0o17`, `0b1010_0001` | 위와 같음 |
| 실수 | `1.5`, `2.0`, `1e3`, `1.5e-2` | `f64` |
| 문자 | `'a'`, `'\n'` | 정수(문맥이 없으면 `i32`). 값은 ASCII/코드 포인트 |
| 문자열 | `"hello\n"` | `str` |
| 불리언 | `true`, `false` | `bool` |

- 정수 리터럴의 `_`는 자릿수 구분용이며 무시된다. 값의 범위는 **`u64` 최댓값까지** 쓸 수 있다(`0xFFFF_FFFF_FFFF_FFFF`). 그보다 크면 `integer literal out of range` 오류다.
- 정수 리터럴이 놓인 자리의 타입에 값이 들어가지 않으면 컴파일 오류다(`let x: u8 = 300`은 `integer literal out of range for u8`).
- `-`는 리터럴의 일부가 아니라 단항 연산자다. 단, 각 타입의 최솟값(`-128` for `i8`, `-9223372036854775808` for `i64`)은 올바르게 쓸 수 있다.
- 문자열 이스케이프: `\n` `\t` `\r` `\0` `\\` `\"`. 문자 리터럴은 추가로 `\'`를 쓴다. 그 밖의 이스케이프는 `unknown escape sequence` 오류다. 유니코드 이스케이프는 없다. 문자열은 UTF-8 바이트 그대로 저장된다(`"한".len`은 3).
- 문자열 리터럴은 줄을 넘길 수 없다(`unterminated string literal`).

```tam
// exit: 0
fn main() i32 {
    let a = 1_000 + 0xff + 0b101 + 0o17 // 1000 + 255 + 5 + 15
    if a != 1275 { return 1 }
    if "한".len != 3 { return 2 }
    if 'A' as i32 != 65 { return 3 }
    let big: u64 = 0xFFFF_FFFF_FFFF_FFFF
    if big != ~0 { return 4 }
    0
}
```

```tam,error
// error: integer literal out of range for `u8`
fn main() i32 {
    let x: u8 = 300
    0
}
```

### 1.6 연산자와 구분자

```
+  -  *  /  %  &  |  ^  ~  <<  >>
== != <  <= >  >=  &&  ||  !
=  += -= *= /= %= &= |= ^= <<= >>=
.  ..  ..=  ,  :  ::  ;  ?  =>
( ) [ ] { }
```

`>>`는 렉서가 한 토큰으로 만들지 않고, 파서가 **맞붙은** `>` 두 개를 오른쪽 시프트로 읽는다. 덕분에 `Result<Result<i32, i32>, i32>`의 `>>`가 타입을 닫는다. `a > > b`처럼 띄우면 시프트가 아니다.

`&&`는 논리곱 연산자이지만, 타입과 단항 위치에서는 `& &`로 읽는다(`&&T`는 `&(&T)`, `&&x`는 `&(&x)`).

### 1.7 줄바꿈과 문장의 끝

TAM은 세미콜론이 없다. **줄바꿈이 문장의 끝**이고, `;`는 한 줄에 여러 문장을 둘 때의 구분자로만 허용된다(줄 끝의 `;`도 허용된다).

한 문장은 다음 경우에 다음 줄로 이어진다.

1. 줄이 이항 연산자, `,`, `(`, `[`, `{`로 끝난다.
2. 다음 줄이 `.`으로 시작한다(메서드 체인).

그 외에는 줄바꿈이 문장의 끝이다. 특히 **다음 줄이 연산자로 시작하는 것은 이어짐이 아니다**.

```tam
// exit: 6
fn add(a: i32, b: i32) i32 { a + b }

fn main() i32 {
    let a = [
        1,
        2,
    ]
    let total = a[0] +
        a[1] +
        3
    add(
        total,
        0,
    )
}
```

```tam,error
// error: expected an expression, found `+`
fn main() i32 {
    let y = 1
        + 3
    y
}
```

---

## 2. 항목(Items)

파일의 최상위에는 함수, 구조체, `const`, `impl` 블록, `use` 선언만 둘 수 있다. 전역 **변수**는 없다(상수만 있다). 항목은 선언 순서와 무관하게 파일 전체에서 보인다(앞에서 뒤의 함수를 호출할 수 있다).

### 2.1 함수

```
function  = [ "pub" ] [ "unsafe" ] "fn" name "(" [ params ] ")" [ type ] block
params    = param { "," param } [ "," ]
param     = name ":" type
```

- 반환 타입은 매개변수 목록 뒤에 **화살표 없이** 쓴다. 생략하면 값이 없는 함수다(타입 표기가 없는 "단위" 함수. 호출 결과를 변수에 묶을 수 없다).
- 본문 블록의 **마지막 표현식이 반환값**이다. `return`은 조기 반환에 쓴다.
- 매개변수는 불변이다. 값을 바꾸려면 본문에서 `mut 이름 = 이름`으로 다시 묶는다.
- 함수는 재귀할 수 있다. 상호 재귀도 된다.
- 같은 이름의 함수를 두 번 정의하면 오류다.
- `pub`은 다른 모듈에서 보이게 한다([7장](#7-이름-해석과-모듈)).
- `unsafe fn`은 `unsafe` 블록이나 다른 `unsafe fn` 안에서만 호출할 수 있다.

```tam
// exit: 7
fn bump(x: &mut i32) {
    *x += 1
    if *x > 100 { return }
    *x += 1
}

fn fact(n: i64) i64 { if n <= 1 { 1 } else { n * fact(n - 1) } }

fn main() i32 {
    mut v = 1
    bump(&mut v)
    v + fact(3) as i32 - 2 // 3 + 6 - 2
}
```

### 2.2 구조체

```
struct = [ "pub" ] "struct" name "{" [ field { sep field } [ sep ] ] "}"
field  = name ":" type
sep    = "," | newline
```

- 필드는 쉼표나 줄바꿈으로 구분한다(둘 다 가능하고 끝의 쉼표는 선택).
- **필드는 항상 공개**다. 필드 단위의 `pub`은 없다. 구조체 이름의 `pub`만 모듈 밖 사용 여부를 정한다.
- 구조체 값은 **이동 타입**이다(필드가 모두 정수여도). 복사는 필드를 직접 읽어 새로 만든다([8장](#8-소유권-이동-자동-해제)).
- 구조체 리터럴은 모든 필드를 한 번씩, 순서 무관하게 쓴다. 필드 약식(`P { x }`)은 없다.
- 구조체는 자기 자신을 필드로 가질 수 없다(크기가 무한). 다른 구조체, 배열, `String`, `Result`/`Option`, 참조, 포인터를 필드로 가질 수 있다.

```tam
// exit: 6
struct Point {
    x: i32,
    y: i32,
}

struct Rect {
    min: Point
    max: Point
}

fn main() i32 {
    let r = Rect { max: Point { x: 4, y: 5 }, min: Point { x: 1, y: 2 } }
    (r.max.x - r.min.x) + (r.max.y - r.min.y)
}
```

### 2.3 `impl`

```
impl = "impl" name "{" { [ "pub" ] function } "}"
```

- `impl T { ... }`는 같은 **파일에 선언된 구조체** `T`에만 붙일 수 있다. 제네릭도 트레이트도 없다.
- 첫 매개변수가 `&self`, `&mut self`, `self`(값)이면 **메서드**, 아니면 **연관 함수**다. `Self`는 `T`를 뜻한다. `self` 매개변수에는 타입을 쓰지 않는다.
- 호출: 메서드는 `값.이름(인자)`, 연관 함수는 `T::이름(인자)`, 다른 모듈의 것은 `모듈::T::이름(인자)`.
- 값에 `&self`/`&mut self` 메서드를 부르면 자동으로 빌린다(`&mut self`는 변수, 필드, 배열 원소가 가변일 때). 이미 `&mut T`인 값이면 그대로 넘긴다(재차용). `&T`로는 `&mut self` 메서드를 부를 수 없다. `self`(값) 메서드는 값을 **이동**하며 참조 너머로는 부를 수 없다.
- 메서드는 `T::메서드(&값, ...)`로도 부를 수 있다.
- `pub`은 함수마다 붙인다(`impl`에는 붙일 수 없다). 다른 모듈에서는 `pub` 없는 메서드와 연관 함수를 부를 수 없다.
- 임시값(`make().get()`)에는 `&self` 메서드를 직접 부를 수 없다. 변수에 먼저 묶는다.
- 메서드 이름은 필드 이름과 같아도 된다(`p.x`와 `p.x()`는 다르다).

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
    let k = b.view().len as i32 // 4
    let s = b.size()            // 4 + 2
    k * 10 + s + b.into_len()   // 40 + 6 + 4
}
```

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

### 2.4 `use`

```
use = "use" name { "." name }
```

`use a.b.c`는 `<루트 폴더>/a/b/c.tam`을 불러오고 이후 `c::항목`으로 참조한다. 첫 조각이 `tam`이면 파일이 아니라 **내장 표준 라이브러리**다(`use tam.os`). 자세한 규칙은 [7장](#7-이름-해석과-모듈).

### 2.5 `extern` 함수

```
extern = [ "pub" ] "extern" string "fn" name "(" [ params ] ")" [ type ] [ "=" string ]
```

본문이 없는 외부 함수 선언이다. 문자열은 라이브러리 이름이다. 호스트 대상에서는 `"c"`(libc), Windows 대상에서는 DLL 이름(`"kernel32"`)이다. `= "심볼"`은 실제 심볼 이름이 TAM 이름과 다를 때만 쓴다.

- 인자와 반환 타입은 C ABI에 맞게 **정수형, `f64`, `bool`, raw 포인터**(와 반환 없음)만 가능하다. 그 밖의 타입은 오류다.
- 호출은 **항상 `unsafe`**다.
- 대상과 맞지 않는 라이브러리(Windows에서 `"c"`, 호스트에서 DLL)는 오류다. 펌웨어 대상(`uefi`, `bios`, `aarch64`)에서는 `extern`이 오류다.

```tam
// exit: 7
extern "c" fn abs(x: i32) i32

fn main() i32 {
    unsafe { abs(0 - 7) }
}
```

### 2.6 `const`: 전역 상수

```
const = [ "pub" ] "const" name ":" type "=" const_expr
```

- 타입은 **정수형, `f64`, `bool`, `str`** 중 하나여야 한다. 반드시 써야 한다(추론하지 않는다).
- 값은 **컴파일 시간에 계산**된다. 쓸 수 있는 것은 리터럴, 다른 상수(`mod::NAME` 포함), 산술·비트·비교·논리 연산, `!`, 단항 `-`, 정수끼리 또는 정수→`f64`의 `as`다. 함수 호출이나 변수는 쓸 수 없다(`this expression cannot be used in a constant`).
- 계산은 128비트로 하고, 결과가 선언한 타입에 들어가야 한다(`constant `A` = 256 does not fit in `u8``). 넘치거나 0으로 나누면 컴파일 오류다.
- 상수끼리 순환하면 `depends on itself` 오류다. 선언 순서는 상관없다.
- 상수를 쓰는 곳에는 **타입이 정해진 값**이 들어간다. 정수 리터럴과 달리 다른 정수형 자리에 그냥 쓸 수 없다(`const A: u8 = 1`을 `i32` 자리에 쓰려면 `A as i32`).
- 상수는 주소가 없다. 대입(`A = 2`)과 `&A`는 오류다.
- 같은 이름의 **지역 변수가 상수를 가린다**(그 선언 이후부터).
- 이름은 함수, 구조체, 다른 상수와 겹칠 수 없다. `pub const`는 다른 모듈에서 `모듈::이름`으로 쓴다.
- **배열 길이**에 쓸 수 있다: `[i32; SIZE]`, `[0; SIZE]`. 값은 1 이상의 정수 상수여야 한다.

```tam
// exit: 117
const SIZE: u64 = 4
const NEG: i32 = -3
const MASK: u8 = 0xF0 | 0x0F
const BIG: i64 = 1 << 40
const PI: f64 = 3.5
const NAME: str = "tam"
const ON: bool = SIZE > 3 && !false
const AREA: u64 = SIZE * SIZE

struct Grid {
    cells: [i32; SIZE],
}

fn total(g: &Grid) i32 {
    mut s = 0
    for i in 0..SIZE { s += g.cells[i] }
    s
}

fn main() i32 {
    let g = Grid { cells: [1; SIZE] }
    let SIZE = 100                       // 지역 변수가 상수를 가린다
    mut r = total(&g) + NEG + SIZE       // 4 - 3 + 100
    if BIG != 1099511627776 { return 1 }
    if MASK as i32 != 255 { return 2 }
    if PI * 2.0 != 7.0 { return 3 }
    if NAME.len != 3 { return 4 }
    if !ON { return 5 }
    r += AREA as i32                     // + 16
    r
}
```

```tam,error
// error: does not fit in `u8`
const A: u8 = 256

fn main() i32 { 0 }
```

### 2.7 제네릭

함수와 구조체에 **타입 매개변수**를 줄 수 있다. 이름 뒤에 `<A, B>`를 쓴다. 구현 방식은 C++ 템플릿과 같은 **단형화**다. 쓰이는 타입마다 컴파일러가 코드를 따로 만들어(인스턴스), 실행 시간 비용이 없다. 다만 C++과 달리 `template` 키워드가 없고, 인스턴스가 만들어지는 지점이 명시적이며, 오류가 어느 호출 때문인지 알려 준다.

```
function = ... "fn" name [ tparams ] "(" ... ")" ...
struct   = ... "struct" name [ tparams ] "{" ... "}"
impl     = "impl" name [ tparams ] "{" ... "}"
tparams  = "<" ident { "," ident } ">"
```

#### 제네릭 함수

```tam
// exit: 9
fn max<T>(a: T, b: T) T {
    if a > b { a } else { b }
}

fn main() i32 {
    max(3, 9)                    // T = i32 (인자에서 추론)
}
```

타입 인자는 다음 순서로 정해진다.

1. **명시한 것**: `max<i64>(3, 9)`.
2. **리터럴이 아닌 인자의 타입**: `max(x, y)`의 `x`, `y`.
3. **기대하는 타입**(반환 값이 쓰이는 자리): `let z: u8 = zero()`.
4. **정수 리터럴만 남으면 `i32`**: `max(3, 9)`.

하나라도 정해지지 않으면 `cannot infer type parameter `T` of `f`` 오류다.

```tam
// exit: 9
fn zero<T>() T { 0 }

fn first<T>(a: &[T]) T { a[0] }

fn main() i32 {
    let a: i32 = zero()          // 기대 타입으로 T = i32
    let b = zero<i64>()          // 명시
    let arr = [4, 5, 6]
    let c = first(&arr)          // &[i32; 3]에서 T = i32
    a + b as i32 + c + 5
}
```

호출 구문 `이름<타입, ...>(인자)`에서 `<`는 **뒤에 `>`와 `(`/`::`/`{`가 이어질 때만** 타입 인자로 읽는다. 그 밖에는 비교 연산자다(`a < b`).

#### 템플릿은 쓰일 때 검사된다

제네릭 함수의 **본문은 인스턴스가 만들어질 때** 실제 타입으로 검사된다. 그래서 `T`에 어떤 연산이 가능한지 미리 선언하지 않는다(트레이트 경계가 없다). 대신 인스턴스에서 오류가 나면 두 위치를 함께 보여 준다.

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

한 번도 쓰이지 않은 제네릭은 **검사되지 않는다**(문법은 검사된다). 함수 본문 안의 이름은 정의된 모듈에서, 호출하는 곳의 타입으로 해석된다.

#### 제네릭 구조체와 `impl`

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
    let p = Pair::new(2, 5)            // Pair<i32, i32>
    let q: Pair<i32, i32> = p.swap()
    let r = Pair<i32, i32> { first: 4, second: 1 }
    q.first + r.get_first() - 2
}
```

- 타입으로 쓸 때는 항상 인자를 준다: `Pair<i32, f64>`. 인자 없는 `Pair`는 `generic struct `Pair` needs type arguments` 오류다(리터럴과 연관 함수 호출은 추론한다).
- **리터럴** `Pair { first: 1, second: 2.5 }`는 필드 값에서 타입 인자를 추론한다. 명시할 수도 있다: `Pair<i32, f64> { ... }`.
- **연관 함수**는 `Pair::new(..)`로 추론하거나 `Pair<i32, f64>::new(..)`로 명시한다.
- `impl Pair<X, Y>`의 `X`, `Y`는 그 `impl` 안에서만 쓰는 이름이다(구조체 선언의 이름과 같을 필요는 없고, 개수만 같아야 한다).
- `impl` 안에서 `Self`는 `Pair<A, B>`다. `Self { .. }`와 `Self::new(..)`는 같은 인자를 쓴다.
- 인스턴스의 메서드는 처음 호출될 때 만들어진다. 같은 인스턴스의 같은 메서드는 하나만 생긴다.
- 인스턴스 이름은 `Pair<i32,f64>`처럼 오류 메시지에 그대로 나온다.
- 자기 자신을 **값으로** 포함하는 인스턴스(`struct Bad<T> { inner: Bad<T> }`)는 `contains itself by value` 오류다. 포인터로 가리키는 것(`next: *mut Node<T>`)은 된다.

#### 한계

- **트레이트(경계)는 없다.** `T`에 대한 요구는 인스턴스 오류로만 알려진다.
- **메서드는 자기만의 타입 매개변수를 가질 수 없다**(`impl Foo { fn f<T>(..) }` 오류). `impl Pair<A, B>`의 `A`, `B`만 쓴다. 필요하면 일반 제네릭 함수를 쓴다.
- 타입 매개변수 자리에 **상수 값**(`[T; N]`의 `N`)은 쓸 수 없다. 타입만 된다.
- `extern` 함수와 `main`은 제네릭일 수 없다.
- 자기 자신을 점점 큰 타입으로 인스턴스화하는 재귀는 **깊이 64, 인스턴스 5000개**에서 `generic instantiation limit exceeded` 오류로 멈춘다.
- 힙을 가진 필드는 `self.field`로 꺼내 이동할 수 없다는 기존 규칙([8장](#8-소유권-이동-자동-해제))은 제네릭에도 그대로다. `T`가 `String`인 인스턴스에서 `self.v`를 반환하면 인스턴스 검사에서 오류가 난다.

---

---

## 3. 타입

| 타입 | 설명 | Copy |
|---|---|---|
| `i8` `i16` `i32` `i64` | 부호 있는 정수(2의 보수) | ✔ |
| `u8` `u16` `u32` `u64` | 부호 없는 정수 | ✔ |
| `f64` | IEEE 754 배정밀도 | ✔ |
| `bool` | `true` / `false` | ✔ |
| `str` | 문자열 조각: (포인터, 길이). 읽기 전용, 소유하지 않음 | ✔ |
| `String` | 힙의 소유 문자열: (포인터, 길이, 용량) | ✘ (이동) |
| `struct` | 사용자 정의 | ✘ (이동) |
| `[T; N]` | 고정 길이 배열 | ✘ (이동) |
| `&T`, `&mut T` | 참조 | `&T` ✔, `&mut T` ✘(재차용) |
| `&[T]`, `&mut [T]` | 슬라이스(포인터, 길이) 참조 | `&[T]` ✔ |
| `*const T`, `*mut T` | raw 포인터 | ✔ |
| `Result<T, E>` | 성공 또는 오류 | 내용이 하나라도 이동 타입이면 ✘ |
| `Option<T>` | 값 또는 없음 | 위와 같음 |

`isize`, `usize`, `f32`, 튜플, 열거형, 제네릭, 함수 포인터 타입은 없다. 크기를 다루는 값은 `u64`를 쓴다(`.len`이 `u64`다).

### 3.1 정수형

- 정수 연산은 같은 타입끼리만 된다. `i32 + i64`는 오류이므로 `as`로 맞춘다.
- 리터럴은 기대 타입을 따른다. 기대 타입이 없으면 `i32`이고, 한쪽 피연산자의 타입이 정해져 있으면 그쪽을 따른다.
- 오버플로는 **trap**한다([10장](#10-수치-의미-오버플로-변환-시프트)).

```tam
// exit: 0
fn main() i32 {
    let a: u8 = 200
    let b = 100          // 리터럴 b는 아래 덧셈에서 u8로 정해진다
    let c = a - b as u8 + 100
    if c != 200 { return 1 }
    let wide = a as i64 * 1_000_000_000
    if wide != 200_000_000_000 { return 2 }
    0
}
```

```tam,error
// error: mismatched types
fn main() i32 {
    let a: i32 = 1
    let b: i64 = 2
    let c = a + b
    0
}
```

### 3.2 `str`과 `String`

- `str`은 (포인터, 길이)이며 Copy다. 읽을 수 있는 것은 `s.len`(`u64`)과 `s.ptr`(`*const u8`)뿐이다. **인덱싱은 없다**(`s[i]`는 오류). 바이트를 읽으려면 `unsafe { *(s.ptr + i) }`를 쓴다.
- `str == str`, `str != str`은 바이트 비교다.
- `String`은 힙을 소유하는 이동 타입이다. 스코프가 끝나면 자동으로 해제된다([8장](#8-소유권-이동-자동-해제)). `.len`, `.ptr`을 읽을 수 있다.
- `string_from(str) String`, `string_push(&mut String, u8)`, `as_str(&String) str`이 내장이다. 이어 붙이기와 정수 출력은 표준 라이브러리 `tam.fmt`에 있다([Standard Library](stdlib.md)).
- `as_str(&s)`가 돌려준 `str`은 `s`를 **빌린다**: `s`를 이동하거나 수정하면 오류다.

```tam
// exit: 0
fn main() i32 {
    let a = "hello"
    mut s = string_from("hel")
    string_push(&mut s, 108)
    string_push(&mut s, 111)
    let b = as_str(&s)
    if a != b { return 1 }
    if a == "world" { return 2 }
    if s.len != 5 { return 3 }
    0
}
```

```tam,error
// error: cannot borrow `s` as mutable because it is also borrowed as immutable
fn main() i32 {
    mut s = string_from("a")
    let t = as_str(&s)
    string_push(&mut s, 1)
    t.len as i32
}
```

### 3.3 배열

- `[T; N]`에서 `N`은 **1 이상의 정수 리터럴 또는 정수 상수의 이름**이다([2.6절](#26-const-전역-상수)). `[T; N * 2]` 같은 식과 길이 0은 불가다.
- 리터럴: `[a, b, c]`, 반복 `[v; N]`(`v`는 한 번만 평가해 복사). 타입 표기: `let a: [u8; 4] = [0; 4]`.
- 원소 타입은 **정수, `f64`, `bool`, raw 포인터, 그것들의 배열, 그것들만 필드로 가진 구조체**다. `String`이나 참조처럼 힙·빌림을 쥔 타입은 원소가 될 수 없다(`cannot be an array element type`).
- 배열은 구조체처럼 **이동 타입**이다. 값 전달과 대입은 이동이다.
- `a[i]`로 읽고 쓴다. 중첩(`a[i][j]`), 구조체 원소의 필드(`a[i].x`)도 된다. 인덱스는 **어떤 정수형이든** 된다.
- **경계를 실행 시간에 검사한다**. 벗어나면 trap(패닉 `index out of bounds`)한다. 부호 있는 음수 인덱스도 trap이다. 배열 길이와 인덱스가 **둘 다 컴파일 시간에 알려진 경우**(상수 인덱스)는 컴파일 오류다.
- `.len`(`u64`)과 `.ptr`(`*const T`, 가변 배열이면 `*mut T`)를 읽을 수 있다.
- 구조체 원소를 통째로 꺼내 옮길 수는 없다. 필드로 접근하거나 빌린다.

```tam
// exit: 14
fn main() i32 {
    mut grid = [[0; 3]; 3]
    for i in 0..3 {
        for j in 0..3 {
            grid[i][j] = (i * 3 + j) as i32
        }
    }
    let diag = grid[0][0] + grid[1][1] + grid[2][2] // 0 + 4 + 8 = 12
    diag + 2
}
```

```tam,error
// error: index out of bounds
fn main() i32 {
    let a = [1, 2, 3]
    a[5]
}
```

```tam,trap
// panic: index out of bounds
fn main() i32 {
    let a = [1, 2, 3]
    let i = a.len + 2
    a[i]
}
```

### 3.4 슬라이스

`&[T]`와 `&mut [T]`는 (포인터, 길이)인 **참조**다.

- 만들기: `&a[..]`, `&a[lo..hi]`, `&a[lo..]`, `&a[..hi]`, `&mut a[..]`. 원본은 배열, `&[T; N]`, 슬라이스다. 슬라이스 식은 반드시 `&`/`&mut`로 빌려서 쓴다.
- 범위가 길이를 넘거나 `lo > hi`이면 trap한다.
- `s.len`(`u64`), `s[i]`(경계 검사)를 쓴다. `for x in s`로 순회할 수 있다(원소는 **값으로 복사**된다).
- 함수 인자 자리에서는 `&배열`(`&[T; N]`)이 `&[T]`로 자동 변환된다(`sum(&a)`). 다른 자리(`let s: &[T] = &a`)에서는 `&a[..]`로 명시한다.
- 빌림 검사는 일반 참조와 같다. 원본 배열을 바꾸면 오류이고, `&mut` 슬라이스 둘은 동시에 만들 수 없고, 지역 배열의 슬라이스는 반환할 수 없다.
- 검사기는 **원소를 구분하지 않는다**: `&mut a[0]`과 `&mut a[1]`은 동시에 만들 수 없다.

```tam
// exit: 15
fn sum(a: &[i32]) i32 {
    mut s = 0
    for x in a { s += x }
    s
}

fn fill(a: &mut [i32], v: i32) {
    for i in 0..a.len { a[i] = v }
}

fn main() i32 {
    mut a = [1, 2, 3, 4, 5]
    let total = sum(&a)              // 15, &배열 → &[i32] 자동 변환
    fill(&mut a[1..3], 9)
    if a[1] != 9 || a[2] != 9 { return 1 }
    total
}
```

### 3.5 참조

`&T`는 읽기 전용 참조, `&mut T`는 배타적 쓰기 참조다. 규칙은 [9장](#9-빌림-규칙)에 있다.

- `&x`, `&mut x`로 만든다. `&mut x`는 `x`가 `mut`일 때만 된다.
- `*r`로 읽고 `*r = v`로 쓴다.
- 구조체 참조의 필드는 `r.x`로 접근한다(자동 역참조).
- `&mut T`는 `&T` 자리에 넘길 수 있다(암묵 변환). 반대는 오류다.
- 함수가 반환하는 참조의 수명은 어노테이션 없이 본문에서 추론한다. 지역 변수의 참조를 돌려주면 컴파일 오류다.

### 3.6 `Result`와 `Option`

- `Result<T, E>`: `Ok(값)` 또는 `Err(오류)`. `Option<T>`: `Some(값)` 또는 `None`.
- 생성자는 **기대 타입이 있는 자리**(반환 값, 인자, 주석 달린 `let`)에서 쓴다. `Some(x)`는 인자에서 타입을 추론하므로 `let o = Some(5)`는 된다. `Ok(...)`, `Err(...)`, `None`은 기대 타입이 필요하다.
- `Result`/`Option`은 이동 타입이거나(내용에 `String`·구조체·배열이 있을 때) Copy다.
- 소비: `match`([6장](#6-match와-패턴))와 `?`([4.9절](#49-오류-전파-)), 그리고 메서드:

| 메서드 | `Option<T>` | `Result<T, E>` |
|---|---|---|
| `is_some()` `is_none()` | ✔ | |
| `is_ok()` `is_err()` | | ✔ |
| `unwrap()` | `None`이면 패닉 | `Err`이면 패닉 |
| `unwrap_or(기본값)` | ✔ | ✔ |

힙을 가진 내용(예: `Option<String>`)에서 `is_*`는 값을 소비하므로 거절된다(`owns heap data`): `match`를 쓴다. `unwrap`은 값을 꺼내 이동한다.

```tam
// exit: 0
fn find(a: &[i32], x: i32) Option<i32> {
    for i in 0..a.len {
        if a[i] == x { return Some(i as i32) }
    }
    None
}

fn parse(n: i32) Result<i32, i32> {
    if n > 0 { Ok(n) } else { Err(0 - n) }
}

fn main() i32 {
    let a = [5, 6, 7]
    if find(&a, 7).unwrap() != 2 { return 1 }
    if find(&a, 9).is_some() { return 2 }
    if find(&a, 9).unwrap_or(-1) != -1 { return 3 }
    if parse(-4).unwrap_or(7) != 7 { return 4 }
    if !parse(3).is_ok() { return 5 }
    0
}
```

### 3.7 타입의 크기와 배치

타입의 메모리 배치는 정해지지 않았다(LLVM의 자연 정렬을 따르지만 약속하지 않는다). `extern` 함수의 인자로 구조체를 넘길 수 없는 이유다.

---

## 4. 표현식

### 4.1 우선순위와 결합 방향

높은 것부터:

| 순위 | 연산자 | 결합 |
|---|---|---|
| 1 | 후위: `.필드` `.메서드()` `[인덱스]` `?` 호출 `()` | 왼쪽 |
| 2 | 단항: `-` `!` `~` `*`(역참조) `&` `&mut` | 오른쪽 |
| 3 | `as` | 왼쪽 |
| 4 | `*` `/` `%` | 왼쪽 |
| 5 | `+` `-` | 왼쪽 |
| 6 | `<<` `>>` | 왼쪽 |
| 7 | `&` | 왼쪽 |
| 8 | `^` | 왼쪽 |
| 9 | `|` | 왼쪽 |
| 10 | `<` `<=` `>` `>=` | 왼쪽 |
| 11 | `==` `!=` | 왼쪽 |
| 12 | `&&` | 왼쪽 |
| 13 | `||` | 왼쪽 |

Rust와 같다. 비트 연산자(`& ^ |`)는 비교보다 **높다**: `x & 1 == 1`은 `(x & 1) == 1`이다(C와 다르다). `-x as i64`는 `(-x) as i64`다.

```tam
// exit: 0
fn main() i32 {
    let a = 1 + 2 * 3 - 4 / 2 % 3  // 1 + 6 - (2 % 3) = 5
    let b = 1 << 2 + 1             // 시프트가 덧셈보다 낮다: 1 << 3
    let c = 6 & 3 == 2             // (6 & 3) == 2
    let d = 5 > 3 == true          // (5 > 3) == true
    let e = -5 as i64 + 1          // ((-5) as i64) + 1
    if a != 5 { return 1 }
    if b != 8 { return 2 }
    if !c { return 3 }
    if !d { return 4 }
    if e != -4 { return 5 }
    0
}
```

### 4.2 평가 순서

피연산자와 인자는 **왼쪽에서 오른쪽**으로 평가한다. `&&`와 `||`는 단락 평가한다.

```tam
use tam.os

fn say(n: i32) bool { os::print_int(n as i64); true }

fn main() i32 {
    let a = false && say(1) // say(1)은 호출되지 않는다
    let b = true || say(2)  // say(2)도
    let c = say(3) && say(4)
    os::println("")
    if a || !b || !c { 1 } else { 0 }
}
// out: 34
```

### 4.3 산술과 비교

- `+ - * / %`는 정수와 `f64`에 쓴다. 두 피연산자의 타입이 같아야 한다. `%`는 `f64`에도 쓸 수 있다(`7.5 % 2.0 == 1.5`).
- 정수 나눗셈은 0 방향으로 자르고, `%`의 부호는 피연산자(왼쪽)를 따른다(`-5 % 3 == -2`).
- 단항 `-`는 부호 있는 정수와 `f64`에만 쓴다(`-x`가 `u32`에는 오류).
- `== != < <= > >=`는 같은 타입의 정수, `f64`, `bool`(`==`/`!=`), `str`(`==`/`!=`), 포인터에 쓴다. 결과는 `bool`이다. 구조체와 배열의 비교는 없다.
- `f64` 비교는 IEEE 754를 따른다(`NaN != NaN`).
- `if`/`while`의 조건은 **`bool`이어야 한다**(`if 1 { }`는 오류).
- `&& || !`는 `bool`에만 쓴다.

### 4.4 비트 연산자

`& | ^ << >>`와 단항 `~`는 정수형에만 쓴다. `& | ^`는 양쪽 타입이 같아야 하고 시프트 양은 어떤 정수형이든 된다. `>>`는 부호 있는 타입에서 산술, 부호 없는 타입에서 논리 시프트다. **시프트 양은 왼쪽 피연산자의 비트 폭 - 1로 마스크**된다(폭 이상 밀어도 정의된 동작): `i32`에서 `1 << 33`은 `1 << 1`이다.

```tam
// exit: 0
fn main() i32 {
    let m: i64 = -1
    if (m as u64 >> 60) != 15 { return 1 }   // 논리 시프트
    let n: i32 = -8
    if (n >> 1) != -4 { return 2 }           // 산술 시프트
    let s: u32 = 33
    if (1 << s) != 2 { return 3 }            // 33 & 31 == 1
    if ~0 != -1 { return 4 }
    0
}
```

### 4.5 변환 `as`

| 원본 → 대상 | 동작 |
|---|---|
| 정수 → 정수 | 확장(부호 있으면 부호 확장) 또는 하위 비트로 **잘라냄**(감싼다, trap하지 않음) |
| 정수 → `f64` | 가장 가까운 값 |
| `f64` → 정수 | 0 방향 절삭 후 **포화**(범위 밖은 최솟값/최댓값, `NaN`은 0) |
| `bool` → 정수 | `false` = 0, `true` = 1 |
| 정수 → `bool` | **불가**(오류). `x != 0`을 쓴다 |
| 포인터 ↔ 포인터, 포인터 ↔ `i64`/`u64` | 가능(safe) |
| `&T` → `*const T`, `&mut T` → `*mut T` | 가능(safe) |

```tam
// exit: 0
fn main() i32 {
    let a: u8 = 250
    if (a as i8) != -6 { return 1 }          // 잘라내고 부호 재해석
    if (300 as u8) != 44 { return 2 }
    if (1e20 as i32) != 2147483647 { return 3 } // 포화
    if (0.0 / 0.0) as i32 != 0 { return 4 }     // NaN → 0
    if (-5.7 as i32) != -5 { return 5 }
    if (-5.7 as u8) != 0 { return 6 }
    if (true as i32) != 1 { return 7 }
    0
}
```

```tam,error
// error: cannot cast `i32` to `bool`
fn main() i32 {
    let b = 5 as bool
    0
}
```

### 4.6 구조체 리터럴, 필드, 인덱스

- `Name { f: v, ... }`. 모듈 밖 구조체는 `모듈::Name { ... }`.
- `if`/`while`/`for`/`match`의 **조건 식 자리에서는** `Name {`를 구조체 리터럴로 읽지 않는다(블록의 시작과 구분하기 위해). 괄호 `( )` 안에서는 다시 구조체 리터럴을 쓸 수 있다.
- `값.필드`, 참조와 포인터가 아닌 값의 체인 `a.b.c`, 인덱스 `a[i]`, `a[i].x`, `a[i][j]`.
- **힙을 쥔 필드를 구조체에서 꺼내 이동하는 것은 오류다**: `let t = p.s`(`p.s`가 `String`)는 `cannot move ... out of a field`. 값 전체를 옮기거나 `&p.s`로 빌린다. 힙을 쥔 임시값의 필드 접근(`make().s`)도 막는다.

### 4.7 `if` 표현식

`if 조건 { 값 } else { 값 }`은 값을 가진다. 두 가지의 타입이 같아야 한다. `else`가 없는 `if`는 값이 없는 문장이다. `else if`로 이을 수 있다.

```tam
// exit: 20
fn main() i32 {
    let x = 5
    let a = if x > 3 { 10 } else { 20 }
    let b = if a == 10 { 1 } else if a == 20 { 2 } else { 3 }
    mut r = 0
    if b == 1 { r = 20 }   // else 없는 if는 문장
    r
}
```

블록 `{ ... }` 자체를 값으로 쓰는 표현식(`let x = { ... }`)은 문법에 없다. 값이 필요하면 `if true { ... }`가 아니라 함수로 뽑는다.

### 4.8 호출

`이름(인자, ...)`, 모듈 함수는 `모듈::이름(...)`, 연관 함수는 `T::이름(...)`, 메서드는 `값.이름(...)`. 인자의 수와 타입은 정확히 맞아야 한다. 함수 값(함수 포인터)과 클로저는 없다.

### 4.9 오류 전파 `?`

`식?`는 다음과 같이 동작한다.

- 식이 `Result<T, E>`: `Ok(v)`면 `v`가 식의 값이 되고, `Err(e)`면 현재 함수가 **즉시 `Err(e)`를 반환**한다(살아 있는 지역 변수를 모두 해제하고 나간다).
- 식이 `Option<T>`: `Some(v)`면 `v`, `None`이면 즉시 `None`을 반환한다.
- 현재 함수의 반환 타입이 같은 종류(`Result<_, E>`에서 **오류 타입 `E`가 정확히 같아야** 한다, 또는 `Option<_>`)여야 한다. `Option`과 `Result`는 `?`로 섞을 수 없다.
- 오류 타입 변환(`From`)은 없다.

```tam
// exit: 30
fn div(a: i32, b: i32) Result<i32, i32> {
    if b == 0 { return Err(1) }
    Ok(a / b)
}

fn calc(a: i32, b: i32, c: i32) Result<i32, i32> {
    let x = div(a, b)?
    let y = div(x, c)?
    Ok(x + y)
}

fn main() i32 {
    let ok = match calc(100, 5, 4) { Ok(v) => v, Err(e) => 0 - e }  // 20 + 5 = 25
    let bad = match calc(1, 0, 4) { Ok(v) => v, Err(e) => 0 - e }   // -1
    ok - bad + 4
}
```

```tam,error
// error: `?` can only be used
fn f() Result<i32, i32> { Ok(1) }
fn main() i32 {
    let x = f()?
    x
}
```

### 4.10 `unsafe` 표현식

`unsafe { 문장들; 값 }`은 값을 가지는 표현식이다. [11장](#11-unsafe와-raw-포인터) 참고.

---

## 5. 문장

### 5.1 변수 선언

```
let 이름 [ ":" 타입 ] "=" 식        // 불변
mut 이름 [ ":" 타입 ] "=" 식        // 가변
```

- **`let mut`이 아니라 `mut`**다(`let mut x`는 오류).
- 초기값은 필수다(초기화되지 않은 선언은 없다).
- 지역 변수의 타입은 추론한다. 타입 주석은 리터럴의 정수형을 정하거나 `Ok`/`Err`/`None`의 기대 타입을 줄 때 쓴다.
- **섀도잉**이 가능하다: 같은 이름으로 다시 선언하면 이전 변수는 가려진다(이전 값은 스코프가 끝날 때 해제된다).
- 값이 없는 식(반환 타입이 없는 함수 호출)을 묶을 수 없다: `cannot bind ... to a value-less expression`.

### 5.2 대입

- `대상 = 식`. 대상은 가변 변수, 가변 변수의 필드, 배열 원소, `*참조` 등 **장소 식**이다. 불변 변수에 대입하면 `cannot assign to immutable variable`이다.
- 재대입은 이전 값을 **먼저 해제**한다(`x = v`, `p.s = v`, `*r = v`가 `String`일 때).
- **복합 대입** `+= -= *= /= %= &= |= ^= <<= >>=`. `a op= b`는 `a = a op b`로 풀어 쓴다. 따라서 대상 식에 **부작용이 있으면 오류**다(`a[f()] += 1`).

### 5.3 `while`

```
while 조건 { 본문 }
```

`break`는 가장 안쪽 루프를 끝내고 `continue`는 다음 반복으로 간다. 레이블과 `break 값`은 없다.

#### `loop`

```
loop { 본문 }
```

`while true { 본문 }`과 같다. 끝나려면 `break`나 `return`이 필요하다. **본문에 (이 루프를 끝내는) `break`가 없는 `loop`는 끝나지 않으므로**, 함수의 마지막 문장이 그런 `loop`이면 값을 돌려주는 함수에도 최종 식이 필요 없다. 안쪽 `while`/`loop`의 `break`는 바깥 루프를 끝내지 못하므로 세지 않는다.

```tam
// exit: 13
fn find() i32 {
    mut i = 0
    loop {
        i += 1
        if i == 5 { return i }      // break가 없다: 이 loop 뒤에는 아무것도 필요 없다
    }
}

fn main() i32 {
    mut n = 0
    loop {
        n += 2
        if n > 7 { break }
    }
    n + find()                       // 8 + 5
}
```

`break`/`continue`를 루프 밖에서 쓰면 오류다. `break`/`continue`로 나갈 때 **그 루프 본문 스코프의 힙 값은 해제된다**.

### 5.4 `for`

```
for 이름 in 시작..끝 { }       // 끝은 제외
for 이름 in 시작..=끝 { }      // 끝을 포함
for 이름 in 슬라이스 { }       // 또는 &배열 — 원소를 값으로 복사
```

- `while`로 풀어 쓴다. `continue`도 증가 문장을 거친다. `a..=최댓값`도 넘치지 않는다.
- **루프 변수는 불변**이다.
- 범위의 정수 리터럴 끝점만 있으면 `i`는 `i32`다. 다른 정수형이 필요하면 끝점을 해당 타입의 변수로 두거나 `as`를 쓴다.
- 슬라이스를 순회하면 `x`는 **원소의 복사본**이다(참조가 아니다). 원소를 바꾸려면 인덱스로 순회한다.
- 이터레이터는 없다. 범위와 슬라이스/배열 참조만 된다.

```tam
// exit: 16
fn main() i32 {
    mut s = 0
    mut i = 0
    while i < 10 {
        i += 1
        if i % 2 == 0 { continue }
        if i > 7 { break }
        s += i                // 1 + 3 + 5 + 7
    }
    for k in 0..=3 { s += k } // + 6
    let pair = [1, 2]
    for x in &pair[..] { s += x }
    s - 9
}
```

### 5.5 `return`

`return 식` 또는 값이 없는 함수의 `return`. 함수를 즉시 끝내며 살아 있는 모든 지역 변수의 힙 값을 해제한다.

### 5.6 식 문장

값을 버리는 식. 버려지는 임시 값이 힙을 가지면(예: `string_from("x")`) 즉시 해제된다.

---

## 6. `match`와 패턴

```
match 식 {
    패턴 => 식 | 블록
    ...
}
```

- 대상은 `Result` 또는 `Option` 값뿐이다. 패턴은 `Ok(이름)`, `Err(이름)`, `Some(이름)`, `None`이다. 한 `match`에서 `Ok/Err`와 `Some/None`을 섞을 수 없다.
- 두 팔을 **정확히 한 번씩**, 순서 무관하게 쓴다. 빠지면 `` `match` is missing the `None` arm`` 오류다.
- 팔은 쉼표나 줄바꿈으로 구분한다. 팔의 본문은 표현식이거나 `{ }` 블록이고, `if`처럼 **값을 가진다**: 모든 팔의 타입이 같아야 한다(값이 없는 문장 팔은 모두 값이 없어야 한다).
- 바인딩 이름은 payload를 **이동하거나 복사**한다. payload가 힙을 가진 `Option<String>`을 `match`하면 `Some(s)`가 소유권을 가져간다.
- 바인딩 이름 자리에 `_`를 쓰면 payload를 쓰지 않겠다는 뜻이다: `Ok(_)`, `Err(_)`, `Some(_)`. payload가 힙을 가져도 팔이 끝날 때 해제된다.
- 정수, 문자열 등에 대한 `match`와 팔 전체를 대신하는 `_ =>` 패턴은 없다. 그 경우 `if`/`else if`를 쓴다.

```tam
// exit: 0
fn describe(o: Option<i32>) i32 {
    match o {
        Some(v) => {
            v + 1
        }
        None => 0
    }
}

fn main() i32 {
    if describe(Some(4)) != 5 { return 1 }
    if describe(None) != 0 { return 2 }
    let r: Result<i32, i32> = Err(3)
    match r { Ok(v) => v, Err(e) => e } - 3
}
```

---

## 7. 이름 해석과 모듈

### 7.1 파일 = 모듈

- `use a.b`는 파일 `<루트 폴더>/a/b.tam`을 불러온다. 이후 `b::항목`으로 참조한다(모듈 경로는 **한 단계**: `alias::item`).
- 루트 폴더는 컴파일을 시작한 파일(`tamc build main.tam`)의 폴더, 프로젝트 모드에서는 패키지의 `src/` 폴더다.
- 불러온 모듈에서도 `use`를 쓸 수 있다(경로는 항상 루트 폴더 기준). 순환 `use`는 허용된다.
- 같은 이름의 모듈 별칭을 두 번 가져오면(`use a.math` 다음에 `use b.math`) `module name `math` is imported more than once` 오류다. 별칭 바꾸기(`as`)는 없다.
- 모듈을 못 찾으면 어떤 파일을 기대했는지 알려 주는 오류가 난다.

```tam,ignore
// main.tam                  // util/math.tam
use util.math                // pub struct Pt { x: i32, y: i32 }
fn main() i32 {              // pub fn add(a: i32, b: i32) i32 { a + b }
    let p = math::Pt { x: 1, y: 2 }
    math::add(p.x, p.y)
}
```

### 7.2 가시성

- 모듈 밖에서 쓰려면 `pub fn` / `pub struct` / `pub extern`이어야 한다. 비공개 항목에 접근하면 **두 파일을 함께 보여 주는 오류**가 난다.
- 구조체 필드는 항상 공개다.
- 재공개(`pub use`)는 없다.
- `const`도 `pub const`만 다른 모듈에서 `모듈::이름`으로 쓸 수 있다. 제네릭 함수·구조체도 같은 규칙이다(`pub fn max<T>`, `pub struct Stack<T>`). 다른 모듈의 제네릭 구조체는 `모듈::Stack<i32>`, 연관 함수는 `모듈::Stack<i32>::new()`로 쓴다.

### 7.3 맹글링과 이름 충돌

루트 항목은 이름 그대로, 다른 모듈의 항목은 `util.math::add` 같은 이름으로 내려간다(LLVM에서는 따옴표 이름). 모듈 안의 항목은 다른 모듈의 같은 이름과 충돌하지 않는다. 오류 메시지의 타입 이름에는 `geo::Point`처럼 모듈이 붙어서 나온다.

### 7.4 표준 라이브러리

`use tam.os`, `use tam.fmt`, `use tam.serial`처럼 **첫 조각이 `tam`이면** 프로젝트 파일 대신 컴파일러에 내장된 TAM 소스를 쓴다. `tam.os`는 대상에 따라 구현이 다르고 공개 API는 같다. 자세한 내용은 [The Standard Library API Documentation](stdlib.md).

### 7.5 패키지와 의존성

프로젝트 모드(`Tmake.toml`)에서는 **의존성 패키지의 이름이 모듈 이름**이 된다. 의존하는 패키지 `fmt-lib`는 `use fmt_lib`로 불러온다(`-`는 `_`로 바뀐다). 의존성 이름은 같은 이름의 지역 모듈보다 우선한다. 자세한 규칙은 [Tmake.toml Documentation](tmake.md).

---

## 8. 소유권, 이동, 자동 해제

TAM은 가비지 컬렉터도 런타임도 없이 메모리를 안전하게 다루기 위해, 컴파일 시간에 **소유권**을 검사한다. 모든 값에는 소유자가 하나 있고, 소유자가 스코프를 벗어나면 값이 해제된다.

### 8.1 Copy 타입과 이동 타입

- **Copy**: 정수, `f64`, `bool`, `str`, `&T`, `&[T]`, raw 포인터. 대입과 인자 전달은 복사다.
- **이동 타입**: `String`, 구조체, 배열, `&mut T`, 이동 타입을 담은 `Result`/`Option`. 대입, 인자 전달, 반환, 구조체 필드 초기화는 **이동**이며 이동된 변수는 이후 쓸 수 없다(`use of moved value`).
- **구조체는 필드가 모두 Copy여도 이동 타입**이다. 배열도 마찬가지다.

```tam,error
// error: use of moved value `a`
struct P { x: i32 }
fn main() i32 {
    let a = P { x: 1 }
    let b = a
    a.x + b.x
}
```

### 8.2 이동 분석

- 분기마다 이동 상태를 합친다. 한쪽 분기에서만 이동했다면 이후에는 "이동했을 수도 있음"이므로 쓸 수 없다.
- 루프 안에서 바깥 변수를 이동하면, 다음 반복에서 다시 쓰이므로 오류다(`use of moved value` — "value moved here, in previous iteration of loop"). 이동 직후에 `break`하면 괜찮다.
- `break`/`continue`는 그 시점의 이동 상태를 루프 뒤(`break`)나 다음 반복(`continue`)으로 합친다.
- 이동은 항상 **값 전체** 단위다. 구조체에서 힙을 쥔 필드 하나만 꺼내 이동할 수는 없다([4.6절](#46-구조체-리터럴-필드-인덱스)).

### 8.3 자동 해제(drop)

- `String`을 가진 타입(구조체 필드, 배열 원소가 아닌 `Result`/`Option` payload 포함)은 **스코프가 끝날 때 선언의 역순으로** 해제된다.
- 이동되었는지는 변수마다 둔 런타임 플래그로 판단하므로 **조건부 이동도 정확히** 해제/생략된다.
- `return`, `?`, `break`, `continue`는 빠져나가는 스코프의 살아 있는 값을 모두 해제하고 나간다.
- 재대입은 이전 값을 먼저 해제한다. 버려지는 임시 값(`string_from("x")` 문장)도 즉시 해제된다.
- 호스트에서 valgrind로 누수·이중 해제·잘못된 접근이 없음을 테스트로 확인한다.
- **해제되지 않는 경우**: `os::exit`는 즉시 종료하므로 살아 있는 지역 변수의 해제를 실행하지 않는다. trap/패닉도 해제를 실행하지 않는다. 할당 실패(`malloc`이 NULL)는 검사하지 않는다.
- 사용자 정의 소멸자(`Drop`)와 `Box<T>`는 없다. 직접 만든 힙 구조는 `unsafe`의 `alloc`/`free`로 관리한다.

```tam
// exit: 3
struct User {
    name: String,
    age: i32,
}

fn age_plus_len(u: User) i32 { u.age + u.name.len as i32 } // u는 여기서 해제된다

fn main() i32 {
    let u = User { name: string_from("ab"), age: 1 }
    age_plus_len(u)
    // u는 이동되었으므로 여기서 다시 해제되지 않는다(valgrind 클린)
}
```

---

## 9. 빌림 규칙

참조(`&T`, `&mut T`, `&[T]`, `&mut [T]`, `as_str`의 결과 `str`)는 **소유자를 빌린다**. 컴파일러는 다음을 검사한다.

### 9.1 규칙

1. 같은 값에 대해 **읽기 참조는 여러 개**, 또는 **쓰기 참조는 하나**만 동시에 살아 있을 수 있다(둘을 섞을 수 없다).
2. 참조가 살아 있는 동안 소유자를 **이동**하거나 **대입**할 수 없다.
3. 참조는 소유자보다 오래 살 수 없다(지역 변수의 참조 반환, 스코프 밖 사용은 오류).
4. `&mut`는 `mut` 변수에서만 만든다.

### 9.2 "살아 있는 구간"

참조가 살아 있는 구간은 **마지막 사용까지**다(NLL 방식). 참조를 이후에 쓰지 않으면 충돌 후 문장도 허용된다.

- 구조체의 **서로 다른 필드**는 따로 빌릴 수 있다(`&mut p.x`와 `&mut p.y`). 한 구조체 전체와 그 필드는 충돌한다.
- **배열 원소는 구분하지 않는다**: `&mut a[0]`과 `&a[1]`은 충돌한다(보수적).
- `&mut` 참조를 값으로 쓰면 암묵적 **재차용**이며, 재차용이 끝날 때까지 원본은 쓸 수 없다.
- 함수가 반환하는 참조의 출처는 **lifetime 표기 없이** 본문에서 추론한다. 여러 인자에서 올 수 있으면 보수적으로 모두를 빌린 것으로 본다.
- 참조 변수에 다시 대입해도 이전 빌림이 풀리지 않는다(보수적). 드물게 올바른 코드가 거부될 수 있다.
- `break`/`continue` 이후의 경로는 보수적으로 "모든 이후 경로가 도달 가능"으로 본다. 안전하지만 드물게 유효한 코드를 거절할 수 있다.
- raw 포인터는 빌림 검사의 대상이 아니다. 책임은 `unsafe`를 쓰는 쪽에 있다.

```tam,error
// error: cannot assign to `v` because it is borrowed
fn main() i32 {
    mut v = 1
    let r = &v
    v = 2
    *r
}
```

```tam,error
// error: cannot borrow `s` as mutable more than once at a time
fn main() i32 {
    mut s = string_from("a")
    let r = &mut s
    let q = &mut s
    string_push(r, 1)
    0
}
```

```tam,error
// error: cannot return reference to local variable `x`
fn f() &i32 {
    let x = 5
    &x
}
fn main() i32 { *f() }
```

```tam
// exit: 5
struct P { x: i32, y: i32 }

fn main() i32 {
    mut p = P { x: 1, y: 2 }
    let a = &mut p.x      // 서로 다른 필드는 동시에 빌릴 수 있다
    let b = &mut p.y
    *a += 1
    *b += 1
    p.x + p.y
}
```

---

## 10. 수치 의미: 오버플로, 변환, 시프트

- 정수 `+ - *`와 단항 `-`는 오버플로하면 **trap**한다(패닉 `integer overflow`). `/`, `%`는 0으로 나누거나 `MIN / -1`이면 trap한다(패닉 `integer overflow in division` / `division by zero`).
- 상수끼리의 식(`2147483647 + 1`)도 실행 시간 trap이다.
- **감싸는 연산은 명시한다**: `wrapping_add(a, b)`, `wrapping_sub(a, b)`, `wrapping_mul(a, b)`(정수형, 두 인자의 타입이 같아야 한다).
- `f64`와 포인터 산술, 비트 연산, 시프트는 trap하지 않는다.
- `as` 변환은 감싸거나 포화한다. trap하지 않는다([4.5절](#45-변환-as)).

```tam
// exit: 0
fn main() i32 {
    let a: i32 = 2147483647
    if wrapping_add(a, 1) != -2147483647 - 1 { return 1 }
    let b: u8 = 200
    if wrapping_mul(b, 2) != 144 { return 2 }
    0
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

---

## 11. `unsafe`와 raw 포인터

### 11.1 raw 포인터

`*const T`와 `*mut T`는 빌림 검사를 받지 않는 포인터다. 만들기와 변환은 safe다: `&mut x as *mut i32`, 포인터 ↔ 포인터, 포인터 ↔ `i64`/`u64`.

### 11.2 `unsafe`가 필요한 것

- 포인터 **역참조** `*p`
- 포인터 **산술**(`p + n`, `p - n`, 단위는 원소)
- `unsafe fn`과 `extern` 함수의 **호출**
- `alloc`, `free`, 하드웨어 접근 내장 함수(`mmio_*` 등)의 호출

`unsafe { ... }` 블록은 **값을 가지는 표현식**이다(마지막 식이 값). `unsafe fn`의 본문 전체는 `unsafe` 맥락이다.

### 11.3 책임

`unsafe` 안에서 컴파일러는 포인터의 유효성(정렬, 범위, 수명, 별칭)을 검사하지 않는다. 위반하면 정의되지 않은 동작이다. 대신 `unsafe` 바깥은 컴파일러가 안전을 책임진다. 이것이 TAM의 철학이다: **사용자에게 통제권을 모두 주되, 안정성은 컴파일러가 책임진다.** `unsafe`의 표면은 작게 유지하고, 안전한 API로 감싸 쓰라.

```tam
// exit: 9
fn main() i32 {
    let m = unsafe { alloc(16) }          // 16바이트 힙 블록
    let q = m as *mut i32
    let r = unsafe {
        *q = 4
        *(q + 1) = 5                      // 포인터 산술: 원소(i32) 단위
        let sum = *q + *(q + 1)
        free(m)
        sum
    }
    r
}
```

```tam,error
// error: needs an `unsafe` block
fn main() i32 {
    let m = alloc(16)
    0
}
```

---

## 12. 프로그램 실행: 진입점, 종료, 패닉, trap

### 12.1 진입점

프로그램에는 정확히 `fn main() i32`가 하나 있어야 한다. 없으면 `no `main` function found`, 시그니처가 다르면 `` `main` must have the signature `fn main() i32` ``. (프로젝트 모드의 `kind = "lib"` 패키지는 `main`이 필요 없고 검사만 된다.)

`main`이 돌려주는 값이 **종료 코드**다. Linux에서는 하위 8비트가 프로세스 종료 상태가 되고, Windows에서는 32비트 전체가 `ExitProcess`로 전달된다. UEFI에서는 `EFI_STATUS`로 돌려준다.

### 12.2 패닉과 trap

컴파일러가 넣은 검사(오버플로, 0 나눗셈, 경계 위반)와 `panic`, `assert`, `unwrap` 실패는 **trap**으로 프로그램을 끝낸다.

- **호스트**: stderr에 `tam: panic: <메시지>`를 쓰고 `SIGILL`(신호 4)로 종료한다. libc 없이 `write` 시스콜을 직접 쓴다. 메시지는 `integer overflow`, `integer overflow in division`, `division by zero`, `index out of bounds`, `assertion failed`, ``called `unwrap()` on a `None` value`` 등이다. `tamc run`은 `128 + 신호 번호`로 종료 코드를 알린다.
- **그 밖의 대상**: 메시지 없이 `ud2`/`brk`로 내려간다. 펌웨어 대상에서는 예외 핸들러를 설치하지 않으므로 예외가 나면 리셋하거나 멈춘다.
- trap과 패닉은 **자동 해제를 실행하지 않는다**.

```tam,trap
// panic: tam: panic: boom
fn main() i32 {
    panic("boom")
}
```

```tam,trap
// panic: assertion failed
fn main() i32 {
    assert(1 + 1 == 3)
    0
}
```

```tam,trap
// panic: called `unwrap()` on a `None` value
fn none() Option<i32> { None }
fn main() i32 { none().unwrap() }
```

### 12.3 스택

스레드도 비동기도 없다. 스택 크기는 운영체제 기본값(Linux 8MiB 등)이며, 무한 재귀로 넘치면 운영체제가 프로세스를 끝낸다(검사는 없다). 4KiB를 넘는 스택 프레임을 가진 함수는 Windows/펌웨어에서 `__chkstk`가 스택을 점검하며 넘기지 않게 한다.

---

## 13. 컴파일 대상

`--target`(또는 `Tmake.toml`의 `[build] target`)으로 고른다.

| 대상 | 산출물 | 비고 |
|---|---|---|
| `host` | 네이티브 실행 파일 | Windows 외 기본값. 시스템 `clang`으로 링크한다 |
| `windows` | x64 `.exe` | Windows 기본값. CRT, MSVC, Windows SDK 없이 `kernel32`만 쓴다. `llc`(없으면 `clang`)와 `lld-link`가 필요 |
| `uefi` | x86-64 `.efi` | `EFI/BOOT/BOOTX64.EFI`로 부팅 |
| `bios` | x86-64 부팅 디스크 `.img` | 부트 섹터 + 프로그램. 페이로드 최대 384KiB |
| `aarch64` | AArch64 `Image` `.bin` | QEMU `virt -kernel`용. EL1, MMU 꺼짐 |

- 대상에 맞지 않는 내장 함수(`mmio_*`, `port_*`, `halt`, `semihost`, `ccall*`, `efi_*`)를 쓰면 위치와 함께 컴파일 오류다.
- `tam.os`의 공개 API는 모든 대상에서 같지만, 펌웨어 대상은 `print`, `println`, `print_int`, `exit`만 제공한다.
- 호스트와 Windows에서 같은 프로그램이 같은 출력을 낸다(에뮬레이터로 검증했다. 실제 Windows 실행은 CI가 확인한다).

---

## 14. 내장 함수

내장 함수는 예약된 이름이며 어떤 `use`도 없이 쓴다.

| 이름 | 시그니처 | 설명 |
|---|---|---|
| `string_from` | `(str) -> String` | 힙에 복사해 새 `String`을 만든다 |
| `string_push` | `(&mut String, u8)` | 한 바이트를 덧붙인다. 용량이 모자라면 2배로 늘린다 |
| `as_str` | `(&String) -> str` | `String`을 빌려 `str`을 만든다 |
| `panic` | `(str) -> !` | 메시지를 출력하고 trap한다 |
| `assert` | `(bool)` | 거짓이면 `assertion failed`로 trap한다 |
| `wrapping_add` `wrapping_sub` `wrapping_mul` | `(T, T) -> T` | 정수형. 감싸는 산술 |
| `alloc` | `unsafe (u64) -> *mut u8` | 힙 블록을 할당한다 |
| `free` | `unsafe (*mut u8)` | `alloc`/`String` 힙 블록을 해제한다 |
| `mmio_read8/16/32/64` | `unsafe (u64) -> uN` | volatile 읽기(펌웨어 전용) |
| `mmio_write8/16/32/64` | `unsafe (u64, uN)` | volatile 쓰기(펌웨어 전용) |
| `port_in8/in32` | `unsafe (u16) -> uN` | x86 포트 읽기(uefi, bios) |
| `port_out8/out32` | `unsafe (u16, uN)` | x86 포트 쓰기(uefi, bios) |
| `halt` | `unsafe ()` | CPU를 멈춘다(펌웨어 전용) |
| `semihost` | `unsafe (u64, u64) -> u64` | AArch64 semihosting 호출(aarch64 전용) |
| `ccall0..ccall4` | `unsafe (*const u8, u64...) -> u64` | 함수 포인터 호출(uefi 전용) |
| `efi_system_table`, `efi_image_handle` | `unsafe () -> *mut u8` | UEFI 인자(uefi 전용) |

할당은 런타임의 `tam_alloc`/`tam_realloc`/`tam_free`를 거친다(호스트는 libc, Windows는 Heap API, 펌웨어는 자체 할당자). 사용자가 allocator를 교체하는 문법은 아직 없다.

---

## 15. 구현 한계

컴파일러가 안전하게 처리하는 범위를 정해 두었다. 넘으면 **크래시가 아니라 위치와 함께 오류**를 낸다.

| 한계 | 값 | 오류 |
|---|---|---|
| 구문 중첩(괄호, 블록, 단항 연산자, 타입) | 200단계 | `nesting is too deep (more than 200 levels)` |
| 한 식의 연산자 사슬(`a + b + c + …`) | 2000개 | `expression has more than 2000 operators in a row` |
| 제네릭 인스턴스 | 5000개, 중첩 깊이 64 | `generic instantiation limit exceeded` |
| 오류 보고 | 첫 번째 오류 하나 | |
| BIOS 이미지 페이로드 | 384KiB | |
| 힙(BIOS/AArch64) | 각 32MiB | 바닥나면 trap |

```tam,fragment
// 긴 식은 문장으로 나눈다. 1900개 항은 허용된다.
fn main() i32 { 0 + 1 + 1 + 1 }
```

---

## 16. 문법 요약(EBNF)

`{ x }`는 0회 이상, `[ x ]`는 선택, `|`는 선택지, 따옴표는 글자 그대로다. `NL`은 문장 끝(줄바꿈 또는 `;`)이다.

```
program     = { item } .
item        = use | struct | impl | function | extern | const .

use         = "use" ident { "." ident } NL .
const       = [ "pub" ] "const" ident ":" type "=" expr NL .
tparams     = "<" ident { "," ident } ">" .
struct      = [ "pub" ] "struct" ident [ tparams ] "{" [ field { ( "," | NL ) field } [ "," ] ] "}" .
field       = ident ":" type .
impl        = "impl" ident [ tparams ] "{" { [ "pub" ] function } "}" .
function    = [ "pub" ] [ "unsafe" ] "fn" ident [ tparams ] "(" [ params ] ")" [ type ] block .
extern      = [ "pub" ] "extern" string "fn" ident "(" [ params ] ")" [ type ] [ "=" string ] .
params      = param { "," param } [ "," ] .
param       = ident ":" type | "&" [ "mut" ] "self" | "self" .

type        = "&" [ "mut" ] type
            | "*" ( "const" | "mut" ) type
            | "[" type [ ";" ( int | ident ) ] "]"
            | ident [ "<" type { "," type } ">" ] .

block       = "{" { stmt NL } [ expr ] "}" .
stmt        = ( "let" | "mut" ) ident [ ":" type ] "=" expr
            | place assign_op expr
            | "return" [ expr ]
            | "break" | "continue"
            | "while" expr block
            | "loop" block
            | "for" ident "in" expr ( ".." | "..=" ) expr block
            | "for" ident "in" expr block
            | expr .
assign_op   = "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" .

expr        = binary .
binary      = unary { binop unary } .          (* 우선순위는 4.1절 *)
unary       = ( "-" | "!" | "~" | "*" | "&" [ "mut" ] ) unary | postfix { "as" type } .
postfix     = primary { "." ident [ "(" args ")" ] | "[" index "]" | "?" } .
index       = expr | [ expr ] ".." [ expr ] .
primary     = int | float | char | string | "true" | "false"
            | ident [ "<" type { "," type } ">" ] [ "::" ident [ "::" ident ] ] [ "(" args ")" | "{" fieldinits "}" ]
            | "(" expr ")"
            | "[" expr { "," expr } [ "," ] "]" | "[" expr ";" ( int | ident ) "]"
            | "if" expr block [ "else" ( block | "if" ... ) ]
            | "match" expr "{" arm { ( "," | NL ) arm } "}"
            | "unsafe" block .
arm         = pattern "=>" ( expr | block ) .
pattern     = ( "Ok" | "Err" | "Some" ) "(" ( ident | "_" ) ")" | "None" .
```

---

## 17. 아직 없는 것

TAM은 작게 시작했다. 다음은 **아직 없다**(없는 것을 쓰면 오류가 난다).

- 트레이트(제네릭의 경계), 메서드 자신의 타입 매개변수, 상수 제네릭(`[T; N]`의 `N`), 연산자 오버로딩, 클로저, 함수 포인터 값
- 열거형(`enum`), 튜플, 별칭 타입, 전역 **변수**/정적 변수(상수는 있다)
- `Vec`과 해시맵 같은 가변 길이 컬렉션(`tam.os`/`tam.fmt` 이외의 표준 라이브러리)
- 레이블 `break`, `break 값`, 문자열·정수 `match`, 팔 전체를 대신하는 `_ =>` 패턴
- `use` 별칭, `pub use`, 필드 단위 `pub`
- 사용자 allocator 교체, 사용자 정의 소멸자, `Box<T>`
- `String` 인덱싱, 슬라이스·배열 비교(`==`), 길이 0 배열, 식으로 된 배열 길이(`N * 2`; 상수 이름은 된다)
- 오류 타입 변환(`From`), `Option`/`Result`의 `map` 등 조합 메서드
- 여러 오류를 한 번에 보고하는 기능(첫 오류만 보고한다)
- 스레드, 비동기, 파일/디렉터리 조작과 명령줄 인자(`tam.os`의 범위: [표준 라이브러리 문서](stdlib.md))

이 목록은 `ROADMAP.md`의 마일스톤에 따라 줄어든다.

## 18. 호환성 약속: TAM 1

`tamc 1.0`은 **TAM 1**의 첫 공식 버전이다. 이 문서가 정의하는 언어를 **에디션 1**이라 부른다.

**약속 (1.x 동안)**

- 에디션 1에서 올바르게 컴파일되던 프로그램은 이후의 모든 `1.x` 컴파일러에서도 같은 의미로 컴파일된다. 종료 코드, 출력, trap 조건(오버플로, 경계, 0 나누기)은 바뀌지 않는다.
- 새 기능은 **추가만** 한다. 17장의 "아직 없는 것"은 채워질 수 있어도 기존 문법의 뜻을 바꾸지 않는다.
- 새 키워드는 에디션 1에서 식별자로 쓰이던 이름을 깨지 않는 방식으로만 들이며, 불가피하면 새 에디션으로 미룬다.
- 컴파일러가 **더 많은 오류를 잡아내는 것**은 버그 수정이다. 이전에 잘못 받아들이던 프로그램(예: 실제로는 메모리 안전하지 않은 코드)은 거절될 수 있다. 이 경우 변경 이력에 적는다.
- 오류 **메시지 문구**와 위치 표시는 약속 대상이 아니다. 도구는 종료 코드로 판단해야 한다.
- 표준 라이브러리(`tam.os`, `tam.fmt`, `tam.serial`)와 내장 함수의 시그니처는 1.x에서 제거·변경하지 않는다.
- `Tmake.toml`의 기존 키와 `Tmake.lock` 형식은 읽을 수 있게 유지한다.

**에디션**

`Tmake.toml`의 `[package]`에 `edition = "1"`을 쓸 수 있다(생략하면 `"1"`). 이 컴파일러가 모르는 에디션이면 `unsupported edition`으로 빌드가 거절된다. 호환되지 않는 변경이 필요해지면 새 에디션(`"2"`)으로만 도입하고, 컴파일러는 에디션 1 패키지를 계속 빌드한다. 한 프로젝트 안에서 패키지마다 에디션이 달라도 된다.

**약속하지 않는 것**

- "정해지지 않았다"고 적은 동작, 구현 한계의 정확한 수치(15장), 코드 생성 결과(LLVM IR, 어셈블리), 단형화 인스턴스의 내부 이름, `tamc` 내부 옵션 중 문서에 없는 것.
- 제네릭의 단형화 방식은 유지되며, 이후 트레이트 경계가 **추가**되더라도 경계 없는 기존 제네릭 코드는 계속 컴파일된다.
