# 11. 여러 대상으로 빌드하기

같은 소스를 운영체제 위의 프로그램으로도, **운영체제 없이 부팅되는 프로그램**으로도 만들 수 있다. `--target`(또는 `Tmake.toml`의 `[build] target`)으로 고른다.

| 대상 | 산출물 | 용도 |
|---|---|---|
| `host` | 실행 파일 | 이 컴퓨터(Linux/macOS)에서 실행. 기본값 |
| `windows` | `.exe` | CRT/MSVC/SDK 없이 `kernel32`만 쓰는 x64 프로그램 |
| `uefi` | `.efi` | UEFI 펌웨어가 불러오는 응용 프로그램 |
| `bios` | `.img` | 부트 섹터가 들어간 x86-64 디스크 이미지 |
| `aarch64` | `.bin` | QEMU `virt`에서 부팅되는 AArch64 `Image` |

```console
$ tamc build main.tam --target windows -o hello.exe
$ tamc build --target bios            # 프로젝트 모드
```

`windows`, `uefi`, `bios`, `aarch64`는 `llc`와 `lld-link`가 필요하다(`tamc` 옆이나 `PATH`).

## 11.1 같은 코드, 같은 결과

`tam.os`의 `print`, `println`, `print_int`, `exit`는 **모든 대상에서 같은 API**다. 그래서 이 프로그램은 어디서나 같은 글자를 낸다.

```tam
// out: hello from tam
use tam.os

fn main() i32 {
    os::println("hello from tam")
    0
}
```

호스트에서는 표준 출력, Windows에서는 `WriteFile`, UEFI에서는 콘솔, BIOS에서는 시리얼 포트(COM1), AArch64에서는 PL011 UART로 나간다. 파일 읽기, 표준 입력, 시간 같은 **운영체제 기능은 `host`와 `windows`에서만** 있다. 펌웨어 대상에서 쓰면 컴파일 오류다.

## 11.2 BIOS 부팅 이미지

BIOS 대상은 부트 섹터와 프로그램을 합친 디스크 이미지를 만든다. 프로그램은 **운영체제 없이** 64비트 모드에서 `main`부터 시작한다. 반환값은 QEMU의 종료 장치를 통해 나간다. 페이로드는 최대 384KiB다.

```tam,boot=bios
// out: hi from bios
use tam.os

fn main() i32 {
    os::println("hi from bios")
    5
}
// exit: 5
```

QEMU에서 실행:

```console
$ tamc build main.tam --target bios -o boot.img
$ qemu-system-x86_64 -drive format=raw,file=boot.img -serial stdio \
      -display none -device isa-debug-exit,iobase=0xf4,iosize=0x04
```

`isa-debug-exit`는 종료 코드를 `(값 << 1) | 1`로 돌려주므로 `main`이 5를 반환하면 QEMU의 종료 코드는 11이다.

## 11.3 UEFI 응용

UEFI 응용은 펌웨어가 불러 준다. **`main`이 반환해도 프로세스가 끝나지 않으므로 `os::exit`로 끝내야 한다.**

```tam,boot=uefi
// out: hi from uefi
use tam.os

fn main() i32 {
    os::println("hi from uefi")
    os::exit(3)
    0
}
// exit: 3
```

실행하려면 `EFI/BOOT/BOOTX64.EFI`로 놓은 FAT 폴더를 QEMU에 주고 OVMF 펌웨어로 부팅한다.

```console
$ mkdir -p esp/EFI/BOOT
$ tamc build main.tam --target uefi -o esp/EFI/BOOT/BOOTX64.EFI
$ qemu-system-x86_64 -machine q35 -m 256 -display none \
      -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
      -drive format=raw,file=fat:rw:esp -serial stdio \
      -device isa-debug-exit,iobase=0xf4,iosize=0x04
```

## 11.4 AArch64 베어메탈

```tam,boot=aarch64
// out: hi arm
use tam.os

fn main() i32 {
    os::println("hi arm")
    4
}
// exit: 4
```

```console
$ tamc build main.tam --target aarch64 -o kernel.bin
$ qemu-system-aarch64 -machine virt -cpu cortex-a57 -m 256 \
      -display none -semihosting -kernel kernel.bin -serial stdio
```

`main`의 반환값은 semihosting으로 QEMU의 종료 코드가 된다(`-semihosting` 필요).

## 11.5 하드웨어를 직접 만지기

펌웨어 대상에서는 [10장](10-unsafe-and-systems.md)의 하드웨어 내장 함수를 쓴다. 이 예는 `tam.os`를 쓰지 않고 PL011 UART의 데이터 레지스터(`0x09000000`)에 바이트를 직접 쓴다.

```tam,boot=aarch64
// out: OK
fn put(c: u8) {
    unsafe { mmio_write8(150994944, c) }     // 0x09000000
}

fn main() i32 {
    put(79)       // 'O'
    put(75)       // 'K'
    put(10)
    0
}
```

x86에서는 `port_out8(0x3F8, c)`로 COM1에 쓴다(`tam.serial`이 이를 감싸고 있다).

## 11.6 프로젝트에서 대상 고르기

`Tmake.toml`에 대상별 설정을 둔다.

```toml
[package]
name = "kernel"
version = "0.1.0"

[build]
target = "bios"

[target.bios]
out-name = "kernel"
```

`tamc build`는 `build/bios/dev/kernel.img`를 만든다. 다른 대상은 `--target`으로 덮어쓴다. 필드 전체는 [Tmake.toml 문서](../tmake.md)를 본다.

## 11.7 대상별로 달라지는 것

| | host | windows | uefi | bios | aarch64 |
|---|:-:|:-:|:-:|:-:|:-:|
| 파일, 표준 입력, 시간 | ✔ | ✔ | ✘ | ✘ | ✘ |
| `extern` | ✔ (libc) | ✔ (DLL) | ✘ | ✘ | ✘ |
| `mmio_*`, `halt` | ✘ | ✘ | ✔ | ✔ | ✔ |
| `port_*` | ✘ | ✘ | ✔ | ✔ | ✘ |
| 힙 할당 | libc | Heap API | 자체 할당자 | 자체 할당자 | 자체 할당자 |

규칙은 하나다. **맞지 않는 기능을 쓰면 실행 중이 아니라 컴파일할 때, 위치와 함께 알려 준다.**

→ [12. 프로젝트: 줄 합계 도구](12-project.md)
