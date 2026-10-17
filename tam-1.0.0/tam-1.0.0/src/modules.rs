//! 모듈 로더와 이름 해석 (M8).
//!
//! - 파일 하나가 모듈 하나다. 루트 파일이 진입점이고, `use a.b`는 `<루트 폴더>/a/b.tam`을 불러온다.
//! - 다른 모듈의 항목은 `b::item`으로 쓴다 (`b`는 `use`한 경로의 마지막 조각).
//! - 다른 모듈에서 보이려면 항목에 `pub`이 있어야 한다.
//! - 해석이 끝나면 모든 모듈을 하나의 `Program`으로 합친다. 루트 항목은 이름 그대로,
//!   나머지는 `a.b::item`으로 맹글링된다. 뒤쪽 단계(typeck, borrowck, codegen)는 모듈을 모른다.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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
    // 상수: 이름이 다른 항목과 겹치지 않는지 보고, 모두 컴파일 시간에 계산해 둔다.
    for mi in 0..mods.len() {
        let consts: Vec<(String, bool, Span)> = mods[mi].prog.consts.iter().map(|c| (c.name.clone(), c.public, c.span)).collect();
        for (name, public, span) in consts {
            if mods[mi].syms.insert(name.clone(), (public, span)).is_some() {
                return Err(LoadError {
                    diag: Diag::new(format!("`{name}` is defined more than once"), span, "a constant cannot share a name with a function, struct or another constant"),
                    files,
                });
            }
        }
    }
    let mut cvals: Vec<HashMap<String, ConstInfo>> = vec![HashMap::new(); mods.len()];
    {
        let mut ev = ConstEval { mods: &mods, cache: HashMap::new(), stack: Vec::new() };
        for mi in 0..mods.len() {
            let names: Vec<(String, Span)> = mods[mi].prog.consts.iter().map(|c| (c.name.clone(), c.span)).collect();
            for (name, span) in names {
                match ev.get(mi, &name, span) {
                    Ok(info) => {
                        cvals[mi].insert(name, info);
                    }
                    Err(diag) => return Err(LoadError { diag, files }),
                }
            }
        }
    }

    // 이름 해석 + 맹글링.
    for mi in 0..mods.len() {
        let mut prog = std::mem::replace(&mut mods[mi].prog, empty_program());
        let result = Resolver {
            mods: &mods,
            me: mi,
            consts: &cvals,
            locals: RefCell::new(Vec::new()),
            tparams: RefCell::new(Vec::new()),
        }
        .program(&mut prog);
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
        consts: Vec::new(),
    }
}

#[derive(Clone, Debug)]
enum CVal {
    Int(i128),
    Float(f64),
    Bool(bool),
    Str(String),
}

#[derive(Clone, Debug)]
struct ConstInfo {
    val: CVal,
    ty: Ty,
    public: bool,
}

/// 상수식 계산기. 리터럴, 다른 상수, 산술·비트·비교·논리 연산, `as` 변환만 쓸 수 있다.
struct ConstEval<'a> {
    mods: &'a [Module],
    cache: HashMap<(usize, String), ConstInfo>,
    stack: Vec<(usize, String)>,
}

impl<'a> ConstEval<'a> {
    fn get(&mut self, mi: usize, name: &str, use_span: Span) -> Result<ConstInfo, Diag> {
        if let Some(c) = self.cache.get(&(mi, name.to_string())) {
            return Ok(c.clone());
        }
        let Some(def) = self.mods[mi].prog.consts.iter().find(|c| c.name == name) else {
            return Err(Diag::new(format!("cannot find constant `{name}`"), use_span, "not defined"));
        };
        if self.stack.contains(&(mi, name.to_string())) {
            return Err(Diag::new(format!("constant `{name}` depends on itself"), def.span, "a constant cannot be defined in terms of itself"));
        }
        let ty = match &def.ty.kind {
            TypeKind::Named(n) => Ty::from_name(n).filter(|t| t.is_int() || matches!(t, Ty::F64 | Ty::Bool | Ty::Str)),
            _ => None,
        };
        let Some(ty) = ty else {
            return Err(Diag::new(
                "a constant must be an integer, `f64`, `bool` or `str`",
                def.ty.span,
                "other types cannot be computed at compile time",
            ));
        };
        self.stack.push((mi, name.to_string()));
        let v = self.eval(mi, &def.value);
        self.stack.pop();
        let v = v?;
        let ok = match (&v, ty) {
            (CVal::Int(n), t) if t.is_int() => {
                if !t.fits(*n) {
                    return Err(Diag::new(
                        format!("constant `{name}` = {n} does not fit in `{}`", t.name()),
                        def.value.span,
                        "out of range",
                    ));
                }
                true
            }
            (CVal::Float(_), Ty::F64) | (CVal::Bool(_), Ty::Bool) | (CVal::Str(_), Ty::Str) => true,
            _ => false,
        };
        if !ok {
            let found = match v {
                CVal::Int(_) => "an integer",
                CVal::Float(_) => "a float",
                CVal::Bool(_) => "a bool",
                CVal::Str(_) => "a string",
            };
            return Err(Diag::new(
                format!("mismatched types: constant `{name}` is declared `{}` but its value is {found}", ty.name()),
                def.value.span,
                format!("expected `{}`", ty.name()),
            ));
        }
        let info = ConstInfo { val: v, ty, public: def.public };
        self.cache.insert((mi, name.to_string()), info.clone());
        Ok(info)
    }

