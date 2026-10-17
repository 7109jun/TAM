# 4. 소유권

소유권은 TAM의 중심 개념이다. 가비지 컬렉터 없이도 메모리를 **누수 없이, 이중 해제 없이, 해제된 메모리를 쓰지 않고** 다루게 해 준다. 규칙은 짧다.

1. 모든 값에는 **소유자**(변수)가 하나 있다.
2. 소유자가 스코프를 벗어나면 값이 **자동으로 해제**된다.
3. 값을 다른 변수에 넘기면 소유권이 **이동**한다. 이전 소유자는 더 쓸 수 없다.

이 검사는 전부 **컴파일 시간**에 일어난다. 실행 시간 비용은 없다(조건부 이동을 위한 작은 플래그 하나 정도다).

## 4.1 스택 값과 힙 값

정수, `f64`, `bool`, `str`, 참조는 **Copy**다. 대입하면 복사되고 둘 다 계속 쓸 수 있다.

```tam
// exit: 10
fn main() i32 {
    let a = 5
    let b = a        // 복사
    a + b
}
```

힙을 소유하는 `String`, 그리고 **구조체와 배열**은 **이동 타입**이다.

```tam
// exit: 3
fn main() i32 {
    let s = string_from("tam")
    let t = s              // s의 소유권이 t로 이동
    t.len as i32
}
```

이동한 뒤에 `s`를 쓰면 컴파일 오류다.

```tam,error
// error: use of moved value `s`
fn main() i32 {
    let s = string_from("tam")
    let t = s
    s.len as i32
}
```

```console
error: use of moved value `s`
 --> ex1.tam:4:5
  |
3 |     let t = s
  |             - value moved here
4 |     s.len as i32
  |     ^^^^^ value used here after move
```

오류는 **이동한 위치**와 **다시 쓴 위치**를 함께 보여 준다.

> 구조체와 배열은 필드가 모두 정수여도 이동 타입이다. 복사하려면 필드를 읽어 새 값을 만든다. (Copy 구조체 표기는 아직 없다.)

## 4.2 `String`

`String`은 힙의 가변 길이 문자열(포인터, 길이, 용량)이다.

| 함수 | 설명 |
|---|---|
| `string_from(str) String` | 리터럴(`str`)을 힙에 복사해 `String`을 만든다 |
| `string_push(&mut String, u8)` | 바이트 하나를 덧붙인다 |
| `as_str(&String) str` | 읽기용 `str`을 빌린다(복사 없음) |
| `s.len` | 길이(`u64`) |

```tam
// exit: 5
fn main() i32 {
    mut s = string_from("tam")
    string_push(&mut s, 33)         // '!'
    string_push(&mut s, 33)
    s.len as i32
}
```

