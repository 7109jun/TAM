//! 모듈 로더와 이름 해석 (M8).
//!
//! - 파일 하나가 모듈 하나다. 루트 파일이 진입점이고, `use a.b`는 `<루트 폴더>/a/b.tam`을 불러온다.
//! - 다른 모듈의 항목은 `b::item`으로 쓴다 (`b`는 `use`한 경로의 마지막 조각).
//! - 다른 모듈에서 보이려면 항목에 `pub`이 있어야 한다.
//! - 해석이 끝나면 모든 모듈을 하나의 `Program`으로 합친다. 루트 항목은 이름 그대로,
//!   나머지는 `a.b::item`으로 맹글링된다. 뒤쪽 단계(typeck, borrowck, codegen)는 모듈을 모른다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::*;
use crate::codegen::Target;
use crate::diag::{Diag, Span};
use crate::parser::parse_file;
use crate::types::Ty;

/// 로드 실패. 진단이 가리키는 파일들을 함께 돌려줘서 렌더링할 수 있게 한다.
pub struct LoadError {
    pub diag: Diag,
    pub files: Vec<(String, String)>,
}

pub struct Loaded {
    pub program: Program,
    /// 파일 id -> (표시 이름, 소스)
    pub files: Vec<(String, String)>,
}

struct Module {
    key: Vec<String>,
    /// 이 모듈이 속한 패키지 번호
    pkg: usize,
    prog: Program,
    /// alias -> 모듈 인덱스
    uses: HashMap<String, usize>,
    /// 항목 이름 -> (pub 여부, 선언 위치)
    syms: HashMap<String, (bool, Span)>,
}

fn mangle(key: &[String], name: &str) -> String {
    if key.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", key.join("."))
    }
}

/// 표준 라이브러리 소스. `use tam.os`처럼 첫 조각이 `tam`이면 프로젝트 파일이 아니라 여기서 가져온다.
/// `tam.os`는 대상에 따라 구현이 다르다(같은 공개 API).
fn stdlib_source(path: &[String], target: Target) -> Option<(&'static str, &'static str)> {
    let name = path.iter().map(String::as_str).collect::<Vec<_>>().join(".");
    match (name.as_str(), target) {
        ("tam.fmt", _) => Some(("<std>/tam/fmt.tam", include_str!("../std/fmt.tam"))),
        ("tam.os", Target::Windows) => Some(("<std>/tam/os.tam", include_str!("../std/os_windows.tam"))),
        ("tam.os", Target::Host) => Some(("<std>/tam/os.tam", include_str!("../std/os_linux.tam"))),
        ("tam.os", Target::Uefi) => Some(("<std>/tam/os.tam", include_str!("../std/os_uefi.tam"))),
        ("tam.os", Target::Bios) => Some(("<std>/tam/os.tam", include_str!("../std/os_bios.tam"))),
        ("tam.os", Target::Aarch64) => Some(("<std>/tam/os.tam", include_str!("../std/os_aarch64.tam"))),
        ("tam.serial", Target::Uefi | Target::Bios) => Some(("<std>/tam/serial.tam", include_str!("../std/serial_x86.tam"))),
        _ => None,
    }
}

/// 루트 파일을 읽고 `use`를 따라 모든 모듈을 불러온다. (호스트 대상)
pub fn load(root_path: &Path) -> Result<Loaded, LoadError> {
    load_with(root_path, Target::Host)
}

/// 패키지 하나의 로드 문맥 (`Tmake.toml` 프로젝트). 0번이 루트 패키지다.
#[derive(Debug, Clone)]
pub struct PkgCtx {
    /// `use`에서 쓰는 이름 (`my-lib` -> `my_lib`)
    pub use_name: String,
    pub entry: PathBuf,
    /// 이 패키지의 지역 모듈을 찾는 폴더 (진입 파일이 있는 폴더)
    pub src_root: PathBuf,
    /// `use` 첫 조각 -> 패키지 번호
    pub deps: HashMap<String, usize>,
}

pub fn load_with(root_path: &Path, target: Target) -> Result<Loaded, LoadError> {
    let root_dir = root_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let ctx = PkgCtx { use_name: String::new(), entry: root_path.to_path_buf(), src_root: root_dir, deps: HashMap::new() };
    load_pkgs(&[ctx], target)
}

