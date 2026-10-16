//! `Tmake.toml` 프로젝트 명령: new, init, build, run, check, clean, metadata, tree.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use sha2::{Digest, Sha256};
use tamc::codegen::Target;
use tamc::manifest::{self, Graph, Kind, LockEntry, MError, Manifest, MANIFEST, LOCKFILE, TARGETS};
use tamc::modules::{self, PkgCtx};

use crate::{fail, frontend, link, link_firmware, link_windows, run_program};

struct Opts {
    release: bool,
    target: Option<String>,
    package: Option<String>,
    workspace: bool,
    locked: bool,
    quiet: bool,
    manifest_path: Option<PathBuf>,
    positional: Vec<String>,
    run_args: Vec<String>,
    lib: bool,
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(2);
}

fn parse_opts(args: &[String]) -> Opts {
    let mut o = Opts {
        release: false,
        target: None,
        package: None,
        workspace: false,
        locked: false,
        quiet: false,
        manifest_path: None,
        positional: Vec::new(),
        run_args: Vec::new(),
        lib: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut value = |name: &str| -> String {
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| die(&format!("`{name}` needs a value")))
        };
        match a {
            "--release" => o.release = true,
            "--workspace" | "--all" => o.workspace = true,
            "--locked" => o.locked = true,
            "--quiet" | "-q" => o.quiet = true,
            "--lib" => o.lib = true,
            "--target" => o.target = Some(value("--target")),
            "-p" | "--package" => o.package = Some(value("-p")),
            "--manifest-path" => o.manifest_path = Some(PathBuf::from(value("--manifest-path"))),
            "--" => {
                o.run_args = args[i + 1..].to_vec();
                break;
            }
            x if x.starts_with('-') => die(&format!("unknown option `{x}` (see `tamc help`)")),
            x => o.positional.push(x.to_string()),
        }
        i += 1;
    }
    if let Some(t) = &o.target {
        if !TARGETS.contains(&t.as_str()) {
            die(&format!("unknown target `{t}` (use {})", TARGETS.join(", ")));
        }
    }
    o
}

fn merr(e: MError) -> ! {
    eprint!("{}", e.render());
    exit(1);
}

fn load(path: &Path) -> Manifest {
    Manifest::load(path).unwrap_or_else(|e| merr(e))
}

fn target_of(name: &str) -> Target {
    match name {
        "windows" => Target::Windows,
        "uefi" => Target::Uefi,
        "bios" => Target::Bios,
        "aarch64" => Target::Aarch64,
        _ => Target::Host,
    }
}

fn host_target_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "host"
    }
}

fn exe_ext(t: &str) -> &'static str {
    match t {
        "windows" => ".exe",
        "uefi" => ".efi",
        "bios" => ".img",
        "aarch64" => ".bin",
        _ => "",
    }
}

/// 가까운 `Tmake.toml`(위로 올라가며)을 찾는다.
fn find_manifest(start: &Path) -> Option<PathBuf> {
    let mut cur = std::fs::canonicalize(start).ok()?;
    loop {
        let m = cur.join(MANIFEST);
        if m.is_file() {
            return Some(m);
        }
        if !cur.pop() {
            return None;
        }
    }
}

struct Selection {
    /// 작업 공간 루트(있으면)
    root: Option<Manifest>,
    /// 빌드할 패키지들의 매니페스트
    pkgs: Vec<Manifest>,
    /// 잠금 파일과 빌드 폴더의 기준이 되는 폴더/매니페스트
    base_dir: PathBuf,
    out_dir: PathBuf,
    release: manifest::Profile,
    dev: manifest::Profile,
    /// 작업 공간의 모든 멤버 (잠금 파일용)
    all_members: Vec<PathBuf>,
}

