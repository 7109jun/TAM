# 3. 소유권과 빌림

## 이동

`String`, 구조체, 배열은 이동 타입이다. 이동한 뒤의 원본은 쓸 수 없다.

```tam,error
// error: use of moved value `a`
fn main() i32 {
    let a = string_from("hi")
    let b = a
    a.len as i32 + b.len as i32
}
```

## 함수에 값을 넘기면 이동한다

```tam
// exit: 3
fn consume(s: String) i32 { s.len as i32 }     // 여기서 해제된다

fn main() i32 {
    let s = string_from("abc")
    consume(s)
}
```

## 참조로 빌려 주기

```tam
// exit: 6
fn len_of(s: &String) i32 { s.len as i32 }

fn main() i32 {
    let s = string_from("abc")
    let n = len_of(&s)
    n + s.len as i32          // s는 아직 우리 것이다
}
```

## `&mut`로 바꾸기

```tam
// exit: 3
fn push_twice(s: &mut String) {
    string_push(s, 120)
    string_push(s, 121)
}

fn main() i32 {
    mut s = string_from("a")
    push_twice(&mut s)
    s.len as i32
}
```

## 값 반환으로 소유권 돌려주기

```tam
// exit: 4
fn exclaim(s: String) String {
    mut t = s
    string_push(&mut t, 33)
    t
}

fn main() i32 {
    let s = exclaim(string_from("abc"))
    s.len as i32
}
```

## 읽는 동안 수정은 막힌다

```tam,error
// error: cannot assign to `v` because it is borrowed
fn main() i32 {
    mut v = 1
    let r = &v
    v = 2
    *r
}
```

## 한 번에 하나의 `&mut`

```tam,error
// error: cannot borrow `x` as mutable more than once at a time
fn main() i32 {
    mut x = 1
    let a = &mut x
    let b = &mut x
    *a += 1
    *b += 1
    x
}
```

## 마지막 사용 이후에는 다시 쓸 수 있다

```tam
// exit: 3
fn main() i32 {
    mut x = 1
    let r = &mut x
    *r += 1               // r의 마지막 사용
    x += 1                // 이제 괜찮다
    x
}
```

## 필드는 따로 빌린다

```tam
// exit: 5
struct P { a: i32, b: i32 }

fn main() i32 {
    mut p = P { a: 1, b: 2 }
    let x = &mut p.a
    let y = &mut p.b
    *x += 1
    *y += 1
    p.a + p.b
}
```

## 지역 변수의 참조는 반환할 수 없다

```tam,error
// error: cannot return reference to local variable `x`
fn bad() &i32 {
    let x = 1
    &x
}

fn main() i32 { *bad() }
```

## 자동 해제: 스코프를 벗어날 때

별도로 `free`를 부르지 않아도 `String`은 스코프 끝에서 해제된다. 조기 `return`과 `?`에서도 마찬가지다.

```tam
// exit: 0
fn work(n: i32) i32 {
    let a = string_from("one")
    if n > 0 { return 1 }          // a가 해제된다
    let b = string_from("two")
    a.len as i32 + b.len as i32 - 6
}

fn main() i32 {
    work(1) - 1 + work(0)
}
```