이어 붙이기와 정수 → 문자열은 표준 라이브러리 `tam.fmt`에 있다([Standard Library](../stdlib.md#4-tamfmt)).

### 복사하고 싶을 때

`String`을 복제하는 내장 함수는 없다. `str`로 빌려서 새로 만든다.

```tam
// exit: 6
fn main() i32 {
    let a = string_from("abc")
    let b = string_from(as_str(&a))   // 힙에 따로 복사
    a.len as i32 + b.len as i32       // 둘 다 살아 있다
}
```

## 4.3 자동 해제

값은 소유자가 스코프를 벗어날 때 **선언의 역순으로** 해제된다. 직접 `free`를 부르지 않는다. 함수가 끝날 때, `return`할 때, `?`로 일찍 빠질 때, 루프에서 `break`/`continue`할 때 모두 해제된다.

```tam
// exit: 0
fn work() i32 {
    let a = string_from("first")
    let b = string_from("second")
    if a.len > 100 { return 1 }    // 이 return에서도 a, b는 해제된다
    0
}                                   // 정상 종료: b, a 순으로 해제

fn main() i32 { work() }
```

valgrind에서 이런 프로그램을 돌리면 누수와 이중 해제가 **0건**이다. TAM의 테스트는 이를 자동으로 확인한다.

**이동된 값은 해제되지 않는다**(새 소유자가 해제한다). 이동 여부는 변수마다 둔 플래그로 판단하므로 **조건부 이동**도 정확하다.

```tam
// exit: 4
fn consume(s: String) i32 { s.len as i32 }

fn main() i32 {
    let s = string_from("abcd")
    if s.len > 2 {
        return consume(s)     // 이 가지에서만 이동. 다른 경로에서는 main이 해제한다
    }
    0
}
```

## 4.4 함수와 소유권

값을 인자로 넘기면 이동한다. 돌려받으려면 반환한다.

**매개변수는 불변**이므로 값을 바꾸려면 본문에서 `mut`으로 다시 묶는다(소유권이 새 변수로 이동한다).

```tam
// exit: 6
fn shout(s: String) String {
    mut t = s                       // 소유권이 t로 이동
    string_push(&mut t, 33)
    t
}

fn main() i32 {
    let a = string_from("hey")
    let b = shout(a)                // a는 이동, 결과는 b가 소유
    let c = shout(b)
    c.len as i32 + 1
}
```

값을 넘기고 돌려받는 일은 번거롭다. 그래서 **빌림**이 있다. 다음 장에서 본다.

## 4.5 구조체 안의 `String`

구조체가 `String`을 가지면 구조체도 자동으로 해제된다(필드도 함께). 구조체 전체를 이동하면 안의 값도 함께 이동한다.

```tam
// exit: 3
struct User {
    name: String,
    age: i32,
}

fn age_plus_len(u: User) i32 {
    u.age + u.name.len as i32     // u는 이 함수가 끝날 때 해제된다
}

fn main() i32 {
    let u = User { name: string_from("ab"), age: 1 }
    age_plus_len(u)
}
```

**힙을 쥔 필드 하나만 꺼내 이동하는 것은 안 된다.** 구조체가 부분적으로 비어 버리는 일을 막기 위해서다.

```tam,error
// error: cannot move
struct User { name: String, age: i32 }

fn main() i32 {
    let u = User { name: string_from("ab"), age: 1 }
    let n = u.name
    0
}
```

구조체 전체를 옮기거나, 필드를 **빌려서**(다음 장) 쓴다.

## 4.6 루프와 이동

루프 안에서 바깥 값을 이동하면, 다음 반복에서 이미 이동한 값을 또 쓰게 되므로 오류다.

```tam,error
// error: use of moved value `s`
fn take(s: String) {}

fn main() i32 {
    let s = string_from("x")
    mut i = 0
    while i < 3 {
        take(s)
        i += 1
    }
    0
}
```

이동한 직후에 `break`하면 괜찮다(그 경로는 다음 반복이 없다).

```tam
// exit: 0
fn take(s: String) {}

fn main() i32 {
    let s = string_from("x")
    mut i = 0
    while i < 3 {
        take(s)
        break
    }
    0
}
```

## 4.7 재대입

`String` 변수에 새 값을 대입하면 **이전 값이 먼저 해제**된다.

```tam
// exit: 4
fn main() i32 {
    mut s = string_from("old value")
    s = string_from("new!")        // "old value"는 여기서 해제된다
    s.len as i32
}
```

## 4.8 `os::exit`와 trap

`os::exit`나 trap(패닉)은 프로세스를 즉시 끝내므로 **살아 있는 값의 해제를 실행하지 않는다.** 운영체제가 메모리를 회수하므로 누수 문제는 아니지만, 해제 시점에 해야 할 작업(예: 파일 닫기)은 직접 해야 한다.

## 4.9 정리

| 규칙 | 의미 |
|---|---|
| 소유자 하나 | 모든 값에는 소유 변수가 하나 |
| 이동 | 대입, 인자 전달, 반환, 구조체 초기화는 소유권을 옮긴다(Copy 타입 제외) |
| 이동 후 사용 금지 | 컴파일 오류(`use of moved value`) |
| 스코프 끝 = 해제 | 선언 역순, 조기 탈출 포함 |
| 부분 이동 금지 | 힙을 쥔 필드 하나만 꺼낼 수 없다 |

→ [5. 참조와 빌림](05-references-and-borrowing.md)