fn select(o: &Opts) -> Selection {
    let start = match &o.manifest_path {
        Some(p) => {
            if p.file_name().map_or(true, |n| n != MANIFEST) {
                die(&format!("`--manifest-path` must point at a file named {MANIFEST}"));
            }
            p.clone()
        }
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|e| die(&format!("cannot read the current directory: {e}")));
            find_manifest(&cwd).unwrap_or_else(|| {
                eprintln!("error: could not find `{MANIFEST}` in `{}` or any parent folder", cwd.display());
                eprintln!("note: create a project with `tamc new <name>` or `tamc init`");
                exit(2);
            })
        }
    };
    let near = load(&start);
    // 작업 공간 루트: 자신이거나, 위쪽에서 자신을 멤버로 가진 매니페스트.
    let root = if near.workspace.is_some() {
        None
    } else {
        manifest::find_workspace_root(&near.dir).map(|d| load(&d.join(MANIFEST)))
    };
    let (ws_manifest_dir, ws): (PathBuf, Option<&manifest::Workspace>) = match (&root, &near.workspace) {
        (Some(r), _) => (r.dir.clone(), r.workspace.as_ref()),
        (None, Some(w)) => (near.dir.clone(), Some(w)),
        _ => (near.dir.clone(), None),
    };
    let top: &Manifest = root.as_ref().unwrap_or(&near);
    let (release, dev, out_dir) = (top.release.clone(), top.dev.clone(), top.out_dir.clone());
    let all_members: Vec<PathBuf> = ws.map(|w| w.members.clone()).unwrap_or_default();

    let mut pkgs: Vec<Manifest> = Vec::new();
    if let Some(name) = &o.package {
        let mut candidates: Vec<PathBuf> = all_members.clone();
        if near.package.is_some() {
            candidates.push(near.dir.clone());
        }
        if let Some(r) = &root {
            if r.package.is_some() {
                candidates.push(r.dir.clone());
            }
        }
        let mut seen: Vec<String> = Vec::new();
        for c in candidates {
            let m = load(&c.join(MANIFEST));
            if let Some(n) = m.package_name() {
                if n == name {
                    pkgs.push(m);
                    break;
                }
                seen.push(n.to_string());
            }
        }
        if pkgs.is_empty() {
            seen.sort();
            seen.dedup();
            die(&format!("no package named `{name}` here (available: {})", if seen.is_empty() { "none".into() } else { seen.join(", ") }));
        }
    } else if o.workspace || (near.package.is_none() && root.is_none()) {
        let members: Vec<PathBuf> = if o.workspace {
            let mut v = all_members.clone();
            let top_dir = top.dir.clone();
            if top.package.is_some() && !v.iter().any(|m| m == &top_dir) {
                v.insert(0, top_dir);
            }
            v
        } else {
            ws.map(|w| w.default_members.clone()).unwrap_or_default()
        };
        for m in members {
            pkgs.push(load(&m.join(MANIFEST)));
        }
    } else {
        pkgs.push(near);
    }
    if pkgs.is_empty() {
        die("nothing to build: the workspace has no members");
    }
    // 읽은 순서와 무관하게 이름순으로 고정한다.
    pkgs.sort_by(|a, b| a.package_name().cmp(&b.package_name()));
    Selection { root, pkgs, base_dir: ws_manifest_dir, out_dir, release, dev, all_members }
}

fn say(o: &Opts, tag: &str, msg: &str) {
    if !o.quiet {
        eprintln!("{tag:>12} {msg}");
    }
}

