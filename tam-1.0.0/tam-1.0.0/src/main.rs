use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

mod project;

use tamc::codegen::Target;
use tamc::{borrowck, codegen, image, implib, modules, ownership, typeck};

const USAGE: &str = "usage: tamc <command> <file.tam> [-o output] [--target host|windows|uefi|bios|aarch64]
       tamc <command> [options]        (project mode: uses the nearest Tmake.toml)

commands:
  check       parse and type-check
  emit-llvm   write LLVM IR (<file>.ll)
  build       compile to a native executable
  run         build, run, and exit with the program's exit code

project commands (Tmake.toml):
  new <name> [--lib]   create a new package in ./<name>
  init [name] [--lib]  create Tmake.toml and src/ in the current folder
  build | run | check  options: --release  --target T  -p NAME  --workspace  --locked  -q
                       --manifest-path <Tmake.toml>; arguments after `--` go to `run`
  clean                remove the build folder
  metadata             print the resolved project as JSON
  tree                 print the dependency tree
  --version            print the tamc version

targets:
  host        (default off Windows) link with the system clang
  windows     (default on Windows) x64 .exe without CRT/MSVC/SDK: needs llc and lld-link
              next to tamc (or on PATH); tamc generates the kernel32 import library itself
  uefi        x86-64 UEFI application (.efi, boot it as EFI/BOOT/BOOTX64.EFI); needs llc and lld-link
  bios        x86-64 BIOS boot disk image (.img): boot sector + program, no OS; needs llc and lld-link
  aarch64     AArch64 bare-metal Linux `Image` (.bin) for QEMU `virt -kernel`; needs llc and lld-link";

/// 소스(와 `use`한 모듈들)를 읽어 LLVM IR 텍스트까지 만든다. 오류는 진단을 출력하고 종료한다.
fn compile(path: &Path, target: Target) -> (String, BTreeMap<String, BTreeSet<String>>) {
    let loaded = match modules::load_with(path, target) {
        Ok(l) => l,
        Err(e) => fail(&e.diag, &e.files),
    };
    frontend(loaded, target)
}

/// 불러온 프로그램을 검사하고 LLVM IR까지 만든다.
pub(crate) fn frontend(loaded: modules::Loaded, target: Target) -> (String, BTreeMap<String, BTreeSet<String>>) {
    let files = loaded.files;
    let mut program = loaded.program;
    // 대상에 맞지 않는 extern은 여기서 걸러낸다. (파싱·이름 해석 뒤라 위치를 보여줄 수 있다.)
    let imports = codegen::extern_imports(&program, target).unwrap_or_else(|d| fail(&d, &files));
    let checked = typeck::check_for(&mut program, target).unwrap_or_else(|d| fail(&d, &files));
    ownership::check(&program, &checked).unwrap_or_else(|d| fail(&d, &files));
    borrowck::check(&program, &checked).unwrap_or_else(|d| fail(&d, &files));
    (codegen::emit_for(&program, &checked, target), imports)
}

/// 라이브러리 패키지: `main` 없이 검사만 하고 코드는 만들지 않는다.
pub(crate) fn check_only(loaded: modules::Loaded, target: Target) {
    let files = loaded.files;
    let mut program = loaded.program;
    typeck::set_require_main(false);
    let checked = typeck::check_for(&mut program, target).unwrap_or_else(|d| fail(&d, &files));
    ownership::check(&program, &checked).unwrap_or_else(|d| fail(&d, &files));
    borrowck::check(&program, &checked).unwrap_or_else(|d| fail(&d, &files));
}

pub(crate) fn fail(d: &tamc::diag::Diag, files: &[(String, String)]) -> ! {
    eprint!("{}", d.render_multi(files));
    exit(1);
}

fn out_path(args: &[String], default: PathBuf) -> PathBuf {
    match args.iter().position(|a| a == "-o") {
        Some(i) => match args.get(i + 1) {
            Some(p) => PathBuf::from(p),
            None => {
                eprintln!("error: `-o` needs a path");
                exit(2);
            }
        },
        None => default,
    }
}

