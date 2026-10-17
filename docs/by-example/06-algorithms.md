# 6. 알고리즘

## 최대공약수

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

## 피보나치(반복)

```tam
// exit: 55
fn fib(n: i32) i64 {
    mut a: i64 = 0
    mut b: i64 = 1
    for _i in 0..n {
        let t = a + b
        a = b
        b = t
    }
    a
}

fn main() i32 { fib(10) as i32 }
```

## 소수 체

```tam
// exit: 25
fn main() i32 {
    mut sieve = [true; 101]
    mut count = 0
    for i in 2..101 {
        if sieve[i] {
            count += 1
            mut j = i * i
            while j < 101 {
                sieve[j] = false
                j += i
            }
        }
    }
    count                      // 100 이하 소수는 25개
}
```

## 삽입 정렬

```tam
// exit: 0
fn sort(a: &mut [i32]) {
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
    mut a = [5, 2, 9, 1, 7, 3]
    sort(&mut a[..])
    for i in 1..6 {
        if a[i - 1] > a[i] { return 1 }
    }
    0
}
```

## 이진 탐색

```tam
// exit: 4
fn search(a: &[i32], x: i32) Option<u64> {
    mut lo: u64 = 0
    mut hi: u64 = a.len
    while lo < hi {
        let mid = lo + (hi - lo) / 2
        if a[mid] == x { return Some(mid) }
        if a[mid] < x { lo = mid + 1 } else { hi = mid }
    }
    None
}

fn main() i32 {
    let a = [1, 3, 5, 7, 9, 11]
    match search(&a, 9) {
        Some(i) => i as i32
        None => 0 - 1
    }
}
```

## 배열 뒤집기

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
    mut a = [1, 2, 3, 4]
    reverse(&mut a[..])
    if a[0] == 4 && a[3] == 1 { 0 } else { 1 }
}
```

## 최대값과 최소값

```tam
// exit: 8
fn range(a: &[i32]) i32 {
    mut lo = a[0]
    mut hi = a[0]
    for x in a {
        if x < lo { lo = x }
        if x > hi { hi = x }
    }
    hi - lo
}

fn main() i32 {
    let a = [3, 9, 1, 4]
    range(&a)
}
```

## 거듭제곱(제곱하며 곱하기)

```tam
// exit: 243
fn pow(base: i64, exp: u32) i64 {
    mut result: i64 = 1
    mut b = base
    mut e = exp
    while e > 0 {
        if e & 1 == 1 { result *= b }
        e >>= 1
        if e > 0 { b *= b }
    }
    result
}

fn main() i32 { pow(3, 5) as i32 }
```

## 팰린드롬 검사

```tam
// exit: 1
fn at(s: str, i: u64) u8 { unsafe { *(s.ptr + i) } }

fn palindrome(s: str) bool {
    if s.len < 2 { return true }
    mut i: u64 = 0
    mut j: u64 = s.len - 1
    while i < j {
        if at(s, i) != at(s, j) { return false }
        i += 1
        j -= 1
    }
    true
}

fn main() i32 {
    if palindrome("level") && !palindrome("tam") { 1 } else { 0 }
}
```

## 콜라츠 단계 수

```tam
// exit: 111
fn steps(n: u64) i32 {
    mut x = n
    mut c = 0
    while x != 1 {
        if x % 2 == 0 { x /= 2 } else { x = 3 * x + 1 }
        c += 1
    }
    c
}

fn main() i32 { steps(27) }
```
