# 9. 모듈과 프로젝트

프로그램이 커지면 파일을 나눈다. TAM에서는 **파일 하나가 모듈 하나**다. 더 큰 단위는 `Tmake.toml`이 관리하는 **패키지**와 **워크스페이스**다.

## 9.1 `use`와 `pub`

`use a.b`는 루트 폴더 아래의 `a/b.tam`을 불러오고, 이후 `b::항목`으로 쓴다. 다른 모듈에서 쓰이려면 항목에 `pub`을 붙인다.

<!-- project run exit=7 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"
```

```tam,file=app/src/main.tam
use geo.shapes

fn main() i32 {
    let r = shapes::Rect { w: 3, h: 2 }
    shapes::area(&r) + shapes::unit()
}
```

```tam,file=app/src/geo/shapes.tam
pub struct Rect {
    w: i32,
    h: i32,
}

pub fn area(r: &Rect) i32 { r.w * r.h }

pub fn unit() i32 { helper() }

fn helper() i32 { 1 }          // pub이 없으므로 이 파일 안에서만 보인다
```
<!-- /project -->

규칙:

- 모듈 경로는 **한 단계**다: `shapes::area`. (`geo::shapes::area`는 없다.)
- 루트 폴더는 프로젝트에서는 패키지의 `src/`다. `use` 경로는 항상 이 루트 기준이다.
- `pub`이 없는 항목을 밖에서 쓰면 컴파일 오류가 나며, 두 파일의 위치를 함께 보여 준다.
- 구조체 필드는 항상 공개다.
- 같은 모듈 이름을 두 번 가져오면(`use a.math`, `use b.math`) 오류다.

<!-- project fail=private -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"
```

```tam,file=app/src/main.tam
use util

fn main() i32 { util::secret() }
```

```tam,file=app/src/util.tam
fn secret() i32 { 1 }
```
<!-- /project -->

## 9.2 표준 라이브러리

첫 조각이 `tam`이면 파일이 아니라 컴파일러에 내장된 표준 라이브러리다.

```tam
// out: hi 3
use tam.os

fn main() i32 {
    os::print("hi ")
    os::print_int(3)
    os::println("")
    0
}
```

전체 API는 [표준 라이브러리 문서](../stdlib.md)를 본다.

## 9.3 패키지

**패키지**는 `Tmake.toml` 하나와 `src/` 폴더다. 만들고 실행하는 가장 짧은 방법:

```console
$ tamc new hello
$ cd hello
$ tamc run
```

```
hello/
├── Tmake.toml
└── src/
    └── main.tam
```

```toml
[package]
name = "hello"
version = "0.1.0"
```

`tamc build`는 `Tmake.toml`을 찾아 의존성 순서대로 빌드하고 `build/<대상>/<프로필>/`에 결과를 둔다. 바뀐 것이 없으면 다시 만들지 않는다(내용 해시 기반).

## 9.4 라이브러리 패키지와 의존성

재사용할 코드는 `kind = "lib"` 패키지로 분리하고 `[dependencies]`로 가져다 쓴다. **의존성 이름이 곧 모듈 이름**이다(`-`는 `_`).

<!-- project run exit=44 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
text-tools = { path = "../text-tools" }
```

```tam,file=app/src/main.tam
use text_tools

fn main() i32 {
    text_tools::triple(14) + 2
}
```

```toml,file=text-tools/Tmake.toml
[package]
name = "text-tools"
version = "0.1.0"
kind = "lib"
```

```tam,file=text-tools/src/lib.tam
pub fn triple(x: i32) i32 { x * 3 }
```
<!-- /project -->

- 라이브러리는 `src/lib.tam`이 시작점이고 `main`이 필요 없다. 빌드할 때는 **검사만** 한다(코드는 이를 쓰는 실행 파일과 함께 만들어진다).
- 의존성은 **경로**로만 지정한다.
- 간접 의존성은 직접 선언해야 쓸 수 있다.

## 9.5 워크스페이스

패키지 여러 개를 한 저장소에서 함께 다룬다. 루트 `Tmake.toml`에 `[workspace]`를 둔다.

```toml
[workspace]
members = ["app", "libs/*"]
default-members = ["app"]

[workspace.package]
version = "0.3.0"
license = "MIT"

[workspace.dependencies]
core = { path = "libs/core", version = "^0.3" }

[profile.release]
opt-level = 2
```

멤버 패키지는 `version = { workspace = true }`, `core = { workspace = true }`처럼 공통 값을 물려받는다. 저장소의 `examples/project/`가 완전한 예다. 모든 필드는 [Tmake.toml 문서](../tmake.md)에 있다.

## 9.6 프로필과 대상

```console
$ tamc build                      # 기본: opt-level 0
$ tamc build --release            # opt-level 2
$ tamc build --target windows     # 다른 대상
$ tamc build --locked             # Tmake.lock이 최신이 아니면 실패
```

의존성이 확정되면 `Tmake.lock`이 만들어진다. 저장소에 함께 올려 두면 모두 같은 구성으로 빌드한다.

## 9.7 정리

| 단위 | 정의 | 가져오기 |
|---|---|---|
| 모듈 | `.tam` 파일 하나 | `use 경로` |
| 패키지 | `Tmake.toml` + `src/` | `[dependencies]` 후 `use 이름` |
| 워크스페이스 | `[workspace]` | 멤버 패키지끼리 의존 |

→ [10. unsafe와 시스템 프로그래밍](10-unsafe-and-systems.md)
