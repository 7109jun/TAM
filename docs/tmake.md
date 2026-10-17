# Tmake.toml Documentation

`Tmake.toml`은 TAM 프로젝트의 **매니페스트**다. 패키지의 메타데이터, 의존성, 컴파일 대상과 빌드 설정, 최적화 프로파일, 워크스페이스를 한곳에서 선언하고, `tamc`가 그것으로 **재현 가능하고 안정적인 빌드**를 만든다.

- 문법은 **표준 TOML(v1.0)** 그대로다. 독자 문법은 없다.
- 대상 버전: `tamc 1.0`
- 이 문서의 `toml,manifest` 블록과 `project` 묶음은 개발 중 검증 도구가 실제 `tamc`로 검증한다.

## 목차

1. [개요](#1-개요)
2. [빠른 시작](#2-빠른-시작)
3. [매니페스트 스키마](#3-매니페스트-스키마)
4. [버전과 버전 요구사항](#4-버전과-버전-요구사항)
5. [의존성](#5-의존성)
6. [워크스페이스](#6-워크스페이스)
7. [대상과 크로스 빌드](#7-대상과-크로스-빌드)
8. [프로파일](#8-프로파일)
9. [잠금 파일 `Tmake.lock`](#9-잠금-파일-tmakelock)
10. [증분 빌드와 캐시](#10-증분-빌드와-캐시)
11. [명령어](#11-명령어)
12. [빌드 폴더 구조](#12-빌드-폴더-구조)
13. [오류 메시지 사전](#13-오류-메시지-사전)
14. [설계 원칙과 한계](#14-설계-원칙과-한계)

---

## 1. 개요

### 1.1 하는 일

| 일 | 어떻게 |
|---|---|
| 패키지 정의 | `[package]` — 이름, 버전, 종류(`bin`/`lib`), 메타데이터 |
| 의존성 | `[dependencies]` — 경로 의존성과 버전 요구사항 |
| 타겟·빌드 설정 | `[build]`, `[target.<대상>]` — 대상, 출력 폴더, 진입 파일, 출력 이름 |
| 최적화 | `[profile.dev]`, `[profile.release]` |
| 워크스페이스 | `[workspace]` — 여러 패키지, 공통 메타데이터와 의존성 상속 |
| 재현성 | `Tmake.lock`(해결된 그래프 기록), `--locked` |
| 속도 | 입력 전체의 SHA-256 지문으로 안 바뀐 패키지는 건너뜀 |

### 1.2 매니페스트 찾기

`tamc`는 현재 폴더에서 **위로 올라가며** 가장 가까운 `Tmake.toml`을 찾는다. `--manifest-path <경로/Tmake.toml>`로 직접 지정할 수 있다(파일 이름은 반드시 `Tmake.toml`). 찾지 못하면:

```console
error: could not find `Tmake.toml` in `/home/me` or any parent folder
note: create a project with `tamc new <name>` or `tamc init`
```

### 1.3 파일 모드와의 관계

`tamc build main.tam`처럼 **`.tam` 파일을 직접 주면 매니페스트를 쓰지 않는다**(단일 파일 모드). 인자 없이 `tamc build`, `tamc run`, `tamc check`를 쓰거나 `Tmake.toml`/폴더를 주면 프로젝트 모드다.

---

## 2. 빠른 시작

```console
$ tamc new hello
     Created binary `hello` in hello
$ cd hello
$ tamc run
   Compiling hello v0.1.0 (.)
    Finished dev [opt-level 0] host in 0.07s
     Running /home/me/hello/build/host/dev/hello
Hello, TAM!
```

`tamc new hello`가 만드는 것:

```
hello/
├── Tmake.toml
├── .gitignore        # /build
└── src/
    └── main.tam
```

`Tmake.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
tamc = ">=1.0"

[dependencies]
```

라이브러리는 `tamc new util --lib`(`kind = "lib"`, `src/lib.tam`)다. 이미 있는 폴더에서는 `tamc init [이름]`을 쓴다.

두 패키지를 잇는 첫 예:

<!-- project run exit=42 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
util = { path = "../util", version = "^1.2" }
```

```tam,file=app/src/main.tam
use util

fn main() i32 {
    util::double(21)
}
```

```toml,file=util/Tmake.toml
[package]
name = "util"
version = "1.2.0"
kind = "lib"
```

```tam,file=util/src/lib.tam
pub fn double(x: i32) i32 { x * 2 }
```
<!-- /project -->

(위 프로젝트에서 `tamc run`은 종료 코드 42로 끝난다.)

---

## 3. 매니페스트 스키마

최상위에 올 수 있는 표는 `package`, `dependencies`, `build`, `profile`, `target`, `workspace` 여섯 개뿐이다. **알 수 없는 키는 오류**이며, 오타에는 `did you mean`을 알려 준다.

```toml,manifest
[package]
name = "app"
version = "0.1.0"
buidl = 1   # expect-error: unknown key `buidl` in [package]
```

### 3.1 `[package]`

| 키 | 타입 | 필수 | 설명 |
|---|---|:-:|---|
| `name` | 문자열 | ✔ | 패키지 이름. 규칙은 아래 |
| `version` | 문자열 | ✔ | `MAJOR.MINOR.PATCH`(예: `"0.1.0"`). 워크스페이스에서 상속 가능 |
| `kind` | `"bin"` \| `"lib"` | | 생략 시 `src/main.tam`이 있으면 `bin`, 없고 `src/lib.tam`이 있으면 `lib`. `entry`만 있으면 `bin` |
| `entry` | 경로 | | 진입 파일. 기본은 `bin` → `src/main.tam`, `lib` → `src/lib.tam` |
| `description` | 문자열 | | 설명. `tamc metadata`에 나온다 |
| `authors` | 문자열 배열 | | 저자 목록 |
| `license` | 문자열 | | 라이선스 식별자 |
| `repository` | 문자열 | | 저장소 URL |
| `tamc` | 문자열 | | 필요한 컴파일러 버전 요구사항(예: `">=1.0"`). 안 맞으면 빌드가 거절된다 |
| `edition` | 문자열 | `"1"` | 언어 에디션. 지금은 `"1"`만 된다. 다른 값은 `unsupported edition`으로 거절된다([Reference 18장](reference.md#18-호환성-약속-tam-1)) |

`description`, `authors`, `license`, `repository`, `tamc`, `version`은 `{ workspace = true }`로 [워크스페이스에서 상속](#63-상속)할 수 있다.

**패키지 이름 규칙**: 소문자, 숫자, `_`, `-`만 쓰고 **소문자로 시작**하며 최대 64자다. `tam`은 표준 라이브러리용으로 **예약**되어 쓸 수 없다. 이름의 `-`는 `use`에서 `_`로 쓴다(`my-lib` → `use my_lib`).

```toml,manifest
[package]
name = "my-lib"
version = "0.1.0"
kind = "lib"
description = "문자열 도구"
authors = ["lulu <lulu@example.com>"]
license = "MIT"
repository = "https://example.com/my-lib"
tamc = ">=1.0"
```

```toml,manifest
[package]
name = "App"   # expect-error: invalid package name `App`
version = "0.1.0"
```

```toml,manifest
[package]
name = "app"
version = "1.0"   # expect-error: is not a valid version
```

```toml,manifest
[package]
name = "app"
version = "0.1.0"
tamc = ">=99.0"   # expect-error: needs tamc >=99.0
```

### 3.2 `[dependencies]`

```toml
[dependencies]
util = { path = "../util", version = "^1.2" }
core = { workspace = true }
```

각 항목은 **키 = 이름, 값 = 표**다. 허용되는 키는 `path`, `version`, `workspace`뿐이다. 자세한 규칙은 [5장](#5-의존성).

### 3.3 `[build]`

| 키 | 타입 | 기본 | 설명 |
|---|---|---|---|
| `target` | 문자열 | `host`(Windows에서는 `windows`) | 기본 컴파일 대상. `host`, `windows`, `uefi`, `bios`, `aarch64` |
| `out-dir` | 경로 | `build` | 산출물 폴더. **패키지(워크스페이스) 폴더 안**이어야 하고 상대 경로여야 한다 |

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[build]
target = "host"
out-dir = "out"
```

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[build]
out-dir = "../out"   # expect-error: must stay inside the package
```

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[build]
target = "linux"   # expect-error: unknown target `linux`
```

### 3.4 `[profile.dev]`, `[profile.release]`

| 키 | 타입 | dev 기본 | release 기본 | 설명 |
|---|---|---|---|---|
| `opt-level` | 0, 1, 2 | 0 | 2 | 0 = 최적화 없음, 1 = 레지스터 승격, 2 = 승격 + 백엔드 전체 최적화 |

`--release`를 주면 release 프로파일, 아니면 dev 프로파일이다. 다른 이름의 프로파일(`[profile.fast]`)은 없다.

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 2
```

### 3.5 `[target.<대상>]`

대상별 설정이다. `<대상>`은 `host`, `windows`, `uefi`, `bios`, `aarch64` 중 하나다.

| 키 | 설명 |
|---|---|
| `entry` | 이 대상에서 쓸 **진입 파일**(패키지 폴더 안). `[package] entry`를 덮어쓴다 |
| `out-name` | 이 대상 산출물의 **파일 이름**(확장자 제외). 경로 구분자, 빈 문자열, `.`로 시작하는 이름은 불가 |
| `dependencies` | 이 대상에서만 추가되는 의존성(표 형식은 `[dependencies]`와 같다) |

```toml,manifest
[package]
name = "kernel"
version = "0.1.0"

[target.uefi]
entry = "src/uefi_main.tam"
out-name = "BOOTX64"

[target.windows]
out-name = "kernel-win"
```

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[target.windows]
out-name = "a/b"   # expect-error: `out-name` must be a plain file name
```

---

## 4. 버전과 버전 요구사항

### 4.1 버전

`MAJOR.MINOR.PATCH` 세 숫자(각각 0 이상의 정수, **앞자리 0 금지**: `01`은 불가). 빌드 메타데이터(`+b`)와 프리릴리스(`-alpha`)는 읽지만 비교에서 무시된다.

### 4.2 요구사항 문법

쉼표로 여러 조건을 **AND**로 묶는다. 각 조건은 연산자 + 부분 버전이다.

| 표기 | 의미 | 예 |
|---|---|---|
| `^1.2.3`, `1.2.3` (연산자 없음) | 캐럿: 호환(왼쪽 첫 0이 아닌 자리를 고정) | `^1.2.3` = `>=1.2.3, <2.0.0` |
| `^0.2.3` | | `>=0.2.3, <0.3.0` |
| `^0.0.3` | | `=0.0.3` |
| `~1.2.3` | 틸드: 마이너까지 고정 | `>=1.2.3, <1.3.0` |
| `~1` | | `>=1.0.0, <2.0.0` |
| `1.2` | `^1.2`와 같다 | `>=1.2.0, <2.0.0` |
| `=1.2.3` | 정확히 | |
| `>` `>=` `<` `<=` | 비교 | `>=1.0, <2` |
| `*` | 모든 버전 | |
| `1.*`, `1.2.*` | 와일드카드 | |

부분 버전(`1.2`)에서 빠진 자리는 0으로 채워 비교한다. 와일드카드 **뒤에 숫자**(`1.x.y`)나 앞자리 0(`01.2`), 빈 요구사항(`^`)은 오류다.

---

## 5. 의존성

### 5.1 경로 의존성

```toml
[dependencies]
util = { path = "../util" }
```

- **키는 패키지 이름과 같아야 한다.** `other = { path = "../util" }`는 `dependency `other` points at package `util``이다.
- `path`는 패키지 폴더(안에 `Tmake.toml`)를 가리킨다. 패키지 폴더 **밖**(`..`)도 가능하다(`out-dir`과 달리).
- `version`(선택)은 [요구사항](#42-요구사항-문법)이며 의존 패키지의 `version`이 맞지 않으면 거절된다.
- **레지스트리나 git 의존성은 아직 없다**(`git = "..."`은 `unknown key`).
- 의존 패키지는 `kind = "lib"`여야 한다(`bin`은 `is a `bin` and cannot be used as a dependency`).
- 같은 이름의 서로 다른 패키지가 그래프에 둘 있으면 `two different packages are named ...`이다.
- 순환은 `dependency cycle: app -> x -> y -> x`처럼 경로를 보여 준다.

### 5.2 코드에서 쓰기

의존성 패키지의 이름이 **모듈 이름**이다(`-`는 `_`). 공개(`pub`) 항목을 `모듈::항목`으로 쓴다.

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

### 5.3 규칙

- **전이 의존성은 직접 쓸 수 없다.** `app → a → b`일 때 `app`이 `use b`를 하려면 `app`의 `[dependencies]`에도 `b`를 선언해야 한다. 선언하지 않으면 `cannot find module `b``이다. (각 패키지는 자신이 선언한 것만 본다.)
- 패키지 안에서 `use sub.mod`는 **그 패키지의 `src/` 폴더** 기준이다. 라이브러리도 `src/` 아래에 여러 파일을 둘 수 있다.
- **의존성 이름이 같은 이름의 지역 모듈보다 우선한다.** `util` 의존성이 있으면 `src/util.tam`이 있어도 `use util`은 의존성을 가리킨다.
- 가시성은 그대로다: 의존 패키지의 비공개 항목에 접근하면 선언 위치를 함께 보여 주는 오류가 난다.
- 같은 패키지가 여러 경로로 이어져도(다이아몬드) **한 번만** 컴파일에 들어간다.

<!-- project run exit=7 -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
a = { path = "../a" }
```

```tam,file=app/src/main.tam
use a

fn main() i32 {
    a::fa()
}
```

```tam,file=app/src/a.tam
// 같은 이름의 지역 모듈이 있어도 의존성 `a`가 우선한다.
pub fn fa() i32 { 1000 }
```

```toml,file=a/Tmake.toml
[package]
name = "a"
version = "0.1.0"
kind = "lib"
```

```tam,file=a/src/lib.tam
use util

pub fn fa() i32 { util::deep() }
```

```tam,file=a/src/util.tam
pub fn deep() i32 { 7 }
```
<!-- /project -->

<!-- project fail=cannot find module `b` -->
```toml,file=app/Tmake.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
a = { path = "../a" }
```

```tam,file=app/src/main.tam
use a
use b

fn main() i32 { a::fa() + b::fb() }
```

```toml,file=a/Tmake.toml
[package]
name = "a"
version = "0.1.0"
kind = "lib"

[dependencies]
b = { path = "../b" }
```

```tam,file=a/src/lib.tam
use b

pub fn fa() i32 { b::fb() + 1 }
```

```toml,file=b/Tmake.toml
[package]
name = "b"
version = "0.1.0"
kind = "lib"
```

```tam,file=b/src/lib.tam
pub fn fb() i32 { 40 }
```
<!-- /project -->

(위 마지막 프로젝트는 `app`이 `b`를 선언하지 않아 거절된다. 실제로 쓰려면 `app`의 `[dependencies]`에 `b`를 추가한다. 이 예의 `fail=`는 그것을 검증한다.)

### 5.4 대상별 의존성

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[dependencies]
core = { path = "../core" }

[target.windows.dependencies]
winapi = { path = "../winapi" }
```

`[target.<t>.dependencies]`의 항목은 그 대상을 빌드할 때만 그래프에 들어간다. 같은 이름을 두 표에 쓰면 `dependency ... is listed twice`다. **`Tmake.lock`에는 모든 대상의 의존성이 합쳐져** 기록된다([9장](#9-잠금-파일-tmakelock)).

### 5.5 잘못된 의존성 선언

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[dependencies]
util = "../util"   # expect-error: dependency `util` must be a table
```

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[dependencies]
util = { version = "1" }   # expect-error: dependency `util` needs a `path`
```

```toml,manifest
[package]
name = "app"
version = "0.1.0"

[dependencies]
util = { path = "../u", git = "x" }   # expect-error: unknown key `git`
```

---

## 6. 워크스페이스

여러 패키지를 하나의 **워크스페이스**로 묶으면 같은 잠금 파일, 같은 빌드 폴더, 공통 메타데이터와 의존성을 공유한다.

### 6.1 선언

워크스페이스 **루트**의 `Tmake.toml`에 `[workspace]`를 둔다.

```toml
[workspace]
members = ["app", "libs/*"]        # 멤버 폴더(끝에 /* 글롭 하나 허용)
exclude = ["libs/experimental"]    # 제외할 폴더
default-members = ["app"]          # 인자 없이 build/check할 때의 기본 대상
```

| 키 | 설명 |
|---|---|
| `members` | 멤버 패키지 폴더 목록. `"libs/*"`는 `libs/` 아래 `Tmake.toml`이 있는 **모든 하위 폴더**(이름순). 그 밖의 `*`는 불가 |
| `exclude` | `members`에서 뺄 폴더 |
| `default-members` | 기본 대상. 반드시 `members`에 있는 폴더여야 한다. 없으면 모든 멤버 |
| `package` | 공통 메타데이터(`[workspace.package]`) |
| `dependencies` | 공통 의존성(`[workspace.dependencies]`) |

루트는 **`[package]`도 함께** 가질 수 있다(루트 패키지). 멤버도 루트 패키지도 없으면 `the workspace has no members`다. 각 멤버 폴더에는 `Tmake.toml`이 있어야 한다.

### 6.2 어떤 패키지가 빌드되나

| 상황 | 빌드되는 것 |
|---|---|
| 멤버 폴더 안에서 인자 없이 | **그 패키지** |
| 워크스페이스 루트에서 인자 없이 | `default-members` |
| `-p <이름>` | 이름이 같은 멤버(루트 패키지 포함) |
| `--workspace` | 모든 멤버(와 루트 패키지) |

`run`은 **정확히 하나의 `bin` 패키지**가 선택되어야 한다(아니면 `-p`로 지정하라는 오류).

### 6.3 상속

`[workspace.package]`에 둔 값을 멤버가 `{ workspace = true }`로 가져온다. 상속 가능한 키: `version`, `authors`, `description`, `license`, `repository`, `tamc`.

`[workspace.dependencies]`에 선언한 의존성은 멤버가 `{ workspace = true }`로 가져온다. 경로는 **워크스페이스 루트 기준**으로 해석된다.

<!-- project run exit=42 -->
```toml,file=Tmake.toml
[workspace]
members = ["app", "libs/*"]
default-members = ["app"]

[workspace.package]
version = "0.3.0"
authors = ["team"]
license = "MIT"

[workspace.dependencies]
core = { path = "libs/core", version = "^0.3" }

[profile.release]
opt-level = 2
```

```toml,file=app/Tmake.toml
[package]
name = "app"
version = { workspace = true }
license = { workspace = true }
description = "워크스페이스 예제"

[dependencies]
core = { workspace = true }
fmt = { path = "../libs/fmt" }
```

```tam,file=app/src/main.tam
use core
use fmt

fn main() i32 {
    fmt::twice(core::base())
}
```

```toml,file=libs/core/Tmake.toml
[package]
name = "core"
version = { workspace = true }
kind = "lib"
```

```tam,file=libs/core/src/lib.tam
pub fn base() i32 { 21 }
```

```toml,file=libs/fmt/Tmake.toml
[package]
name = "fmt"
version = { workspace = true }
kind = "lib"
```

```tam,file=libs/fmt/src/lib.tam
pub fn twice(x: i32) i32 { x * 2 }
```
<!-- /project -->

```console
$ tamc tree
app v0.3.0
    core v0.3.0
    fmt v0.3.0
```

- 워크스페이스 밖에서 `{ workspace = true }`를 쓰면 `` `version.workspace = true` but there is no workspace``이고, 워크스페이스에 그 값이 없으면 `the workspace has no `version` to inherit`이다.
- **프로파일, 빌드 폴더, 잠금 파일은 워크스페이스 루트의 것**이 쓰인다(멤버 매니페스트의 `[profile]`은 무시된다).
- `[workspace.dependencies]`에 없는 이름을 `{ workspace = true }`로 쓰면 `dependency `x` is not in `[workspace.dependencies]``이다.

### 6.4 워크스페이스 오류

```toml,manifest
[workspace]
members = ["nope"]   # expect-error: workspace member `nope` has no Tmake.toml
```

```toml,manifest
[workspace]
members = ["a*b"]   # expect-error: only a trailing `/*` glob is supported in `members`
```

---

## 7. 대상과 크로스 빌드

### 7.1 대상 고르기

우선순위: **`--target` 옵션 > `[build] target` > 기본값**(Windows에서는 `windows`, 그 밖에는 `host`).

| 대상 | 산출물 | 필요한 도구 |
|---|---|---|
| `host` | 확장자 없는 실행 파일 | 시스템 `clang` |
| `windows` | `.exe` | `llc`(또는 `clang`), `lld-link`(tamc와 같은 폴더 또는 PATH) |
| `uefi` | `.efi` | `llc`(또는 `clang`), `lld-link` |
| `bios` | `.img` | `llc`(또는 `clang`), `lld-link` |
| `aarch64` | `.bin` | `llc`(또는 `clang`), `lld-link` |

`check`는 도구가 없어도 모든 대상에서 동작한다(타입, 소유권, 빌림만 검사한다). `run`은 `host`(또는 Windows 호스트에서 `windows`)만 지원한다. 펌웨어 대상은 `build`로 만들어 QEMU 등으로 부팅한다.

### 7.2 대상별 진입 파일과 이름

```toml,manifest
[package]
name = "kernel"
version = "0.1.0"

[target.uefi]
entry = "src/uefi_main.tam"
out-name = "BOOTX64"
```

`uefi` 대상으로 빌드하면 `src/uefi_main.tam`이 루트(진입 모듈)가 되고 산출물은 `build/uefi/<프로파일>/BOOTX64.efi`다. `entry`는 **루트 패키지에만** 적용된다(의존성 패키지의 `[target.*] entry`는 무시한다).

### 7.3 검사만

```console
$ tamc check --target windows
$ tamc check --target uefi
```

---

## 8. 프로파일

| 프로파일 | 선택 | `opt-level` 기본 |
|---|---|---|
| dev | 기본 | 0 |
| release | `--release` | 2 |

- `opt-level = 0`: IR을 그대로 `llc`/`clang`에 넘긴다(`-O0`).
- `opt-level = 1`: tamc가 **스택 변수를 SSA 레지스터로 승격**(`mem2reg`)한다.
- `opt-level = 2`: 승격 + 백엔드 `-O2`.
- 환경 변수 `TAMC_NO_OPT=1`은 승격을 끈다(디버깅용).
- 오버플로 검사, 경계 검사 같은 **언어의 안전 검사는 프로파일로 끌 수 없다**(릴리스에서도 같다).

---

## 9. 잠금 파일 `Tmake.lock`

`tamc build/run/check`가 **자동으로 만들고 갱신**한다. 워크스페이스 루트(워크스페이스가 아니면 패키지 폴더)에 놓이며 버전 관리에 **커밋**한다.

```toml
# This file is generated by tamc. Do not edit it by hand.
version = 1

[[package]]
name = "app"
version = "0.3.0"
path = "app"
dependencies = ["core", "fmt"]

[[package]]
name = "core"
version = "0.3.0"
path = "libs/core"

[[package]]
name = "fmt"
version = "0.3.0"
path = "libs/fmt"
```

- 패키지는 **이름순**, 경로는 루트 기준 `/` 구분 상대 경로라서, 같은 입력이면 **항상 같은 바이트**다(머신, 대상, OS와 무관).
- **모든 대상의 의존성을 합쳐서** 기록하므로, 어떤 대상으로 빌드해도 같은 파일이 된다.
- `--locked`: 잠금 파일이 없거나 갱신이 필요하면 **오류로 끝낸다**(CI 권장).

```console
$ tamc build --locked
error: `Tmake.lock` needs to be updated but `--locked` was given
note: run `tamc build` without `--locked` and commit the result
```

- 의존성이 경로(path)뿐이므로 잠금 파일은 **그래프의 스냅샷**이며 "버전 재해결"은 없다. 레지스트리가 생기면 여기에 체크섬이 더해진다.

---

## 10. 증분 빌드와 캐시

빌드는 패키지마다 **지문(SHA-256)**을 계산한다. 지문의 입력은 다음 전부다.

1. 컴파일러 버전, 대상, `opt-level`, 패키지 종류
2. 읽은 **모든 `Tmake.toml`**(내용)
3. 읽은 **모든 소스 파일**(경로와 내용, 내장 표준 라이브러리 제외)

지문이 `build/<대상>/<프로파일>/.fingerprint/`에 저장된 값과 같고 산출물이 있으면 컴파일을 건너뛰고 `Fresh`를 출력한다. 소스 한 글자만 바뀌어도(의존성 포함) 다시 컴파일한다.

```console
$ tamc build
   Compiling app v0.3.0 (app)
    Finished dev [opt-level 0] host in 0.06s
$ tamc build
       Fresh app v0.3.0 (app)
```

- 상태 메시지는 **stderr**로 나간다. `-q`/`--quiet`로 끈다.
- `tamc clean`은 빌드 폴더를 지운다.
- 라이브러리 패키지는 **검사만**(`Checking`) 하며 지문도 따로(`.chk`) 저장한다.
- 컴파일 단위는 **프로그램 전체**다(패키지별 목적 파일이 아니다). 의존성이 바뀌면 그것을 쓰는 모든 패키지가 다시 컴파일된다.

---

## 11. 명령어

```
tamc <명령> [옵션]            프로젝트 모드(가장 가까운 Tmake.toml 사용)
tamc <명령> <파일.tam> ...    파일 모드(Tmake.toml을 쓰지 않음)
```

### 11.1 프로젝트 명령

| 명령 | 설명 |
|---|---|
| `new <이름> [--lib]` | `./<이름>/`에 새 패키지를 만든다. 이미 있으면 오류 |
| `init [이름] [--lib]` | **현재 폴더**에 `Tmake.toml`과 `src/`를 만든다. 이름을 생략하면 폴더 이름(소문자, 공백/점은 `-`)을 쓴다 |
| `build` | 컴파일해 실행 파일(또는 이미지)을 만든다. 라이브러리는 검사만 한다 |
| `run` | 빌드하고 실행한다. 프로그램의 종료 코드로 `tamc`도 끝난다 |
| `check` | 구문, 타입, 소유권, 빌림만 검사한다(링크하지 않는다) |
| `clean` | 빌드 폴더를 지운다 |
| `tree` | 의존성 트리를 출력한다 |
| `metadata` | 해결된 프로젝트를 **JSON 한 줄**로 출력한다(도구 연동용) |
| `--version` | 컴파일러 버전을 출력한다 |

### 11.2 옵션

| 옵션 | 설명 |
|---|---|
| `--release` | release 프로파일 |
| `--target <t>` | 대상을 정한다(`host`, `windows`, `uefi`, `bios`, `aarch64`) |
| `-p <이름>`, `--package <이름>` | 패키지를 고른다 |
| `--workspace`, `--all` | 모든 워크스페이스 멤버 |
| `--locked` | 잠금 파일이 바뀌면 오류 |
| `-q`, `--quiet` | 상태 메시지를 끈다 |
| `--manifest-path <경로>` | 사용할 `Tmake.toml` |
| `--lib` | `new`/`init`에서 라이브러리로 |
| `-- <인자...>` | `run`에서 프로그램에 넘길 인자(`tam.os`는 아직 읽지 못한다) |

종료 코드: 성공 0, 컴파일/매니페스트 오류 1, 사용법 오류 2, `run`은 프로그램의 종료 코드(신호로 죽으면 128 + 신호 번호).

### 11.3 상태 메시지

stderr에 오른쪽 정렬된 태그로 나온다.

| 태그 | 의미 |
|---|---|
| `Locking` | 잠금 파일을 만들었거나 갱신했다 |
| `Compiling` | 컴파일하고 링크한다 |
| `Checking` | 검사한다(`check`, 라이브러리) |
| `Fresh` | 지문이 같아 건너뛰었다 |
| `Finished` | 끝(프로파일, 최적화 수준, 대상, 걸린 시간) |
| `Running` | `run`이 실행 파일을 시작한다 |
| `Created` | `new`/`init`이 프로젝트를 만들었다 |
| `Removed` | `clean`이 폴더를 지웠다 |

### 11.4 `metadata`의 출력

```json
{"tamc":"1.0.0","workspace_root":"/p","build_dir":"/p/build","packages":[
  {"name":"app","version":"0.3.0","kind":"bin","manifest_path":"/p/app/Tmake.toml",
   "entry":"/p/app/src/main.tam","description":null,"dependencies":["core","fmt"],
   "workspace_member":true}]}
```

(실제 출력은 한 줄이다.) `workspace_member`는 이 패키지가 워크스페이스의 멤버(또는 선택한 루트)인지다. 문자열은 JSON 규칙으로 이스케이프된다.

---

## 12. 빌드 폴더 구조

```
build/
└── <대상>/
    └── <프로파일>/
        ├── <출력 이름>[.exe|.efi|.img|.bin]
        └── .fingerprint/
            ├── <패키지>.bin      # 빌드 지문
            └── <패키지>.chk      # 검사 지문
```

- 출력 이름은 `[target.<t>] out-name`, 없으면 패키지 이름이다.
- 빌드 폴더의 위치는 `[build] out-dir`(기본 `build`)이며 워크스페이스에서는 루트 기준이다.
- `tamc new`가 만드는 `.gitignore`는 `/build`를 제외한다.

---

## 13. 오류 메시지 사전

모든 매니페스트 오류는 `Tmake.toml`의 **파일, 줄, 칸**을 가리키며 해결 힌트를 붙인다.

```console
error: unknown key `buidl` in [package]
 --> /home/me/app/Tmake.toml:4:1
  |
4 | buidl = 1
  | ^^^^^ did you mean `build`?
```

| 메시지 | 원인과 해결 |
|---|---|
| `invalid TOML: ...` | TOML 문법 오류(줄 위치 표시). 같은 키를 두 번 쓴 경우도 여기에 해당 |
| `unknown key `k` in <표>` | 허용되지 않는 키. 비슷한 키가 있으면 `did you mean` |
| `` `[package]` needs a `name` `` / `` ... `version` `` | 필수 키 누락 |
| `invalid package name` | 소문자/숫자/`_`/`-`, 소문자로 시작, 64자 이하 |
| `` `tam` is reserved for the standard library `` | `tam`은 쓸 수 없는 이름 |
| `` `1.0` is not a valid version `` | `MAJOR.MINOR.PATCH` 필요 |
| `` `version` must be a string `` 등 | 값의 타입이 다름(`must be a table` 등도 같다) |
| `unknown package kind` | `kind`는 `"bin"` 또는 `"lib"` |
| `package `p` has no `src/main.tam` or `src/lib.tam`` | 진입 파일이 없다. 만들거나 `entry`/`kind`를 지정 |
| `entry file ... does not exist` | `entry`/`[target.*] entry`가 가리키는 파일이 없다 |
| `unknown target` | `host`/`windows`/`uefi`/`bios`/`aarch64`만 가능 |
| `must be a relative path` / `must stay inside the package` | `out-dir`, `entry`, `members` 등은 상대 경로이고 `..`로 나갈 수 없다 |
| `` `opt-level` must be 0, 1 or 2 `` | 범위 밖 |
| `dependency `d` must be a table` | `d = { path = "..." }` 형식 |
| `dependency `d` needs a `path`` | 경로 없는 의존성(레지스트리는 아직 없다) |
| `invalid dependency name` | 이름 규칙은 패키지와 같다 |
| `cannot load dependency `d`` | `path/Tmake.toml`이 없다 |
| `dependency `d` points at package `p`` | 키와 패키지 이름이 다르다 |
| `dependency `d` is version V, which does not satisfy `R`` | 버전 요구사항을 만족하지 않는다 |
| `is a `bin` and cannot be used as a dependency` | 의존성은 `kind = "lib"` |
| `dependency cycle: a -> b -> a` | 순환 의존 |
| `two different packages are named `n`` | 이름 충돌 |
| `package `p` needs tamc R, but this is tamc V` | `tamc` 요구사항 불일치 |
| `workspace member `m` has no Tmake.toml` | 멤버 폴더 확인 |
| `the workspace has no members` | `members`가 비었고 루트 패키지도 없다 |
| `` `k.workspace = true` but there is no workspace `` | 상속은 워크스페이스 안에서만 |
| `Tmake.lock needs to be updated but --locked was given` | 잠금 파일 갱신 필요 |
| `run needs exactly one binary package` | `-p <이름>`으로 지정 |
| `cannot run a Windows executable on this machine` | Windows 이외에서 `run --target windows` |

```toml,manifest
[package
name = "app"   # expect-error: invalid TOML
```

---

## 14. 설계 원칙과 한계

### 원칙

- **표준 TOML**만 쓴다. 파서는 검증된 구현을 쓰고, 의미 검사는 `tamc`가 한다.
- **안정성**: 알 수 없는 키, 잘못된 타입, 경로 이탈, 순환, 버전 불일치는 **조용히 넘어가지 않고** 위치와 함께 오류가 된다.
- **재현성**: 같은 입력은 같은 `Tmake.lock`과 같은 지문을 만든다. 상태 파일은 정렬되고 시간·경로·머신 정보를 담지 않는다.

### 한계 (현재)

- 의존성은 **경로**뿐이다. 레지스트리, git, 체크섬은 없다.
- 컴파일 단위는 프로그램 전체다. 라이브러리는 **검사만** 하며 목적 파일을 만들지 않는다.
- 전이 의존성은 직접 선언해야 쓸 수 있고, 의존성 이름은 지역 모듈을 가린다.
- `[profile]`은 `opt-level`만 있다(안전 검사는 언어 보장이라 끌 수 없다). 사용자 프로파일 이름은 없다.
- `[features]`, 빌드 스크립트, `[patch]`, 조건부 설정(`cfg`)은 없다. 대상별 차이는 `[target.<t>]`로 표현한다.
- 워크스페이스 `members`의 글롭은 끝의 `/*` 하나만 지원한다.
- Windows용 `tamc.exe`는 이 저장소의 CI 워크플로(`.github/workflows/windows.yml`)로 만든다.