/// .ll을 네이티브 실행 파일로 만든다.
/// 현재는 개발용으로 PATH의 clang을 쓴다. 번들 llc + lld-link 경로는 M10에서 추가한다.
pub(crate) fn link(ll: &Path, exe: &Path) {
    let status = Command::new("clang")
        .arg(format!("-O{}", tamc::opt::level()))
        .arg("-x")
        .arg("ir")
        .arg(ll)
        .arg("-o")
        .arg(exe)
        .arg("-Wno-override-module")
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(_) => exit(1),
        Err(e) => {
            eprintln!("error: could not run `clang`: {e}");
            exit(2);
        }
    }
}

/// 번들 도구(`tamc` 옆에 설치된 llc, lld-link)를 먼저 찾고, 없으면 PATH의 이름을 쓴다.
fn tool(name: &str) -> PathBuf {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            let p = dir.join(&file);
            if p.exists() {
                return p;
            }
        }
    }
    PathBuf::from(file)
}

/// 백엔드 도구가 번들이나 PATH에 있는가.
fn have_tool(name: &str) -> bool {
    let p = tool(name);
    if p.components().count() > 1 {
        return true;
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(&p).is_file()))
        .unwrap_or(false)
}

/// .ll을 대상 triple의 오브젝트 파일로 만든다. 기본은 `llc`다. `llc`가 없는 LLVM 배포본
/// (Windows 공식 설치본)에서는 같은 일을 하는 `clang -c`로 대신한다(`TAMC_BACKEND=clang`이면 강제).
fn compile_object(triple: &str, strict_align: bool, ll: &Path, obj: &Path) {
    let use_clang = std::env::var("TAMC_BACKEND").map(|v| v == "clang").unwrap_or(false) || !have_tool("llc");
    let level = tamc::opt::level();
    if use_clang && have_tool("clang") {
        let mut c = Command::new(tool("clang"));
        c.args(["-c", "-x", "ir", "-target", triple, &format!("-O{level}"), "-Wno-override-module", "-w", "-Xclang", "-disable-llvm-passes"]);
        if strict_align {
            c.arg("-mstrict-align");
        }
        run_tool(c.arg(ll).arg("-o").arg(obj), "clang");
        return;
    }
    let mut c = Command::new(tool("llc"));
    c.args(["-filetype=obj", &format!("-O{level}"), &format!("-mtriple={triple}")]);
    if strict_align {
        c.arg("-mattr=+strict-align");
    }
    run_tool(c.arg(ll).arg("-o").arg(obj), "llc");
}

fn run_tool(cmd: &mut Command, what: &str) {
    match cmd.output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!("error: {what} failed");
            eprint!("{}", String::from_utf8_lossy(&o.stdout));
            eprint!("{}", String::from_utf8_lossy(&o.stderr));
            exit(1);
        }
        Err(e) => {
            eprintln!("error: could not run `{what}`: {e}");
            eprintln!("note: tamc looks for it next to tamc itself and then on PATH");
            exit(2);
        }
    }
}

/// .ll을 Windows x64 .exe로 만든다. llc로 .obj를 만들고, kernel32 import lib을 직접 생성해
/// lld-link로 링크한다. CRT, MSVC, Windows SDK가 필요 없다.
pub(crate) fn link_windows(ll: &Path, exe: &Path, imports: &BTreeMap<String, BTreeSet<String>>) {
    let obj = exe.with_extension("obj");
    compile_object("x86_64-pc-windows-msvc", false, ll, &obj);
    // DLL마다 import lib을 만든다. kernel32는 런타임이 쓰는 함수를 항상 포함한다.
    let mut dlls = imports.clone();
    dlls.entry("kernel32.dll".to_string())
        .or_default()
        .extend(implib::KERNEL32_RUNTIME.iter().map(|s| s.to_string()));
    let mut libs: Vec<PathBuf> = Vec::new();
    for (dll, names) in &dlls {
        let lib = exe.with_extension(format!("{}.lib", dll.trim_end_matches(".dll")));
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        std::fs::write(&lib, implib::import_lib(dll, &names)).unwrap_or_else(|e| {
            eprintln!("error: cannot write `{}`: {e}", lib.display());
            exit(2);
        });
        libs.push(lib);
    }
    let mut out_arg = std::ffi::OsString::from("/out:");
    out_arg.push(exe);
    run_tool(
        Command::new(tool("lld-link"))
            .args(["/nologo", "/machine:x64", "/subsystem:console", "/entry:mainCRTStartup", "/nodefaultlib"])
            .arg(out_arg)
            .arg(&obj)
            .args(&libs),
        "lld-link",
    );
    let _ = std::fs::remove_file(&obj);
    for lib in &libs {
        let _ = std::fs::remove_file(lib);
    }
}

