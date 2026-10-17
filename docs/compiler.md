# Compiler Architecture

`tamc`가 TAM 소스를 어떻게 실행 파일로 바꾸는지, 코드가 어디에 있는지, 어떻게 시험하는지를 설명한다. 컴파일러를 고치거나 비슷한 컴파일러를 만들려는 사람을 위한 문서다.

- 대상 버전: `tamc 1.0`
- `tamc`는 Rust로 쓰였고, 외부 크레이트는 `toml`(매니페스트 파싱)과 `sha2`(증분 빌드 지문)뿐이다. 코드 생성은 **텍스트 LLVM IR**을 직접 쓴다. LLVM 라이브러리에 링크하지 않는다.
- 설명하는 소스 위치는 개발 저장소의 `src/` 기준이다. 줄 수는 대략적이며 바뀔 수 있다.

## 1. 한눈에

```
 소스(.tam) ──► Lexer ──► Parser ──► 모듈 로더/이름 해석 ──► 타입 검사 ──► 소유권 검사 ──► 빌림 검사
                (lexer)   (parser)    (modules)              (typeck)     (ownership)    (borrowck)
                                                                                              │
          LLVM IR 텍스트 ◄── 코드 생성 (codegen) ◄──────────────────────────────────────────────┘
               │
               ▼  opt::promote (스택 변수 → SSA)
               │
     ┌─────────┴───────────┬──────────────────────────────┐
     ▼ host                ▼ windows                      ▼ uefi / bios / aarch64
  clang으로 링크       llc|clang → .obj +            llc|clang → .obj → lld-link(PE) →
                       kernel32 import lib 생성 →     (bios, aarch64는) PE를 평평한 메모리 이미지로
                       lld-link                        펼쳐 부팅 형식으로 감싼다
```

한 번의 컴파일은 위에서 아래로 한 방향으로만 흐른다. 앞 단계가 **첫 번째 오류**를 내면 거기서 멈춘다(오류를 여러 개 모으지 않는다).

## 2. 단계별 설명

### 2.1 Lexer (`src/lexer.rs`)

- **줄바꿈이 문장 끝**이다. 어휘 단계에서 "이어지는 줄"을 걸러 `Newline` 토큰을 만든다. 줄 끝이 이항 연산자나 `,`, `(`, `[`, `{`이면 다음 줄로 이어지고, 다음 줄이 `.`, `)`, `]`로 시작해도 이어진다.
- 블록 주석 `/* */`는 중첩된다. 여러 줄 블록 주석은 줄바꿈 하나로 취급한다.
- UTF-8 BOM은 무시한다.

### 2.2 Parser (`src/parser.rs`, `src/ast.rs`)

- 재귀 하강 파서다. 식은 연산자 우선순위 방식(연산자 사슬)으로 읽는다.
- **깊이 한도**가 있다. 구문 중첩 200단계, 한 식의 연산자 사슬 2000개를 넘으면 스택이 넘치기 전에 `nesting is too deep` 오류를 낸다. 컴파일러 본체는 큰 스택(256MiB)을 가진 스레드에서 돈다.
- 제네릭 모호성: `name<..>`는 `(`, `::`, `{`(구조체 리터럴이 허용되는 곳) 중 하나가 뒤따를 때만 타입 인자 목록이고, 아니면 비교 연산이다.

### 2.3 모듈 로더와 이름 해석 (`src/modules.rs`)

- 파일 하나가 모듈 하나다. `use a.b`는 루트 폴더 기준 `a/b.tam`을 읽는다. `use tam.os` 같은 표준 라이브러리는 컴파일러에 **내장된 소스**(`std/*.tam`을 `include_str!`)이고, 대상(host, windows, uefi, bios, aarch64)에 맞는 파일이 골라진다.
- 모든 모듈을 이름 변경(mangling)해 **하나의 `Program`**으로 합친다. 비공개 항목 접근은 여기서 오류가 난다.
- **상수**(`const`)는 여기서 128비트로 계산해, 이름 사용을 값으로 치환한다(지역 변수가 가리면 치환하지 않는다). 순환 정의와 범위 초과를 여기서 잡는다.

