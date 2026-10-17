# 8. 모듈과 프로젝트

## 파일 나누기

`use geo.shapes`는 `src/geo/shapes.tam`을 불러온다. 쓰려면 `pub`이어야 한다.

<!-- project run exit=10 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"
```

```tam,file=app/src/main.tam
use geo.shapes

fn main() i32 {
    let s = shapes::Square { side: 3 }
    shapes::area(&s) + 1
}
```

```tam,file=app/src/geo/shapes.tam
pub struct Square { side: i32 }

pub fn area(s: &Square) i32 { s.side * s.side }
```
<!-- /project -->

## 비공개 항목은 밖에서 못 쓴다

<!-- project fail=private -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"
```

```tam,file=app/src/main.tam
use util

fn main() i32 { util::hidden() }
```

```tam,file=app/src/util.tam
fn hidden() i32 { 1 }
```
<!-- /project -->

## 라이브러리 의존성

<!-- project run exit=42 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
math-kit = { path = "../math-kit" }
```

```tam,file=app/src/main.tam
use math_kit

fn main() i32 {
    math_kit::square(6) + math_kit::double(3)
}
```

```toml,file=math-kit/Tmake.toml
[package]
name = "math-kit"
version = "0.1.0"
kind = "lib"
```

```tam,file=math-kit/src/lib.tam
pub fn square(x: i32) i32 { x * x }
pub fn double(x: i32) i32 { x * 2 }
```
<!-- /project -->

## 모듈 안의 모듈 불러오기

모듈에서도 `use`를 쓴다. 경로는 항상 루트(`src/`) 기준이다.

<!-- project run exit=9 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"
```

```tam,file=app/src/main.tam
use calc

fn main() i32 { calc::nine() }
```

```tam,file=app/src/calc.tam
use base.num

pub fn nine() i32 { num::three() * num::three() }
```

```tam,file=app/src/base/num.tam
pub fn three() i32 { 3 }
```
<!-- /project -->

## 순환하는 `use`도 허용된다

<!-- project run exit=2 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"
```

```tam,file=app/src/main.tam
use even
fn main() i32 { if even::is_even(10) { 2 } else { 3 } }
```

```tam,file=app/src/even.tam
use odd
pub fn is_even(n: i32) bool { if n == 0 { true } else { odd::is_odd(n - 1) } }
```

```tam,file=app/src/odd.tam
use even
pub fn is_odd(n: i32) bool { if n == 0 { false } else { even::is_even(n - 1) } }
```
<!-- /project -->

## 워크스페이스

```toml
[workspace]
members = ["app", "libs/*"]
default-members = ["app"]

[workspace.dependencies]
core = { path = "libs/core" }
```

완전한 워크스페이스 예제는 저장소의 `examples/project/`에 있다. `tamc tree`로 의존성 트리를, `tamc metadata`로 JSON 요약을 본다.

## 프로필과 대상

```toml
[profile.release]
opt-level = 2

[build]
target = "host"

[target.windows]
out-name = "app"
```

```console
$ tamc build --release
$ tamc build --target windows
```