/// 펌웨어 대상(UEFI/BIOS/AArch64)을 만든다. 모두 llc → lld-link(PE) 경로를 타고,
/// BIOS/AArch64는 PE를 평평한 메모리 이미지로 펼쳐 부팅 형식으로 감싼다.
pub(crate) fn link_firmware(target: Target, ll: &Path, out: &Path) {
    let obj = out.with_extension("obj");
    compile_object(target.triple().unwrap(), target == Target::Aarch64, ll, &obj);
    let pe = out.with_extension("pe.tmp");
    let mut out_arg = std::ffi::OsString::from("/out:");
    out_arg.push(if target == Target::Uefi { out } else { &pe });
    let mut link = Command::new(tool("lld-link"));
    link.args(["/nologo", "/nodefaultlib"]);
    match target {
        Target::Uefi => link.args(["/machine:x64", "/subsystem:efi_application", "/entry:efi_main"]),
        Target::Bios => link.args([
            "/machine:x64", "/subsystem:console", "/entry:bios_entry", "/base:0x10000", "/fixed",
            "/dynamicbase:no", "/largeaddressaware:no",
        ]),
        Target::Aarch64 => link.args([
            "/machine:arm64", "/subsystem:console", "/entry:tam_start", "/base:0x40080000", "/largeaddressaware:no",
        ]),
        _ => unreachable!(),
    };
    run_tool(link.arg(out_arg).arg(&obj), "lld-link");
    let _ = std::fs::remove_file(&obj);
    if target != Target::Uefi {
        let bytes = std::fs::read(&pe).unwrap_or_else(|e| {
            eprintln!("error: cannot read `{}`: {e}", pe.display());
            exit(2);
        });
        let _ = std::fs::remove_file(&pe);
        let _ = std::fs::remove_file(pe.with_extension("lib"));
        let result = image::flatten_pe(&bytes).and_then(|flat| match target {
            Target::Bios => image::bios_disk(&flat),
            _ => image::aarch64_image(&flat),
        });
        match result {
            Ok(img) => std::fs::write(out, img).unwrap_or_else(|e| {
                eprintln!("error: cannot write `{}`: {e}", out.display());
                exit(2);
            }),
            Err(e) => {
                eprintln!("error: {e}");
                exit(1);
            }
        }
    }
}

/// 만든 프로그램을 실행하고 그 종료 코드로 끝낸다. 신호로 죽으면 셸 관례대로 128 + 신호 번호다.
pub(crate) fn run_program(exe: &Path, args: &[String]) -> ! {
    let exe_abs = std::fs::canonicalize(exe).unwrap_or(exe.to_path_buf());
    let status = Command::new(exe_abs).args(args).status().ok();
    #[cfg(unix)]
    let sig = status.and_then(|s| std::os::unix::process::ExitStatusExt::signal(&s));
    #[cfg(not(unix))]
    let sig: Option<i32> = None;
    let code = match (sig, status.and_then(|s| s.code())) {
        (Some(sig), _) => 128 + sig,
        (None, Some(c)) => c,
        _ => 1,
    };
    if sig.is_some() {
        eprintln!("error: the program was terminated by signal {}", code - 128);
    }
    exit(code);
}