### 2.4 타입 검사 (`src/typeck.rs`, `src/types.rs`)

- 모든 표현식의 타입을 AST에 기록한다. 정수 리터럴은 "기대 타입"을 따라가고 없으면 `i32`다.
- **제네릭은 단형화**로 구현된다. 제네릭 함수와 구조체는 "틀"로 따로 보관하고, 호출이나 타입 사용이 나올 때 타입 인자를 정해 **인스턴스**(`max<i32>`, `Pair<i32,f64>.swap`)를 만들어 검사한다. 쓰이지 않은 틀은 검사하지 않는다. 인스턴스 안의 오류는 "while instantiating" 위치를 함께 보여 준다. 깊이 64, 인스턴스 5000개 한도가 있다.
- 타입 인자 추론 순서: 직접 적은 것 → 인자의 타입 → 기대 타입 → 정수 리터럴 `i32`.
- 검사가 끝나면 인스턴스들이 일반 함수처럼 프로그램에 합쳐진다. 이후 단계는 제네릭을 알 필요가 없다.

### 2.5 소유권 검사 (`src/ownership.rs`)

- Copy 타입과 이동 타입을 가르고, 이동한 값의 재사용을 막는다(`use of moved value`).
- 분기와 루프를 따라 "이동했을 수도 있음" 상태를 합친다. 루프 안에서 바깥 변수를 이동하면 다음 반복 때문에 오류다.

### 2.6 빌림 검사 (`src/borrowck.rs`)

- 함수 본문을 **평가 순서의 이벤트 목록**(참조 생성, 사용, 이동, 대입 등)으로 낮춘 뒤 훑는다.
- `&`/`&mut`는 **loan**을 만들고, loan은 참조를 쥔 변수가 이후에 쓰이는 동안만 산다(NLL 방식).
- 구조체의 서로 다른 필드는 따로 빌리고, 배열 원소는 구분하지 않는다(보수적). 반환 참조의 출처는 lifetime 표기 없이 본문에서 추론한다.

### 2.7 코드 생성 (`src/codegen.rs`)

- 검사를 통과한 AST에서 **텍스트 `.ll`**을 만든다. 모든 지역 변수는 함수 `entry` 블록의 `alloca`와 `load`/`store`로 둔다.
- 정수 `+ - *`는 `llvm.sadd.with.overflow` 계열로 낮추고, 오버플로 플래그가 서면 `tam_panic` 후 `unreachable`로 가는 분기를 넣는다. 0 나누기, 경계 검사도 같은 방식이다.
- 힙은 런타임의 `tam_alloc`/`tam_realloc`/`tam_free`만 거친다. 대상마다 구현이 다르다(호스트는 libc, Windows는 Heap API, 펌웨어는 자체 할당자). 이것이 "allocator 교체"가 들어올 자리다.
- 자동 해제는 여기서 만든다. 스코프를 나가는 모든 경로(`return`, `?`, `break`, `continue`)에서 살아 있는 힙 값의 drop 호출을 넣고, 이동 여부는 변수마다 둔 런타임 플래그로 판단한다.

작은 프로그램이 이런 IR이 된다(발췌).

```console
$ cat a.tam
fn add(a: i32, b: i32) i32 { a + b }
fn main() i32 { add(40, 2) }
$ tamc emit-llvm a.tam        # a.ll 이 생긴다
$ cat a.ll
...
define i32 @add(i32 %a.arg, i32 %b.arg) {
entry:
  %t5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %a.arg, i32 %b.arg)
  %t6 = extractvalue { i32, i1 } %t5, 0
  %t7 = extractvalue { i32, i1 } %t5, 1
  br i1 %t7, label %trap.fail2, label %trap.ok1
trap.fail2:
  call void @tam_panic(ptr @.str.0.0, i64 16)
  unreachable
trap.ok1:
  ret i32 %t6
}
define i32 @main() {
entry:
  %t1 = call i32 @add(i32 40, i32 2)
  ret i32 %t1
}
```