/// 패키지 그래프를 따라 모든 모듈을 불러온다. 패키지 `p`(>0)의 모듈은 `[use_name, ...]` 키를 가져서
/// 서로 다른 패키지의 같은 이름 모듈이 충돌하지 않는다.
pub fn load_pkgs(pkgs: &[PkgCtx], target: Target) -> Result<Loaded, LoadError> {
    let mut files: Vec<(String, String)> = Vec::new();
    let root_path: &Path = &pkgs[0].entry;

    let read = |p: &Path| std::fs::read_to_string(p);
    let src = match read(root_path) {
        Ok(s) => s,
        Err(e) => {
            return Err(LoadError {
                diag: Diag::new(format!("cannot read `{}`: {e}", root_path.display()), Span::default(), ""),
                files,
            })
        }
    };
    files.push((root_path.display().to_string(), src.clone()));
    let prog = match parse_file(&src, 0) {
        Ok(p) => p,
        Err(diag) => return Err(LoadError { diag, files }),
    };

    let mut mods: Vec<Module> = vec![Module {
        key: Vec::new(),
        pkg: 0,
        prog,
        uses: HashMap::new(),
        syms: HashMap::new(),
    }];
    let mut by_key: HashMap<Vec<String>, usize> = HashMap::new();
    by_key.insert(Vec::new(), 0);

    // 너비 우선으로 `use`를 따라간다.
    let mut i = 0;
    while i < mods.len() {
        let uses = mods[i].prog.uses.clone();
        for u in &uses {
            // 어느 패키지의 어떤 파일/키인가: 의존성 이름이 먼저, 아니면 지역 모듈.
            let cur_pkg = mods[i].pkg;
            let dep_pkg = if u.path[0] == "tam" { None } else { pkgs[cur_pkg].deps.get(&u.path[0]).copied() };
            let (key, pkg) = match dep_pkg {
                Some(dp) => (u.path.clone(), dp),
                None if cur_pkg == 0 || u.path[0] == "tam" => (u.path.clone(), cur_pkg),
                None => {
                    let mut k = vec![pkgs[cur_pkg].use_name.clone()];
                    k.extend(u.path.iter().cloned());
                    (k, cur_pkg)
                }
            };
            let idx = match by_key.get(&key) {
                Some(&k) => k,
                None => {
                    let (display, src) = if u.path[0] == "tam" {
                        match stdlib_source(&u.path, target) {
                            Some((d, s)) => (d.to_string(), s.to_string()),
                            None => {
                                return Err(LoadError {
                                    diag: Diag::new(
                                        format!("cannot find module `{}`", u.path.join(".")),
                                        u.span,
                                        "the standard library has `tam.os`, `tam.fmt` (and `tam.serial` on x86 firmware targets)",
                                    ),
                                    files,
                                })
                            }
                        }
                    } else {
                        let p: PathBuf = if dep_pkg.is_some() && u.path.len() == 1 {
                            pkgs[pkg].entry.clone()
                        } else {
                            let mut p = pkgs[pkg].src_root.clone();
                            for seg in u.path.iter().skip(usize::from(dep_pkg.is_some())) {
                                p.push(seg);
                            }
                            p.set_extension("tam");
                            p
                        };
                        match read(&p) {
                            Ok(s) => (p.display().to_string(), s),
                            Err(_) => {
                                return Err(LoadError {
                                    diag: Diag::new(
                                        format!("cannot find module `{}`", u.path.join(".")),
                                        u.span,
                                        format!("expected file `{}`", p.display()),
                                    ),
                                    files,
                                })
                            }
                        }
                    };
                    let fid = files.len() as u16;
                    files.push((display, src.clone()));
                    let prog = match parse_file(&src, fid) {
                        Ok(p) => p,
                        Err(diag) => return Err(LoadError { diag, files }),
                    };
                    mods.push(Module {
                        key: key.clone(),
                        pkg,
                        prog,
                        uses: HashMap::new(),
                        syms: HashMap::new(),
                    });
                    by_key.insert(key, mods.len() - 1);
                    mods.len() - 1
                }
            };
            if mods[i].uses.insert(u.alias.clone(), idx).is_some() {
                return Err(LoadError {
                    diag: Diag::new(
                        format!("module name `{}` is imported more than once", u.alias),
                        u.span,
                        "duplicate `use`",
                    ),
                    files,
                });
            }
        }
        i += 1;
    }

    // 모듈별 심볼 테이블.
    for m in &mut mods {
        for s in &m.prog.structs {
            m.syms.insert(s.name.clone(), (s.public, s.span));
        }
        for f in &m.prog.funcs {
            m.syms.insert(f.name.clone(), (f.public, f.span));
        }
    }

    // 이름 해석 + 맹글링.
    for mi in 0..mods.len() {
        let mut prog = std::mem::replace(&mut mods[mi].prog, empty_program());
        let result = Resolver { mods: &mods, me: mi }.program(&mut prog);
        if let Err(diag) = result {
            return Err(LoadError { diag, files });
        }
        mods[mi].prog = prog;
    }

    let mut out = empty_program();
    for m in mods {
        out.structs.extend(m.prog.structs);
        out.funcs.extend(m.prog.funcs);
    }
    Ok(Loaded { program: out, files })
}