/// 잠금 파일을 만들거나 갱신한다. `--locked`면 바뀔 때 오류.
fn update_lock(sel: &Selection, o: &Opts) {
    let mut entries: BTreeMap<String, LockEntry> = BTreeMap::new();
    let mut members: Vec<PathBuf> = sel.all_members.clone();
    for m in &sel.pkgs {
        if !members.iter().any(|x| x == &m.dir) {
            members.push(m.dir.clone());
        }
    }
    if let Some(r) = &sel.root {
        if r.package.is_some() && !members.iter().any(|x| x == &r.dir) {
            members.push(r.dir.clone());
        }
    }
    for dir in members {
        let m = load(&dir.join(MANIFEST));
        if m.package.is_none() {
            continue;
        }
        // 모든 타겟의 의존성을 합쳐서, 어떤 타겟으로 빌드해도 같은 잠금 파일이 되게 한다.
        for t in TARGETS {
            match manifest::resolve(&m, t) {
                Ok(g) => g.collect_lock(&sel.base_dir, &mut entries),
                Err(e) => merr(e),
            }
        }
    }
    let text = manifest::lock_text(&entries);
    let path = sel.base_dir.join(LOCKFILE);
    let old = std::fs::read_to_string(&path).ok();
    if old.as_deref() == Some(text.as_str()) {
        return;
    }
    if o.locked {
        eprintln!(
            "error: `{LOCKFILE}` {} but `--locked` was given",
            if old.is_some() { "needs to be updated" } else { "does not exist" }
        );
        eprintln!("note: run `tamc build` without `--locked` and commit the result");
        exit(1);
    }
    std::fs::write(&path, text).unwrap_or_else(|e| die(&format!("cannot write `{}`: {e}", path.display())));
    say(o, "Locking", &format!("{} packages in {LOCKFILE}", entries.len()));
}

fn pkg_ctxs(g: &Graph) -> Vec<PkgCtx> {
    g.pkgs
        .iter()
        .map(|p| PkgCtx {
            use_name: manifest::use_name(&p.name),
            entry: p.entry.clone(),
            src_root: p.src_root.clone(),
            deps: p.deps.iter().cloned().collect::<HashMap<_, _>>(),
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct Built {
    exe: Option<PathBuf>,
}

fn build_one(sel: &Selection, m: &Manifest, o: &Opts, always: bool, link_it: bool) -> Built {
    let pkg = m.package.as_ref().unwrap();
    let tname: String = o
        .target
        .clone()
        .or_else(|| m.default_target.clone())
        .unwrap_or_else(|| host_target_name().to_string());
    let target = target_of(&tname);
    let g = manifest::resolve(m, &tname).unwrap_or_else(|e| merr(e));
    let profile = if o.release { &sel.release } else { &sel.dev };
    let pname = if o.release { "release" } else { "dev" };
    let started = Instant::now();
    let ctxs = pkg_ctxs(&g);
    tamc::opt::set_level(profile.opt_level);

    let loaded = modules::load_pkgs(&ctxs, target).unwrap_or_else(|e| fail(&e.diag, &e.files));

    // 지문: 컴파일러 버전, 대상, 최적화 수준, 읽은 모든 매니페스트와 소스.
    let mut h = Sha256::new();
    h.update(format!("tamc {}\ntarget {tname}\nopt {}\nkind {:?}\n", manifest::TAMC_VERSION, profile.opt_level, pkg.kind));
    let mut mans = g.manifests.clone();
    mans.sort();
    for (n, t) in &mans {
        h.update(format!("manifest {n} {}\n", t.len()));
        h.update(t.as_bytes());
    }
    for (n, t) in loaded.files.iter().filter(|f| !f.0.starts_with("<std>")) {
        h.update(format!("source {n} {}\n", t.len()));
        h.update(t.as_bytes());
    }
    let fp = hex(&h.finalize());

    let out_name = g.pkgs[0].out_name.clone().unwrap_or_else(|| pkg.name.clone());
    let dir = sel.out_dir.join(&tname).join(pname);
    let exe = dir.join(format!("{out_name}{}", exe_ext(&tname)));
    let fp_path = dir.join(".fingerprint").join(format!("{}.{}", pkg.name, if link_it { "bin" } else { "chk" }));
    let ident = format!("{} v{} ({})", pkg.name, pkg.version, manifest::relative_to(&m.dir, &std::env::current_dir().unwrap_or_default()));

    let fresh = !always
        && std::fs::read_to_string(&fp_path).ok().as_deref() == Some(fp.as_str())
        && (!link_it || pkg.kind == Kind::Lib || exe.is_file());
    if fresh {
        say(o, "Fresh", &ident);
        return Built { exe: (pkg.kind == Kind::Bin && link_it).then_some(exe) };
    }
    say(o, if link_it { "Compiling" } else { "Checking" }, &ident);
    if pkg.kind == Kind::Lib {
        crate::check_only(loaded, target);
        if let Some(parent) = fp_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&fp_path, &fp);
        say(o, "Finished", &format!("{pname} [opt-level {}] {tname} in {:.2}s", profile.opt_level, started.elapsed().as_secs_f64()));
        return Built { exe: None };
    }
    let (ll, imports) = frontend(loaded, target);
    if link_it && pkg.kind == Kind::Bin {
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("cannot create `{}`: {e}", dir.display())));
        let ll_path = exe.with_extension("ll");
        std::fs::write(&ll_path, ll).unwrap_or_else(|e| die(&format!("cannot write `{}`: {e}", ll_path.display())));
        match target {
            Target::Host => link(&ll_path, &exe),
            Target::Windows => link_windows(&ll_path, &exe, &imports),
            _ => link_firmware(target, &ll_path, &exe),
        }
        let _ = std::fs::remove_file(&ll_path);
    }
    if link_it || pkg.kind == Kind::Lib {
        if let Some(parent) = fp_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&fp_path, &fp);
    }
    let secs = started.elapsed().as_secs_f64();
    say(o, "Finished", &format!("{pname} [opt-level {}] {tname} in {secs:.2}s", profile.opt_level));
    Built { exe: (pkg.kind == Kind::Bin && link_it).then_some(exe) }
}