### 2.8 IR 최적화 (`src/opt.rs`)

- 번들 도구에 LLVM의 IR 최적화기(`opt`)가 없으므로, **IR 텍스트 수준의 `mem2reg`**를 직접 구현했다. 주소가 새지 않는 스칼라 지역 변수를 SSA 값으로 올린다(지배자 트리, 지배 경계, 이름 바꾸기). 의심스러운 모양이 나오면 아무것도 바꾸지 않고 원본을 돌려준다.
- 호스트 대상은 거기에 `clang -O2`가 더 해진다.

### 2.9 링크와 대상별 마무리 (`src/main.rs`, `src/implib.rs`, `src/image.rs`)

| 대상 | 방식 |
|---|---|
| `host` | `clang`으로 `.ll`을 컴파일·링크 |
| `windows` | `.ll` → `.obj`(`llc`가 있으면 `llc`, 없으면 `clang -c`) → DLL별 import lib을 `implib.rs`가 직접 만들어(`kernel32` 등) `lld-link`로 링크. CRT, MSVC, SDK가 필요 없다 |
| `uefi` | `.obj` → `lld-link`(`/subsystem:efi_application`) → `.efi` |
| `bios` | `.obj` → `lld-link` → PE를 "평평한 메모리 이미지"로 펼쳐(`image.rs`) 부트 섹터(`src/boot_bios.bin`, 원본은 `boot/bios.asm`)와 합쳐 디스크 이미지로 |
| `aarch64` | 같은 방식으로 PE를 펼쳐 QEMU `virt -kernel`이 읽는 `Image` 헤더를 붙인다 |

`.ll`을 오브젝트로 만드는 단계는 `llc`가 우선이다. 공식 Windows LLVM 배포본에는 `llc.exe`가 없어서, 없을 때는 같은 일을 `clang -c`로 한다(`TAMC_BACKEND=clang`이면 강제). 이때 IR 최적화는 끄고(`-Xclang -disable-llvm-passes`) 백엔드 코드 생성만 쓰므로 `llc` 경로와 같은 결과를 낸다.

## 3. 프로젝트 모드 (`src/manifest.rs`, `src/project.rs`)

- `Tmake.toml`은 표준 TOML이다. 파싱은 `toml` 크레이트가 하고, `manifest.rs`가 의미를 검사해 모든 오류를 **`Tmake.toml`의 줄/열**로 보여 준다.
- 워크스페이스, 상속(`{ workspace = true }`), 경로 의존성, 프로파일, 잠금 파일(`Tmake.lock`)을 다룬다.
- **증분 캐시**: `build/<대상>/<프로파일>/.fingerprint/`에 컴파일러 버전, 대상, 최적화 수준, 모든 매니페스트와 소스의 SHA-256을 저장한다. 같으면 `Fresh`로 건너뛴다.
- 실행 파일 패키지는 모든 소스를 한 프로그램으로 합쳐 컴파일하고, lib 패키지는 검사만 한다.

## 4. 진단 (`src/diag.rs`)

모든 오류는 파일, 줄, 열, 소스 조각, 밑줄, 설명(`note`)으로 출력된다. 파일을 못 읽는 것처럼 위치가 없는 오류는 가짜 위치 없이 `error:` 한 줄이다. 제네릭 인스턴스 오류는 원인 위치와 인스턴스를 만든 호출 위치, 두 곳을 가리킨다.

## 5. 설계 원칙

