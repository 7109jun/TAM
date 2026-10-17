# 5. 참조와 빌림

값을 넘길 때마다 소유권을 옮기면 불편하다. **참조**는 소유권을 옮기지 않고 값을 **빌려** 준다. 빌림 검사기(borrow checker)는 빌린 값이 도중에 사라지거나 위험하게 바뀌지 않음을 컴파일 시간에 증명한다.

## 5.1 읽기 참조 `&T`

`&x`는 `x`를 읽기 전용으로 빌린다. `*r`로 값을 읽는다.

```tam
// exit: 5
fn len_of(s: &String) i32 {
    s.len as i32          // 참조 너머의 필드도 바로 읽는다
}

fn main() i32 {
    let s = string_from("hello")
    let n = len_of(&s)     // 빌려 주기만 한다
    n + 0 * s.len as i32   // s는 여전히 쓸 수 있다
}
```

```tam
// exit: 6
fn double(r: &i32) i32 { *r * 2 }

fn main() i32 {
    let x = 3
    double(&x)
}
```

참조는 Copy다. 읽기 참조는 **여러 개** 동시에 있을 수 있다.

## 5.2 쓰기 참조 `&mut T`

`&mut x`는 `x`를 **배타적으로** 빌려 바꿀 수 있게 한다. `x`가 `mut`여야 한다.

```tam
// exit: 11
fn bump(x: &mut i32) {
    *x += 1
}

fn main() i32 {
    mut n = 10
    bump(&mut n)
    n
}
```

```tam,error
// error: cannot borrow immutable variable `a` as mutable
fn main() i32 {
    let a = 5
    let r = &mut a
    0
}
```

## 5.3 빌림 규칙

> 같은 값에 대해 **읽기 참조 여러 개** 또는 **쓰기 참조 하나**만 동시에 있을 수 있다. 참조는 가리키는 값보다 오래 살 수 없다.

이 규칙이 데이터 레이스, 반복 중 컬렉션 수정, 해제 후 사용 같은 버그를 **원천 봉쇄**한다.

### 읽는 동안에는 바꿀 수 없다

```tam,error
// error: cannot assign to `v` because it is borrowed
fn main() i32 {
    mut v = 10
    let r = &v
    v += 1
    *r
}
```

```console
error: cannot assign to `v` because it is borrowed
 --> ex2.tam:4:5
  |
3 |     let r = &v
  |             -- `v` is borrowed here
4 |     v += 1
  |     ^ assignment to borrowed `v` occurs here
5 |     *r
  |     -- first borrow later used here
```

오류는 **세 군데**를 가리킨다: 빌린 곳, 충돌한 곳, 빌림이 다시 쓰이는 곳. 이 셋을 읽으면 무엇을 고쳐야 하는지 보인다. 보통 참조의 사용을 충돌 앞으로 옮기거나, 충돌을 참조 사용 뒤로 옮긴다.

### 쓰기 참조는 하나만

```tam,error
// error: cannot borrow `s` as mutable more than once at a time
fn main() i32 {
    mut s = string_from("a")
    let r = &mut s
    let q = &mut s
    string_push(r, 98)
    0
}
```

### 읽기와 쓰기를 섞을 수 없다

```tam,error
// error: cannot borrow `s` as mutable because it is also borrowed as immutable
fn main() i32 {
    mut s = string_from("a")
    let view = as_str(&s)
    string_push(&mut s, 98)
    view.len as i32
}
```

`as_str(&s)`가 돌려준 `str`은 `s`를 **빌린** 것이다. `s`를 바꾸면 `view`가 가리키는 메모리가 바뀌거나 옮겨질 수 있으니 막는다.

## 5.4 "마지막 사용까지": NLL

참조가 살아 있는 구간은 **마지막으로 쓰는 곳까지**다. 스코프 끝까지가 아니다. 참조를 더 쓰지 않으면 이어지는 수정은 허용된다.

```tam
// exit: 11
fn main() i32 {
    mut v = 10
    let r = &v
    let seen = *r        // r의 마지막 사용
    v += 1               // 이제 괜찮다
    v + seen - 10
}
```