pub fn run(cmd: &str, args: &[String]) -> ! {
    let o = parse_opts(args);
    match cmd {
        "new" | "init" => new_project(cmd, &o),
        "clean" => {
            let sel = select(&o);
            let dir = &sel.out_dir;
            if dir.exists() {
                std::fs::remove_dir_all(dir).unwrap_or_else(|e| die(&format!("cannot remove `{}`: {e}", dir.display())));
                say(&o, "Removed", &dir.display().to_string());
            }
            exit(0);
        }
        "metadata" => {
            metadata(&select(&o), &o);
            exit(0);
        }
        "tree" => {
            tree(&select(&o), &o);
            exit(0);
        }
        "build" | "run" | "check" => {
            let sel = select(&o);
            update_lock(&sel, &o);
            let check = cmd == "check";
            if cmd == "run" {
                let bins: Vec<&Manifest> = sel.pkgs.iter().filter(|m| m.package.as_ref().map_or(false, |p| p.kind == Kind::Bin)).collect();
                if bins.len() != 1 {
                    die(&format!(
                        "`run` needs exactly one binary package, found {} (use `-p <name>`)",
                        bins.len()
                    ));
                }
                let tn = o.target.clone().or_else(|| bins[0].default_target.clone()).unwrap_or_else(|| host_target_name().into());
                if tn == "windows" && !cfg!(windows) {
                    die("cannot run a Windows executable on this machine; use `build`");
                }
                if matches!(tn.as_str(), "uefi" | "bios" | "aarch64") {
                    die(&format!("`run` is not supported for {tn} (there is no OS to run it); use `build` and boot the result, e.g. in QEMU"));
                }
                let built = build_one(&sel, bins[0], &o, false, true);
                let exe = built.exe.expect("binary output");
                say(&o, "Running", &exe.display().to_string());
                run_program(&exe, &o.run_args);
            }
            for m in &sel.pkgs {
                build_one(&sel, m, &o, check, !check);
            }
            exit(0);
        }
        _ => unreachable!("dispatched by main"),
    }
}

