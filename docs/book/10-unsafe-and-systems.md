# 10. unsafe와 시스템 프로그래밍

TAM의 약속은 이렇다. **`unsafe` 바깥은 컴파일러가 안전을 책임진다. `unsafe` 안은 당신이 책임진다.** 이 장은 그 안쪽을 다룬다. 운영체제, 드라이버, C 라이브러리 연동에 필요한 도구들이다.

## 10.1 raw 포인터

`*const T`는 읽기용, `*mut T`는 읽고 쓰는 포인터다. **빌림 검사를 받지 않는다.**

```tam
// exit: 8
fn main() i32 {
    mut x = 5
    let p = &mut x as *mut i32         // 참조 → 포인터: safe
    unsafe { *p = 8 }                  // 역참조: unsafe
    x
}
```

만들기와 변환(`&x as *const i32`, 포인터 ↔ 포인터, 포인터 ↔ `u64`/`i64`)은 safe이고, 다음은 `unsafe`가 필요하다.

| 동작 | 예 |
|---|---|
| 역참조 | `*p`, `*p = v` |
| 포인터 산술 | `p + n`, `p - n` (단위: 원소) |
| `unsafe fn`, `extern` 함수 호출 | |
| `alloc`, `free`, 하드웨어 내장 함수 | |

```tam,error
// error: needs an `unsafe` block
fn main() i32 {
    let x = 1
    let p = &x as *const i32
    *p
}
```

`unsafe { ... }`는 **값을 가지는 식**이다. 마지막 식이 값이다.

## 10.2 직접 할당

`alloc(바이트 수)`는 힙 블록을, `free(포인터)`는 해제를 한다. **자동 해제는 없다.** 해제를 잊으면 누수이고, 두 번 하거나 해제 후 쓰면 정의되지 않은 동작이다.

```tam
// exit: 15
fn main() i32 {
    let n: u64 = 5
    let raw = unsafe { alloc(n * 4) }          // i32 5개
    let a = raw as *mut i32
    unsafe {
        for i in 0..n { *(a + i) = (i as i32) + 1 }
    }
    mut s = 0
    for i in 0..n { s += unsafe { *(a + i) } }
    unsafe { free(raw) }
    s
}
```

## 10.3 unsafe를 safe로 감싸기

`unsafe`는 작게 모아 두고 바깥에는 안전한 인터페이스만 보이게 하라. 경계 검사를 직접 넣어 주면 사용하는 쪽은 `unsafe`를 알 필요가 없다.

```tam
// exit: 7
struct IntBuf {
    ptr: *mut i32,
    len: u64,
}

impl IntBuf {
    fn new(len: u64) Self {
        let raw = unsafe { alloc(len * 4) } as *mut i32
        for i in 0..len { unsafe { *(raw + i) = 0 } }
        IntBuf { ptr: raw, len: len }
    }

    fn set(&mut self, i: u64, v: i32) {
        assert(i < self.len)                    // 경계 검사는 우리 책임
        unsafe { *(self.ptr + i) = v }
    }

    fn get(&self, i: u64) i32 {
        assert(i < self.len)
        unsafe { *(self.ptr + i) }
    }

    fn release(self) {                          // 소비하며 해제
        unsafe { free(self.ptr as *mut u8) }
    }
}

fn main() i32 {
    mut b = IntBuf::new(4)
    b.set(1, 3)
    b.set(3, 4)
    let r = b.get(1) + b.get(3)
    b.release()
    r
}
```

주의: 구조체는 안의 포인터를 **자동으로 해제하지 않는다**(포인터는 소유권이 없다). 해제 함수를 명시적으로 부르는 것이 컴파일러가 대신해 줄 수 없는 부분이다.

## 10.4 `unsafe fn`

호출자가 지켜야 할 조건이 있는 함수는 `unsafe fn`으로 선언한다. 본문 전체가 `unsafe` 맥락이고, 호출은 `unsafe` 안에서만 가능하다.