## 5.5 구조체의 필드는 따로 빌릴 수 있다

```tam
// exit: 5
struct P { x: i32, y: i32 }

fn main() i32 {
    mut p = P { x: 1, y: 2 }
    let a = &mut p.x
    let b = &mut p.y          // 다른 필드라 동시에 가능
    *a += 1
    *b += 1
    p.x + p.y
}
```

구조체 **전체**와 그 필드는 서로 충돌한다. **배열은 원소를 구분하지 않는다**: `&mut a[0]`과 `&a[1]`은 동시에 만들 수 없다(보수적이다).

## 5.6 반환하는 참조와 댕글링

함수가 참조를 돌려줄 때, 그 참조는 **호출자가 가진 값**을 가리켜야 한다. 지역 변수를 가리키는 참조는 함수가 끝나면 사라지므로 컴파일러가 막는다.

```tam,error
// error: cannot return reference to local variable `x`
fn bad() &i32 {
    let x = 5
    &x
}

fn main() i32 { *bad() }
```

인자로 받은 참조를 돌려주는 것은 괜찮다. 수명 표기는 필요 없다. 컴파일러가 본문에서 출처를 **추론**한다.

```tam
// exit: 7
fn pick(a: &i32, b: &i32) &i32 {
    if *a > *b { a } else { b }
}

fn main() i32 {
    let x = 3
    let y = 7
    *pick(&x, &y)
}
```

여러 인자 중 어느 것이 나올지 모르면 컴파일러는 **보수적으로 모두를 빌린 것**으로 본다. 그래서 위의 `pick`의 결과가 살아 있는 동안 `x`와 `y` 둘 다 바꿀 수 없다.

```tam,error
// error: cannot assign to `x` because it is borrowed
fn pick(a: &i32, b: &i32) &i32 { a }

fn main() i32 {
    mut x = 1
    let y = 2
    let r = pick(&x, &y)
    x += 1
    *r
}
```

## 5.7 `&mut`의 재차용

`&mut` 참조를 다른 함수에 넘기면 **잠시 빌려 주는 것**(재차용)이며, 그 함수가 끝나면 원래 참조를 다시 쓸 수 있다.

```tam
// exit: 3
fn inc(x: &mut i32) { *x += 1 }

fn twice(x: &mut i32) {
    inc(x)
    inc(x)
}

fn main() i32 {
    mut n = 1
    twice(&mut n)
    n
}
```

`&mut T`는 `&T`가 필요한 자리에 넘길 수 있다(읽기만 하게 된다). 반대는 안 된다.

## 5.8 참조의 참조

```tam
// exit: 5
fn deref2(r: &&i32) i32 { **r }

fn main() i32 {
    let x = 5
    let r = &x
    deref2(&r)
}
```

## 5.9 오류를 고치는 요령

| 오류 | 보통의 해결 |
|---|---|
| `use of moved value` | 값 대신 **참조**를 넘긴다. 또는 이동한 뒤에는 쓰지 않는다 |
| `cannot assign to ... because it is borrowed` | 참조의 마지막 사용을 수정 앞으로 당긴다 |
| `cannot borrow ... as mutable more than once` | 먼저 만든 `&mut`을 다 쓴 뒤에 다음 것을 만든다 |
| `cannot borrow ... as mutable because it is also borrowed as immutable` | 읽기 참조(`as_str` 결과 포함)의 사용을 끝낸 뒤에 수정한다 |
| `cannot return reference to local variable` | 값을 **반환**(이동)하거나, 호출자가 만든 값을 참조로 받는다 |

검사기가 지나치게 보수적이라 올바른 코드를 거절하는 경우도 있다(예: 배열의 서로 다른 원소 둘을 동시에 `&mut`로). 그때는 인덱스로 한 번에 하나씩 접근하거나, 값을 임시 변수에 복사해 쓴다. 정말 필요한 때에는 `unsafe`의 raw 포인터가 있다([10장](10-unsafe-and-systems.md)).

→ [6. 구조체와 메서드](06-structs-and-methods.md)