fn empty_program() -> Program {
    Program {
        uses: Vec::new(),
        structs: Vec::new(),
        funcs: Vec::new(),
    }
}

struct Resolver<'a> {
    mods: &'a [Module],
    me: usize,
}

impl<'a> Resolver<'a> {
    fn program(&self, p: &mut Program) -> Result<(), Diag> {
        let key = self.mods[self.me].key.clone();
        for s in &mut p.structs {
            for f in &mut s.fields {
                self.ty(&mut f.ty)?;
            }
            s.name = mangle(&key, &s.name);
        }
        for f in &mut p.funcs {
            // extern 함수의 실제 심볼은 맹글링 전의 이름이다.
            if f.ext.is_some() && f.link.is_none() {
                f.link = Some(f.name.clone());
            }
            for a in &mut f.params {
                self.ty(&mut a.ty)?;
            }
            if let Some(r) = &mut f.ret {
                self.ty(r)?;
            }
            self.block(&mut f.body)?;
            f.name = mangle(&key, &f.name);
            if let Some(t) = &mut f.impl_ty {
                *t = mangle(&key, t);
            }
        }
        Ok(())
    }

    /// 항목 이름(`name` 또는 `alias::name`)을 맹글링된 전역 이름으로 바꾼다.
    fn name(&self, name: &str, span: Span) -> Result<String, Diag> {
        let me = &self.mods[self.me];
        let Some((alias, item)) = name.split_once("::") else {
            return Ok(if me.syms.contains_key(name) {
                mangle(&me.key, name)
            } else {
                name.to_string()
            });
        };
        // 연관 함수: 같은 모듈의 `Type::f`
        if !item.contains("::") && !me.uses.contains_key(alias) && me.syms.contains_key(alias) {
            let sym = format!("{alias}.{item}");
            return match me.syms.get(&sym) {
                Some(_) => Ok(mangle(&me.key, &sym)),
                None => Err(Diag::new(
                    format!("type `{alias}` has no associated function `{item}`"),
                    span,
                    "define it in an `impl` block",
                )),
            };
        }
        // 다른 모듈의 `module::Type::f`는 항목 이름을 `Type.f`로 바꿔 같은 규칙으로 찾는다.
        let dotted;
        let item = match item.split_once("::") {
            Some((ty, f)) if !f.contains("::") => {
                dotted = format!("{ty}.{f}");
                dotted.as_str()
            }
            Some(_) => {
                return Err(Diag::new(
                    format!("invalid path `{name}`"),
                    span,
                    "use `module::item` or `module::Type::function`",
                ))
            }
            None => item,
        };
        let Some(&ti) = me.uses.get(alias) else {
            return Err(Diag::new(
                format!("unknown module `{alias}`"),
                span,
                format!("add `use {alias}` (or the full path) at the top of this file"),
            ));
        };
        let target = &self.mods[ti];
        match target.syms.get(item) {
            None => Err(Diag::new(
                format!("module `{alias}` has no item `{item}`"),
                span,
                "not found",
            )),
            Some((false, decl)) if ti != self.me => Err(Diag::new(
                format!("`{item}` is private to module `{alias}`"),
                span,
                "private item",
            )
            .with_secondary(*decl, "declared here without `pub`")),
            Some(_) => Ok(mangle(&target.key, item)),
        }
    }

