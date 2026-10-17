# 14. 더 나아가기

## 14.1 연습 문제

문제를 먼저 직접 풀어 보고 해답을 본다. 해답은 모두 컴파일·실행으로 검증되어 있다.

### 문제 1. 최대공약수

`gcd(a, b)`를 `while`로 구현하라. `gcd(48, 18)`은 6이다.

```tam
// exit: 6
fn gcd(a: i64, b: i64) i64 {
    mut x = a
    mut y = b
    while y != 0 {
        let t = x % y
        x = y
        y = t
    }
    x
}

fn main() i32 { gcd(48, 18) as i32 }
```

### 문제 2. 배열 뒤집기

`&mut [i32]`를 받아 제자리에서 뒤집어라. 서로 다른 두 원소를 동시에 `&mut`로 빌릴 수 없으니 값으로 바꿔치기한다.

```tam
// exit: 0
fn reverse(a: &mut [i32]) {
    if a.len < 2 { return }
    mut i: u64 = 0
    mut j: u64 = a.len - 1
    while i < j {
        let t = a[i]
        a[i] = a[j]
        a[j] = t
        i += 1
        j -= 1
    }
}

fn main() i32 {
    mut a = [1, 2, 3, 4, 5]
    reverse(&mut a[..])
    if a[0] != 5 || a[4] != 1 || a[2] != 3 { return 1 }
    0
}
```

### 문제 3. 소수 거르기

2부터 50까지 소수의 개수를 세어라(답 15).

```tam
// exit: 15
fn main() i32 {
    mut sieve = [true; 51]
    mut count = 0
    for i in 2..51 {
        if sieve[i] {
            count += 1
            mut j = i * 2
            while j < 51 {
                sieve[j] = false
                j += i
            }
        }
    }
    count
}
```

### 문제 4. 문자열 만들기

`String`에 `0`부터 `4`까지의 숫자 글자를 이어 붙이고 길이를 반환하라.

```tam
// exit: 5
fn main() i32 {
    mut s = string_from("")
    for i in 0..5 {
        string_push(&mut s, 48 + i as u8)
    }
    s.len as i32
}
```

### 문제 5. 오류를 값으로

안전한 나눗셈 `div(a, b)`가 0으로 나누면 `Err(1)`을 돌려주게 하고, 호출자 `ratio`가 `?`로 전파하게 하라.

```tam
// exit: 7
fn div(a: i32, b: i32) Result<i32, i32> {
    if b == 0 { return Err(1) }
    Ok(a / b)
}

fn ratio(a: i32, b: i32) Result<i32, i32> {
    let q = div(a, b)?
    Ok(q + 1)
}

fn main() i32 {
    let ok = ratio(12, 2).unwrap_or(0)        // 7
    let bad = ratio(1, 0).unwrap_or(0)        // 0
    ok + bad
}
```

## 14.2 자주 만나는 컴파일 오류

| 메시지(일부) | 원인 | 고치는 법 |
|---|---|---|
| `use of moved value` | 이동한 뒤에 다시 썼다 | 참조(`&`)로 넘긴다. 필요한 값은 이동 전에 꺼낸다 |
| `cannot assign to ... because it is borrowed` | 참조가 살아 있는 동안 수정했다 | 참조의 마지막 사용을 수정 앞으로 옮긴다 |
| `cannot borrow ... as mutable more than once` | `&mut`을 둘 만들었다 | 하나를 다 쓴 뒤에 다음을 만든다 |
| `cannot return reference to local variable` | 지역 변수의 참조를 반환했다 | 값을 반환하거나 호출자의 값을 참조로 받는다 |
| `needs an `unsafe` block` | 포인터 역참조, `alloc` 등 | 꼭 필요하면 최소 범위로 `unsafe` |
| `match` is missing the ... arm | 팔 하나가 빠졌다 | 두 팔을 모두 쓴다 |
| `index out of bounds` | 상수 인덱스가 범위 밖이다 | 길이를 확인한다 |
| `expected `i64`, found `i32`` | 정수형이 다르다 | `as`로 명시적으로 변환한다 |

TAM은 **암묵적 변환이 없다**. 형 불일치는 거의 항상 `as`로 풀린다.

## 14.3 실행 중 멈출 때

컴파일을 통과한 프로그램이 `tam: panic: ...`을 내고 멈추면 아래 중 하나다.

| 메시지 | 원인 |
|---|---|
| `integer overflow` | 산술이 범위를 넘었다. 의도한 것이면 `wrapping_*` |
| `division by zero` | 0으로 나눴다 |
| `index out of bounds` | 인덱스가 길이 이상(또는 음수)이다 |
| `called `unwrap()` on ...` | `None`/`Err`에 `unwrap` |
| `assertion failed` | `assert`가 거짓이다 |

`tamc run`은 종료 코드를 `128 + 시그널`로 보고한다. 조용히 잘못된 결과를 내는 대신 **즉시, 이유와 함께 멈추는 것**이 설계다.

## 14.4 다음에 읽을 것

| 문서 | 언제 |
|---|---|
| [TAM by Example](../by-example/README.md) | 작은 예제를 빠르게 훑고 싶을 때 |
| [The TAM Reference](../reference.md) | 문법과 의미를 정확히 확인할 때 |
| [표준 라이브러리 API](../stdlib.md) | `os`, `fmt`, `serial`의 함수를 찾을 때 |
| [Tmake.toml 문서](../tmake.md) | 의존성, 워크스페이스, 프로필, 대상 설정 |

그리고 아직 없는 것(트레이트, 열거형, 클로저, 가변 길이 컬렉션 등)은 [레퍼런스 17장](../reference.md#17-아직-없는-것)에 정리되어 있다. 지금의 TAM으로도 `alloc`과 구조체를 이용해 직접 `Vec`을 만들 수 있다. 제네릭([13장](13-generics.md))을 배웠으니 `Vec<T>`를 직접 만들어 보는 것이 다음 연습으로 좋다.