    fn lookup(&mut self, mi: usize, name: &str, span: Span) -> Result<ConstInfo, Diag> {
        match name.split_once("::") {
            None => {
                if !self.mods[mi].prog.consts.iter().any(|c| c.name == name) {
                    return Err(Diag::new(
                        format!("`{name}` is not a constant"),
                        span,
                        "a constant's value can only use literals and other constants",
                    ));
                }
                self.get(mi, name, span)
            }
            Some((alias, item)) => {
                let Some(&ti) = self.mods[mi].uses.get(alias) else {
                    return Err(Diag::new(format!("unknown module `{alias}`"), span, format!("add `use {alias}` at the top of this file")));
                };
                if !self.mods[ti].prog.consts.iter().any(|c| c.name == item) {
                    return Err(Diag::new(format!("module `{alias}` has no constant `{item}`"), span, "not found"));
                }
                let info = self.get(ti, item, span)?;
                if !info.public && ti != mi {
                    return Err(Diag::new(format!("`{item}` is private to module `{alias}`"), span, "private item"));
                }
                Ok(info)
            }
        }
    }

    fn eval(&mut self, mi: usize, e: &Expr) -> Result<CVal, Diag> {
        let span = e.span;
        let overflow = || Diag::new("constant expression overflows", span, "the value does not fit in 128 bits");
        match &e.kind {
            ExprKind::Int(n) => Ok(CVal::Int(*n)),
            ExprKind::Float(f) => Ok(CVal::Float(*f)),
            ExprKind::Bool(b) => Ok(CVal::Bool(*b)),
            ExprKind::Str(s) => Ok(CVal::Str(s.clone())),
            ExprKind::Var(n) => Ok(self.lookup(mi, n, span)?.val),
            ExprKind::Unary(op, x) => match (op, self.eval(mi, x)?) {
                (UnOp::Neg, CVal::Int(n)) => n.checked_neg().map(CVal::Int).ok_or_else(overflow),
                (UnOp::Neg, CVal::Float(f)) => Ok(CVal::Float(-f)),
                (UnOp::Not, CVal::Bool(b)) => Ok(CVal::Bool(!b)),
                _ => Err(Diag::new("this operator cannot be used on this constant", span, "unsupported in a constant expression")),
            },
            ExprKind::Binary(op, l, r) => {
                let (a, b) = (self.eval(mi, l)?, self.eval(mi, r)?);
                use BinOp::*;
                match (a, b) {
                    (CVal::Int(x), CVal::Int(y)) => {
                        let int = |v: Option<i128>| v.map(CVal::Int).ok_or_else(overflow);
                        match op {
                            Add => int(x.checked_add(y)),
                            Sub => int(x.checked_sub(y)),
                            Mul => int(x.checked_mul(y)),
                            Div | Rem if y == 0 => Err(Diag::new("division by zero in a constant expression", span, "the divisor is 0")),
                            Div => int(x.checked_div(y)),
                            Rem => int(x.checked_rem(y)),
                            BitAnd => Ok(CVal::Int(x & y)),
                            BitOr => Ok(CVal::Int(x | y)),
                            BitXor => Ok(CVal::Int(x ^ y)),
                            Shl | Shr if !(0..127).contains(&y) => Err(Diag::new("shift amount out of range in a constant expression", span, "use 0..=126")),
                            Shl => int(x.checked_mul(1i128 << y)),
                            Shr => Ok(CVal::Int(x >> y)),
                            Eq => Ok(CVal::Bool(x == y)),
                            Ne => Ok(CVal::Bool(x != y)),
                            Lt => Ok(CVal::Bool(x < y)),
                            Le => Ok(CVal::Bool(x <= y)),
                            Gt => Ok(CVal::Bool(x > y)),
                            Ge => Ok(CVal::Bool(x >= y)),
                            _ => Err(Diag::new("this operator cannot be used in a constant expression", span, "unsupported")),
                        }
                    }
                    (CVal::Float(x), CVal::Float(y)) => match op {
                        Add => Ok(CVal::Float(x + y)),
                        Sub => Ok(CVal::Float(x - y)),
                        Mul => Ok(CVal::Float(x * y)),
                        Div => Ok(CVal::Float(x / y)),
                        Eq => Ok(CVal::Bool(x == y)),
                        Ne => Ok(CVal::Bool(x != y)),
                        Lt => Ok(CVal::Bool(x < y)),
                        Le => Ok(CVal::Bool(x <= y)),
                        Gt => Ok(CVal::Bool(x > y)),
                        Ge => Ok(CVal::Bool(x >= y)),
                        _ => Err(Diag::new("this operator cannot be used on floats in a constant expression", span, "unsupported")),
                    },
                    (CVal::Bool(x), CVal::Bool(y)) => match op {
                        And => Ok(CVal::Bool(x && y)),
                        Or => Ok(CVal::Bool(x || y)),
                        Eq => Ok(CVal::Bool(x == y)),
                        Ne => Ok(CVal::Bool(x != y)),
                        _ => Err(Diag::new("this operator cannot be used on bools in a constant expression", span, "unsupported")),
                    },
                    _ => Err(Diag::new("mismatched operand types in a constant expression", span, "both sides must be the same kind of value")),
                }
            }
            ExprKind::Cast(x, t) => {
                let v = self.eval(mi, x)?;
                let to = match &t.kind {
                    TypeKind::Named(n) => Ty::from_name(n),
                    _ => None,
                };
                match (v, to) {
                    (CVal::Int(n), Some(t)) if t.is_int() => {
                        let bits = t.bits();
                        let m = 1i128 << bits;
                        let mut w = n.rem_euclid(m);
                        if t.is_signed() && w >= m / 2 {
                            w -= m;
                        }
                        Ok(CVal::Int(w))
                    }
                    (CVal::Int(n), Some(Ty::F64)) => Ok(CVal::Float(n as f64)),
                    _ => Err(Diag::new("this conversion cannot be used in a constant expression", span, "only integer-to-integer and integer-to-`f64` casts are supported")),
                }
            }
            _ => Err(Diag::new(
                "this expression cannot be used in a constant",
                span,
                "use literals, other constants and operators",
            )),
        }
    }
}