fn new_project(cmd: &str, o: &Opts) -> ! {
    let (dir, name) = if cmd == "new" {
        let Some(n) = o.positional.first() else { die("usage: tamc new <name> [--lib]") };
        let d = PathBuf::from(n);
        let name = d.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if d.exists() {
            die(&format!("`{}` already exists", d.display()));
        }
        (d, name)
    } else {
        let d = std::env::current_dir().unwrap_or_else(|e| die(&format!("cannot read the current directory: {e}")));
        let name = o
            .positional
            .first()
            .cloned()
            .unwrap_or_else(|| d.file_name().map(|s| s.to_string_lossy().to_lowercase().replace([' ', '.'], "-")).unwrap_or_default());
        if d.join(MANIFEST).exists() {
            die(&format!("`{MANIFEST}` already exists here"));
        }
        (d, name)
    };
    if let Err(m) = manifest::check_new_name(&name) {
        die(&m);
    }
    let w = |p: PathBuf, text: &str| {
        if p.exists() {
            return;
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| die(&format!("cannot create `{}`: {e}", parent.display())));
        }
        std::fs::write(&p, text).unwrap_or_else(|e| die(&format!("cannot write `{}`: {e}", p.display())));
    };
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("cannot create `{}`: {e}", dir.display())));
    w(dir.join(MANIFEST), &manifest::manifest_template(&name, o.lib));
    if o.lib {
        w(dir.join("src/lib.tam"), "pub fn answer() i32 {\n    42\n}\n");
    } else {
        w(dir.join("src/main.tam"), "use tam.os\n\nfn main() i32 {\n    os::println(\"Hello, TAM!\")\n    0\n}\n");
    }
    w(dir.join(".gitignore"), "/build\n");
    eprintln!("     Created {} `{name}` in {}", if o.lib { "library" } else { "binary" }, dir.display());
    exit(0);
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn metadata(sel: &Selection, o: &Opts) {
    let mut pkgs: Vec<String> = Vec::new();
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let members: Vec<PathBuf> = sel.all_members.iter().map(|m| canon(m)).collect();
    for m in &sel.pkgs {
        let tname = o.target.clone().or_else(|| m.default_target.clone()).unwrap_or_else(|| host_target_name().into());
        let g = manifest::resolve(m, &tname).unwrap_or_else(|e| merr(e));
        for (i, p) in g.pkgs.iter().enumerate() {
            let deps: Vec<String> = p.deps.iter().map(|(_, d)| json_str(&g.pkgs[*d].name)).collect();
            let s = format!(
                "{{\"name\":{},\"version\":{},\"kind\":{},\"manifest_path\":{},\"entry\":{},\"description\":{},\"dependencies\":[{}],\"workspace_member\":{}}}",
                json_str(&p.name),
                json_str(&p.version.to_string()),
                json_str(if p.kind == Kind::Bin { "bin" } else { "lib" }),
                json_str(&p.manifest_path.display().to_string()),
                json_str(&p.entry.display().to_string()),
                p.description.as_deref().map_or("null".to_string(), json_str),
                deps.join(","),
                i == 0 || members.contains(&canon(&p.dir))
            );
            if !pkgs.iter().any(|x| x.starts_with(&format!("{{\"name\":{},", json_str(&p.name)))) {
                pkgs.push(s);
            }
        }
    }
    println!(
        "{{\"tamc\":{},\"workspace_root\":{},\"build_dir\":{},\"packages\":[{}]}}",
        json_str(manifest::TAMC_VERSION),
        json_str(&sel.base_dir.display().to_string()),
        json_str(&sel.out_dir.display().to_string()),
        pkgs.join(",")
    );
}

fn tree(sel: &Selection, o: &Opts) {
    for m in &sel.pkgs {
        let tname = o.target.clone().or_else(|| m.default_target.clone()).unwrap_or_else(|| host_target_name().into());
        let g = manifest::resolve(m, &tname).unwrap_or_else(|e| merr(e));
        fn show(g: &Graph, i: usize, depth: usize, out: &mut String) {
            let p = &g.pkgs[i];
            out.push_str(&format!("{}{} v{}\n", "    ".repeat(depth), p.name, p.version));
            for (_, d) in &p.deps {
                show(g, *d, depth + 1, out);
            }
        }
        let mut s = String::new();
        show(&g, 0, 0, &mut s);
        print!("{s}");
    }
}