```tam
// exit: 4
unsafe fn read_at(p: *const i32, i: u64) i32 {
    *(p + i)                           // 호출자가 범위를 보장해야 한다
}

fn main() i32 {
    let a = [1, 2, 4]
    let p = &a as *const [i32; 3] as *const i32
    unsafe { read_at(p, 2) }
}
```

함수 위에 "무엇을 보장해야 하는가"를 주석으로 적는 습관을 들인다.

## 10.5 C 라이브러리 호출: `extern`

본문 없는 선언으로 외부 함수를 부른다. 호스트에서는 `"c"`가 libc다.

```tam
// exit: 7
extern "c" fn abs(x: i32) i32

fn main() i32 {
    unsafe { abs(0 - 7) }
}
```

- 인자·반환 타입은 C ABI에 맞는 것만 쓴다: **정수형, `f64`, `bool`, raw 포인터**.
- 문자열을 넘기려면 널 종료 바이트 배열을 만들어 `*const u8`로 넘긴다.

```tam
// exit: 5
extern "c" fn strlen(s: *const u8) u64

fn main() i32 {
    let s: [u8; 6] = [104, 101, 108, 108, 111, 0]   // "hello\0"
    unsafe { strlen(&s as *const [u8; 6] as *const u8) as i32 }
}
```

- 실제 심볼 이름이 다르면 `extern "c" fn my_name(...) = "real_symbol"`.
- Windows 대상에서는 DLL 이름(`"kernel32"`)을 쓴다. 대상과 맞지 않으면 오류다. 

## 10.6 수 연산을 직접 제어하기

기본 산술은 오버플로 시 trap한다. 해시, 난수, 체크섬처럼 **감싸는 것이 의도**일 때는 명시한다.

```tam
// exit: 1
fn main() i32 {
    let a: u32 = 4000000000
    let b = wrapping_add(a, 500000000)         // 4_500_000_000 mod 2^32
    if b == 205032704 { 1 } else { 0 }
}
```

`wrapping_add`, `wrapping_sub`, `wrapping_mul`이 있다. 시프트의 양은 비트 폭 − 1로 마스크되고, `as` 변환은 값을 자르거나 포화시킨다([레퍼런스 10장](../reference.md#10-수치-의미-오버플로-변환-시프트)).

## 10.7 하드웨어 접근

펌웨어 대상(`bios`, `uefi`, `aarch64`)에서는 하드웨어 내장 함수를 쓸 수 있다. 모두 `unsafe`다.

| 내장 함수 | 용도 | 대상 |
|---|---|---|
| `mmio_read8/16/32/64(addr)` `mmio_write…(addr, v)` | 메모리 맵 I/O (volatile) | 펌웨어 |
| `port_in8/in32(port)` `port_out8/out32(port, v)` | x86 I/O 포트 | bios, uefi |
| `halt()` | CPU 정지 | 펌웨어 |
| `semihost(op, arg)` | 호스트 호출 | aarch64 |

MMIO는 **일반 메모리가 아니다.** 읽기와 쓰기 자체가 효과이므로 컴파일러가 줄이거나 없애지 않는다(volatile). 장치 레지스터는 항상 `mmio_*`로만 만진다. 실제로 동작하는 예는 [11장](11-targets.md)에서 본다.

## 10.8 안전하게 쓰는 요령

1. **`unsafe` 블록은 한 줄이나 몇 줄로 작게.** 길수록 어디서 깨졌는지 모른다.
2. **불변식을 주석으로 적는다.** "i < len임을 호출자가 보장한다."
3. **경계가 필요하면 `assert`로 직접 건다.** 느려지는 것보다 조용히 메모리를 깨는 쪽이 훨씬 비싸다.
4. **소유권 하나, 해제 지점 하나.** 누가 `free`하는지 이름만 봐도 알게 한다.
5. `unsafe`가 없는 코드는 **어떤 입력에서도 메모리를 깨지 않는다.** 버그가 났다면 범인은 `unsafe` 안에 있다. 찾을 곳이 좁다.

→ [11. 여러 대상으로 빌드하기](11-targets.md)