1. **안전은 컴파일러의 일이다.** 검사는 모두 컴파일러 안에 있고, 생성된 코드는 단순한 IR이다. LLVM이 최적화하지만 안전성은 LLVM에 기대지 않는다.
2. **한 방향 파이프라인.** 각 단계는 앞 단계의 결과만 읽는다. 단계마다 모듈이 하나라 따로 고치고 따로 시험할 수 있다.
3. **견고함이 기능이다.** 어떤 입력에도 컴파일러가 패닉하거나 스택이 넘치지 않고, 위치가 있는 오류로 끝나야 한다.
4. **의존성을 늘리지 않는다.** LLVM 도구(`clang`, `llc`, `lld-link`)는 실행 파일로 부르고, Rust 크레이트는 둘뿐이다.
5. **호환성을 지킨다.** 에디션 1의 의미는 바꾸지 않는다([Reference 18장](reference.md#18-호환성-약속-tam-1)). 새 기능은 기존 코드의 뜻을 바꾸지 않고 **추가**한다.

## 6. 개발과 시험

```console
$ cargo build --release        # tamc 빌드
$ cargo test                   # 전체 테스트(아래)
$ python3 tools/doccheck.py docs   # 문서의 모든 코드 블록을 컴파일·실행해 검증
```

| 종류 | 위치 | 내용 |
|---|---|---|
| 프런트엔드 테스트 | `tests/frontend.rs` | Lexer, Parser, 타입 검사 |
| 통합 테스트 | `tests/integration.rs`, `m14.rs`, `m17.rs`, `m18.rs` | 컴파일하고 실행해 종료 코드·출력 비교, 오류 메시지 시험 |
| 최적화 테스트 | `tests/opt.rs` | `mem2reg`가 변수를 레지스터로 올렸는지 |
| Windows 테스트 | `tests/windows.rs` | `.exe` 생성, 임포트가 `kernel32`뿐인지, 에뮬레이터 실행 |
| 펌웨어 테스트 | `tests/firmware.rs` | QEMU로 BIOS/UEFI/AArch64 부팅 |
| 프로젝트 테스트 | `tests/tmake.rs` | `Tmake.toml`, 잠금 파일, 캐시 |
| 문서 테스트 | `tests/docs.rs` | `tools/doccheck.py` 호출 |

**퍼저**(`tools/`): 각각 `python3 tools/fuzz_expr.py [개수] [시드]` 꼴로 돌린다.

| 퍼저 | 대상 |
|---|---|
| `fuzz_expr.py` | 임의의 정수 식을 파이썬 참조 구현과 비교 |
| `fuzz_prog.py` | 변수, 배열, `if`, `for`, `while`, 함수 호출이 섞인 프로그램 |
| `fuzz_heap.py` | `String`, 구조체 안의 `String`, `Option<String>`, `Result`, `?`, 조기 `return`을 섞은 소유권상 올바른 프로그램. valgrind로 누수·이중 해제 확인. `--win`은 Windows 대상 |
| `fuzz_generic.py` | 제네릭 함수·구조체·메서드가 섞인 프로그램 |
| `fuzz_crash.py` | 예제 소스를 무작위로 망가뜨려 컴파일러가 크래시 없이 오류로 끝나는지 |

## 7. 기여하는 방법

1. 재현 가능한 최소 프로그램을 만든다(`tamc check`나 `tamc run`으로).
2. 컴파일러가 틀린 곳을 고치고, **그 프로그램을 회귀 테스트로 추가**한다.
3. `cargo test`가 전부 통과해야 하고, 문서를 건드렸다면 `tools/doccheck.py docs`도 통과해야 한다.
4. 에디션 1의 기존 의미를 바꾸는 변경은 받지 않는다. 안전하지 않은 코드를 받아들이던 버그의 수정은 예외이며 [Release Notes](changelog.md)에 적는다.

## 8. 알려진 한계 (구현)

- 오류는 첫 번째 하나만 보고한다.
- lib 패키지는 검사만 한다. 패키지별 오브젝트 파일은 만들지 않고, 실행 파일은 모든 소스를 한 번에 컴파일한다.
- 사용자가 allocator를 바꾸는 문법과 `Drop`은 아직 없다.
- 호스트 대상의 링크는 시스템 `clang`에 의존한다.
- 실제 하드웨어에서의 펌웨어 실행은 확인하지 않았다(QEMU까지).
