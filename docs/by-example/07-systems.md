# 7. 시스템

## raw 포인터 읽고 쓰기

```tam
// exit: 8
fn main() i32 {
    mut x = 5
    let p = &mut x as *mut i32
    unsafe { *p = 8 }
    x
}
```

## 직접 할당과 해제

```tam
// exit: 10
fn main() i32 {
    let raw = unsafe { alloc(16) }
    let p = raw as *mut i32
    let r = unsafe {
        *p = 4
        *(p + 1) = 6
        let s = *p + *(p + 1)
        free(raw)
        s
    }
    r
}
```

## 포인터 산술은 원소 단위

```tam
// exit: 30
fn main() i32 {
    let raw = unsafe { alloc(24) }
    let p = raw as *mut i64
    unsafe {
        for i in 0..3 { *(p + i) = (i as i64 + 1) * 5 }
    }
    let r = unsafe { *(p + 1) + *(p + 2) + *p }       // 10 + 15 + 5
    unsafe { free(raw) }
    r as i32
}
```

## unsafe를 안전한 API로 감싸기

```tam
// exit: 9
struct Buf { ptr: *mut u8, len: u64 }

impl Buf {
    fn new(len: u64) Self {
        let p = unsafe { alloc(len) }
        for i in 0..len { unsafe { *(p + i) = 0 } }
        Buf { ptr: p, len: len }
    }
    fn set(&mut self, i: u64, v: u8) {
        assert(i < self.len)
        unsafe { *(self.ptr + i) = v }
    }
    fn get(&self, i: u64) u8 {
        assert(i < self.len)
        unsafe { *(self.ptr + i) }
    }
    fn release(self) { unsafe { free(self.ptr) } }
}

fn main() i32 {
    mut b = Buf::new(8)
    b.set(2, 9)
    let v = b.get(2)
    b.release()
    v as i32
}
```

## 경계를 넘으면 멈춘다

```tam,trap
// panic: assertion failed
struct Buf { len: u64 }

fn check(b: &Buf, i: u64) {
    assert(i < b.len)
}

fn main() i32 {
    let b = Buf { len: 4 }
    check(&b, 4)
    0
}
```

## C 함수 부르기

```tam
// exit: 7
extern "c" fn abs(x: i32) i32

fn main() i32 {
    unsafe { abs(0 - 7) }
}
```

## 널 종료 문자열을 C에 넘기기

```tam
// exit: 4
extern "c" fn strlen(s: *const u8) u64

fn main() i32 {
    let s: [u8; 5] = [116, 97, 109, 99, 0]      // "tamc\0"
    unsafe { strlen(&s as *const [u8; 5] as *const u8) as i32 }
}
```

## 감싸는 산술(해시)

```tam
// exit: 0
fn hash(s: str) u32 {
    mut h: u32 = 2166136261
    for i in 0..s.len {
        let b = unsafe { *(s.ptr + i) } as u32
        h = wrapping_mul(h ^ b, 16777619)          // FNV-1a
    }
    h
}

fn main() i32 {
    if hash("") != 2166136261 { return 1 }
    if hash("a") != 3826002220 { return 2 }
    0
}
```

## `unsafe fn`

```tam
// exit: 3
unsafe fn third(p: *const i32) i32 {
    *(p + 2)                 // 호출자가 최소 3개 원소를 보장한다
}

fn main() i32 {
    let a = [1, 2, 3]
    unsafe { third(&a as *const [i32; 3] as *const i32) }
}
```

## 베어메탈: UART에 직접 쓰기(AArch64)

```tam,boot=aarch64
// out: tam
fn put(c: u8) { unsafe { mmio_write8(150994944, c) } }   // 0x09000000

fn main() i32 {
    put(116)
    put(97)
    put(109)
    put(10)
    0
}
```

## BIOS 부팅 이미지

```tam,boot=bios
// out: booted
use tam.os

fn main() i32 {
    os::println("booted")
    0
}
```