struct Resolver<'a> {
    mods: &'a [Module],
    me: usize,
    consts: &'a [HashMap<String, ConstInfo>],
    /// 지금 함수 안에서 보이는 지역 변수 이름들(스코프별). 상수와 같은 이름이면 지역 변수가 이긴다.
    locals: RefCell<Vec<HashSet<String>>>,
    /// 지금 해석 중인 항목의 타입 매개변수. 구조체 이름보다 우선한다.
    tparams: RefCell<Vec<String>>,
}

impl<'a> Resolver<'a> {
    fn program(&self, p: &mut Program) -> Result<(), Diag> {
        let key = self.mods[self.me].key.clone();
        for s in &mut p.structs {
            *self.tparams.borrow_mut() = s.tparams.clone();
            for f in &mut s.fields {
                self.ty(&mut f.ty)?;
            }
            s.name = mangle(&key, &s.name);
        }
        for f in &mut p.funcs {
            *self.tparams.borrow_mut() = f.tparams.clone();
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
            *self.locals.borrow_mut() = vec![f.params.iter().map(|a| a.name.clone()).collect()];
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
            TypeKind::ArrayOf(inner, cname) => {
                self.ty(inner)?;
                let n = self.array_len(cname, t.span)?;
                let inner = std::mem::replace(inner, Box::new(TypeRef { kind: TypeKind::Named(String::new()), span: t.span }));
                t.kind = TypeKind::Array(inner, n);
                Ok(())
            }
            TypeKind::Generic(n, args) => {
                for a in args.iter_mut() {
                    self.ty(a)?;
                }
                if !matches!(n.as_str(), "Option" | "Result") && !self.tparams.borrow().contains(n) {
                    *n = self.name(n, t.span)?;
                }
                Ok(())
            }
            TypeKind::Named(n) => {
                if Ty::from_name(n).is_none() && !self.tparams.borrow().contains(n) {
                    *n = self.name(n, t.span)?;
                }
                Ok(())
            }
        }
    }