    fn ty(&self, t: &mut TypeRef) -> Result<(), Diag> {
        match &mut t.kind {
            TypeKind::Ref(_, inner) | TypeKind::Ptr(_, inner) | TypeKind::Array(inner, _) | TypeKind::Slice(inner) => self.ty(inner),
            TypeKind::Generic(_, args) => {
                for a in args {
                    self.ty(a)?;
                }
                Ok(())
            }
            TypeKind::Named(n) => {
                if Ty::from_name(n).is_none() {
                    *n = self.name(n, t.span)?;
                }
                Ok(())
            }
        }
    }

    fn block(&self, b: &mut Block) -> Result<(), Diag> {
        for s in &mut b.stmts {
            self.stmt(s)?;
        }
        if let Some(t) = &mut b.tail {
            self.expr(t)?;
        }
        Ok(())
    }

    fn stmt(&self, s: &mut Stmt) -> Result<(), Diag> {
        match s {
            Stmt::Let { ty, init, .. } => {
                if let Some(t) = ty {
                    self.ty(t)?;
                }
                self.expr(init)
            }
            Stmt::Assign { target, value, .. } => {
                self.expr(target)?;
                self.expr(value)
            }
            Stmt::Return { value, .. } => match value {
                Some(v) => self.expr(v),
                None => Ok(()),
            },
            Stmt::While { cond, body, step, .. } => {
                self.expr(cond)?;
                self.block(body)?;
                for s in step {
                    self.stmt(s)?;
                }
                Ok(())
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => Ok(()),
            Stmt::Expr(e) => self.expr(e),
        }
    }

    fn expr(&self, e: &mut Expr) -> Result<(), Diag> {
        let span = e.span;
        match &mut e.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::NoneTag
            | ExprKind::Var(_) => Ok(()),
            ExprKind::Cast(x, t) => {
                self.expr(x)?;
                self.ty(t)
            }
            ExprKind::Field(x, _)
            | ExprKind::AddrOf(_, x)
            | ExprKind::Deref(x)
            | ExprKind::Unary(_, x)
            | ExprKind::Try(x)
            | ExprKind::Ctor(_, x) => {
                self.expr(x)
            }
            ExprKind::Array(xs) => {
                for x in xs {
                    self.expr(x)?;
                }
                Ok(())
            }
            ExprKind::Repeat(x, _) => self.expr(x),
            ExprKind::Index(b, i) => {
                self.expr(b)?;
                self.expr(i)
            }
            ExprKind::Slice(b, lo, hi) => {
                self.expr(b)?;
                if let Some(x) = lo {
                    self.expr(x)?;
                }
                if let Some(x) = hi {
                    self.expr(x)?;
                }
                Ok(())
            }
            ExprKind::StructLit(name, inits) => {
                *name = self.name(name, span)?;
                for i in inits {
                    self.expr(&mut i.value)?;
                }
                Ok(())
            }
            ExprKind::Binary(_, l, r) => {
                self.expr(l)?;
                self.expr(r)
            }
            ExprKind::MethodCall(recv, _, args) => {
                self.expr(recv)?;
                for a in args {
                    self.expr(a)?;
                }
                Ok(())
            }
            ExprKind::Call(name, args) => {
                *name = self.name(name, span)?;
                for a in args {
                    self.expr(a)?;
                }
                Ok(())
            }
            ExprKind::Unsafe(b) => self.block(b),
            ExprKind::Match { scrut, ok, err } => {
                self.expr(scrut)?;
                self.block(&mut ok.body)?;
                self.block(&mut err.body)
            }
            ExprKind::If { cond, then, els } => {
                self.expr(cond)?;
                self.block(then)?;
                if let Some(b) = els {
                    self.block(b)?;
                }
                Ok(())
            }
        }
    }
}