pub(crate) fn parse_target(args: &[String]) -> Target {
    match args.iter().position(|a| a == "--target") {
        Some(i) => match args.get(i + 1).map(String::as_str) {
            Some("host") => Target::Host,
            Some("windows") => Target::Windows,
            Some("uefi") => Target::Uefi,
            Some("bios") => Target::Bios,
            Some("aarch64") => Target::Aarch64,
            other => {
                eprintln!("error: unknown target `{}` (use `host`, `windows`, `uefi`, `bios` or `aarch64`)", other.unwrap_or(""));
                exit(2);
            }
        },
        None if cfg!(windows) => Target::Windows,
        None => Target::Host,
    }
}

fn main() {
    // 깊게 중첩된 입력에서도 재귀 단계(검사기, 코드 생성기)가 스택을 넘기지 않도록 큰 스택에서 돌린다.
    let h = std::thread::Builder::new().stack_size(256 << 20).spawn(real_main).expect("cannot start the compiler thread");
    if h.join().is_err() {
        exit(101);
    }
}

fn real_main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V" | "version") => {
            println!("tamc {}", tamc::manifest::TAMC_VERSION);
            return;
        }
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            return;
        }
        Some(c @ ("new" | "init" | "clean" | "metadata" | "tree")) => project::run(c, &args[1..]),
        Some(c @ ("build" | "run" | "check")) => {
            let second = args.get(1).map(String::as_str);
            let manifest_arg = second.filter(|s| !s.starts_with('-') && (s.ends_with("Tmake.toml") || Path::new(s).is_dir()));
            if second.map_or(true, |s| s.starts_with('-')) {
                project::run(c, &args[1..]);
            } else if let Some(m) = manifest_arg {
                let p = if m.ends_with("Tmake.toml") { PathBuf::from(m) } else { Path::new(m).join("Tmake.toml") };
                let mut rest = vec!["--manifest-path".to_string(), p.display().to_string()];
                rest.extend(args[2..].iter().cloned());
                project::run(c, &rest);
            }
        }
        _ => {}
    }
    let (Some(cmd), Some(file)) = (args.first(), args.get(1)) else {
        eprintln!("{USAGE}");
        exit(2);
    };
    let path = PathBuf::from(file);
    let target = parse_target(&args);
    let exe_default = path.with_extension(match target {
        Target::Windows => "exe",
        Target::Uefi => "efi",
        Target::Bios => "img",
        Target::Aarch64 => "bin",
        Target::Host => "",
    });

    match cmd.as_str() {
        "check" => {
            let _ = compile(&path, target);
        }
        "emit-llvm" => {
            let (ll, _) = compile(&path, target);
            let out = out_path(&args, path.with_extension("ll"));
            std::fs::write(&out, ll).unwrap_or_else(|e| {
                eprintln!("error: cannot write `{}`: {e}", out.display());
                exit(2);
            });
        }
        "build" | "run" => {
            if cmd == "run" && target == Target::Windows && !cfg!(windows) {
                eprintln!("error: cannot run a Windows executable on this machine; use `build`");
                exit(2);
            }
            if cmd == "run" && target.is_firmware() {
                eprintln!("error: `run` is not supported for {} (there is no OS to run it); use `build` and boot the result, e.g. in QEMU", target.name());
                exit(2);
            }
            let (ll, imports) = compile(&path, target);
            let exe = out_path(&args, exe_default);
            let ll_path = exe.with_extension("ll");
            std::fs::write(&ll_path, ll).unwrap_or_else(|e| {
                eprintln!("error: cannot write `{}`: {e}", ll_path.display());
                exit(2);
            });
            match target {
                Target::Host => link(&ll_path, &exe),
                Target::Windows => link_windows(&ll_path, &exe, &imports),
                _ => link_firmware(target, &ll_path, &exe),
            }
            if cmd == "run" {
                run_program(&exe, &[]);
            }
        }
        _ => {
            eprintln!("{USAGE}");
            exit(2);
        }
    }
}