    fn block(&self, b: &mut Block) -> Result<(), Diag> {
        self.locals.borrow_mut().push(HashSet::new());
        let r = (|| {
            for s in &mut b.stmts {
                self.stmt(s)?;
            }
            if let Some(t) = &mut b.tail {
                self.expr(t)?;
            }
            Ok(())
        })();
        self.locals.borrow_mut().pop();
        r
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.borrow().iter().any(|s| s.contains(name))
    }

    /// 상수 이름(`N` 또는 `module::N`)을 찾는다. 지역 변수가 가리면 `None`.
    fn find_const(&self, name: &str, span: Span) -> Result<Option<&ConstInfo>, Diag> {
        match name.split_once("::") {
            None => {
                if self.is_local(name) {
                    return Ok(None);
                }
                Ok(self.consts[self.me].get(name))
            }
            Some((alias, item)) => {
                let me = &self.mods[self.me];
                let Some(&ti) = me.uses.get(alias) else { return Ok(None) };
                match self.consts[ti].get(item) {
                    Some(c) if !c.public && ti != self.me => Err(Diag::new(format!("`{item}` is private to module `{alias}`"), span, "private item")),
                    other => Ok(other),
                }
            }
        }
    }

    fn array_len(&self, name: &str, span: Span) -> Result<u64, Diag> {
        match self.find_const(name, span)? {
            Some(ConstInfo { val: CVal::Int(n), ty, .. }) if ty.is_int() => {
                if *n < 1 {
                    return Err(Diag::new("array length must be at least 1", span, format!("`{name}` is {n}")));
                }
                Ok(*n as u64)
            }
            Some(_) => Err(Diag::new(format!("`{name}` is not an integer constant"), span, "an array length must be an integer constant")),
            None => Err(Diag::new(format!("cannot find constant `{name}`"), span, "an array length must be an integer literal or a constant")),
        }
    }

    fn stmt(&self, s: &mut Stmt) -> Result<(), Diag> {
        match s {
            Stmt::Let { name, ty, init, .. } => {
                if let Some(t) = ty {
                    self.ty(t)?;
                }
                self.expr(init)?;
                if let Some(top) = self.locals.borrow_mut().last_mut() {
                    top.insert(name.clone());
                }
                Ok(())
            }
            Stmt::Assign { target, value, .. } => {
                if let ExprKind::Var(n) = &target.kind {
                    if self.find_const(n, target.span)?.is_some() {
                        return Err(Diag::new(format!("cannot assign to constant `{n}`"), target.span, "constants never change; use `mut` for a variable"));
                    }
                }
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
            | ExprKind::TypedInt(..)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::NoneTag => Ok(()),
            ExprKind::Var(name) => {
                if let Some(c) = self.find_const(name, span)? {
                    e.kind = match (&c.val, c.ty) {
                        (CVal::Int(n), t) => ExprKind::TypedInt(*n, t),
                        (CVal::Float(f), _) => ExprKind::Float(*f),
                        (CVal::Bool(b), _) => ExprKind::Bool(*b),
                        (CVal::Str(s), _) => ExprKind::Str(s.clone()),
                    };
                }
                Ok(())
            }
            ExprKind::RepeatOf(x, cname) => {
                self.expr(x)?;
                let n = self.array_len(cname, span)?;
                let inner = std::mem::replace(x, Box::new(Expr { kind: ExprKind::Int(0), span, ty: Ty::Unit }));
                e.kind = ExprKind::Repeat(inner, n);
                Ok(())
            }
            ExprKind::Generic(targs, inner) => {
                for t in targs.iter_mut() {
                    self.ty(t)?;
                }
                self.expr(inner)
            }
            ExprKind::Cast(x, t) => {
                self.expr(x)?;
                self.ty(t)
            }
            ExprKind::AddrOf(_, x) if matches!(&x.kind, ExprKind::Var(n) if self.find_const(n, x.span)?.is_some()) => {
                let ExprKind::Var(n) = &x.kind else { unreachable!() };
                Err(Diag::new(
                    format!("cannot take the address of constant `{n}`"),
                    x.span,
                    "a constant has no address; copy it into a variable first",
                ))
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
                for arm in [ok, err] {
                    self.locals.borrow_mut().push(HashSet::from([arm.bind.clone()]));
                    let r = self.block(&mut arm.body);
                    self.locals.borrow_mut().pop();
                    r?;
                }
                Ok(())
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
