# 7. 배열과 슬라이스

## 7.1 배열 `[T; N]`

고정 길이 배열이다. 길이는 **1 이상의 정수 리터럴**이고 타입의 일부다.

```tam
// exit: 6
fn main() i32 {
    let a = [1, 2, 3]            // [i32; 3]
    let z = [0; 4]               // 0이 4개. [i32; 4]
    a[0] + a[1] + a[2] + z[3]
}
```

타입을 쓰려면 `[원소; 길이]`다.

```tam
// exit: 0
fn main() i32 {
    let bytes: [u8; 4] = [0; 4]
    let grid: [[i32; 3]; 2] = [[0; 3]; 2]
    (bytes[3] as i32) + grid[1][2]
}
```

- 원소를 쓰려면 `mut`로 선언한다: `mut a = [0; 8]`, `a[i] = v`.
- 인덱스는 **어떤 정수형이든** 된다.
- 원소 타입은 정수, `f64`, `bool`, 포인터, 그것들의 배열, 그것들만 가진 구조체다. `String`이나 참조의 배열은 안 된다.
- 길이는 `.len`(`u64`)이다.

### 경계 검사

범위를 벗어난 인덱스는 **실행 시간에 trap**한다. 길이와 인덱스가 둘 다 상수이면 **컴파일 오류**다.

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

음수 인덱스(부호 있는 정수)도 trap이다. 경계를 넘는 읽기·쓰기는 **절대로 조용히 일어나지 않는다.**

### 배열은 이동 타입이다

값 전달과 대입은 이동(복사 비용이 큰 LLVM 집합체 복사)이다. 큰 배열은 참조나 슬라이스로 넘긴다.

```tam,error
// error: use of moved value `a`
fn main() i32 {
    let a = [1, 2]
    let b = a
    a[0] + b[0]
}
```

## 7.2 슬라이스 `&[T]`

**슬라이스**는 배열의 일부를 가리키는 참조(포인터 + 길이)다. 배열 크기에 무관한 함수를 쓰게 해 준다.

```tam
// exit: 15
fn sum(a: &[i32]) i32 {
    mut s = 0
    for i in 0..a.len { s += a[i] }
    s
}

fn main() i32 {
    let a = [1, 2, 3, 4, 5]
    sum(&a)                    // &배열 → &[i32] 자동 변환(함수 인자 자리)
}
```

슬라이스 만들기:

| 식 | 의미 |
|---|---|
| `&a[..]` | 전체 |
| `&a[lo..hi]` | `lo`부터 `hi` 직전까지 |
| `&a[lo..]`, `&a[..hi]` | 한쪽 끝까지 |
| `&mut a[..]` | 쓰기 슬라이스 |

```tam
// exit: 9
fn main() i32 {
    let a = [10, 20, 30, 40, 50]
    let mid = &a[1..4]            // [20, 30, 40]
    mid.len as i32 * 3            // 길이는 3
}
```

범위가 배열을 넘거나 `lo > hi`이면 trap한다.

### 쓰기 슬라이스

```tam
// exit: 9
fn fill(a: &mut [i32], v: i32) {
    for i in 0..a.len { a[i] = v }
}

fn main() i32 {
    mut a = [1, 2, 3, 4, 5]
    fill(&mut a[1..3], 9)         // 가운데 두 개만 9로
    a[1] + a[2] - 9
}
```

### `for`로 순회

```tam
// exit: 6
fn main() i32 {
    let a = [1, 2, 3]
    mut s = 0
    for x in &a[..] { s += x }    // x는 원소의 복사본(값)
    s
}
```

원소를 바꾸려면 인덱스로 순회한다.

```tam
// exit: 12
fn main() i32 {
    mut a = [1, 2, 3]
    for i in 0..a.len { a[i] *= 2 }
    a[0] + a[1] + a[2]
}
```

## 7.3 빌림 규칙은 슬라이스에도

```tam,error
// error: cannot assign to `a[..]` because it is borrowed
fn main() i32 {
    mut a = [1, 2, 3]
    let s = &a[..]
    a[0] = 9
    s[0]
}
```

지역 배열의 슬라이스를 반환할 수도 없다(댕글링). 배열 원소는 **구분하지 않으므로** `&mut a[0]`과 `&mut a[1]`을 동시에 만들 수 없다. 두 원소를 바꾸려면 하나씩 차례로 한다.

## 7.4 예: 삽입 정렬

```tam
// exit: 1
fn sort(a: &mut [i64]) {
    mut i: u64 = 1
    while i < a.len {
        let key = a[i]
        mut j = i
        while j > 0 && a[j - 1] > key {
            a[j] = a[j - 1]
            j -= 1
        }
        a[j] = key
        i += 1
    }
}

fn main() i32 {
    mut a: [i64; 6] = [5, 2, 9, 1, 7, 3]
    sort(&mut a[..])
    mut ok = 1
    for i in 1..6 { if a[i - 1] > a[i] { ok = 0 } }
    ok
}
```

## 7.5 문자열과 바이트

`str`과 `String`은 바이트 열이지만 인덱싱(`s[i]`)이 없다. 바이트를 읽으려면 `s.ptr`와 `unsafe`가 필요하다([10장](10-unsafe-and-systems.md)).

```tam
// exit: 2
fn count_byte(s: str, b: u8) i32 {
    mut n = 0
    for i in 0..s.len {
        if unsafe { *(s.ptr + i) } == b { n += 1 }
    }
    n
}

fn main() i32 {
    count_byte("hello", 108)       // 'l'이 두 번
}
```

## 7.5.1 한계

- 슬라이스·배열의 `==` 비교는 없다. 원소를 하나씩 비교한다.
- 길이 0 배열과 상수 식 길이(`[T; N * 2]`)는 없다.
- 가변 길이 컬렉션(`Vec`)은 아직 없다. `alloc`으로 직접 만들 수 있다([10장](10-unsafe-and-systems.md)).

→ [8. 오류 처리](08-error-handling.md)
