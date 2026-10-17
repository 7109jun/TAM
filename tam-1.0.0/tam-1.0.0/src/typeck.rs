//! 타입 검사. 모든 표현식의 타입을 AST(`Expr::ty`)에 기록한다.
//!
//! 정수 리터럴은 "기대 타입"(변수 표기, 함수 인자, 반대편 피연산자 등)을 따라가며,
//! 정보가 없으면 `i32`다. 실수 리터럴은 항상 `f64`다. 숫자 타입 사이의 변환은 `as`로만 한다.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::*;
use crate::codegen::Target;
use crate::diag::{Diag, Span};
pub use crate::types::Ty;

pub type Sigs = HashMap<String, (Vec<Ty>, Ty)>;

const TYPE_HINT: &str = "supported types: i8 i16 i32 i64 u8 u16 u32 u64 f64 bool, structs, &T, &mut T, *const T, *mut T";

pub type Fields = Vec<(String, Ty)>;

/// 스코프가 끝날 때 해제(drop)해야 하는 타입인가.
pub fn needs_drop(ty: Ty, st: &StructTable) -> bool {
    match ty {
        Ty::String => true,
        Ty::Struct(n) => st.fields.get(n).map(|fs| fs.iter().any(|(_, t)| needs_drop(*t, st))).unwrap_or(false),
        Ty::Array(t, _) => needs_drop(*t, st),
        Ty::Result(t, e) => needs_drop(*t, st) || needs_drop(*e, st),
        _ => false,
    }
}

/// 내장 함수: (이름, 매개변수, 반환, unsafe 여부). 코드 생성이 직접 IR로 풀어 쓴다.
pub fn builtins() -> Vec<(&'static str, Vec<Ty>, Ty, bool)> {
    vec![
        ("string_from", vec![Ty::Str], Ty::String, false),
        ("string_push", vec![Ty::reference(Ty::String, true), Ty::U8], Ty::Unit, false),
        ("as_str", vec![Ty::reference(Ty::String, false)], Ty::Str, false),
        ("panic", vec![Ty::Str], Ty::Never, false),
        ("assert", vec![Ty::Bool], Ty::Unit, false),
        ("alloc", vec![Ty::U64], Ty::ptr(Ty::U8, true), true),
        ("free", vec![Ty::ptr(Ty::U8, true)], Ty::Unit, true),
        // ---- 하드웨어 접근 (펌웨어 대상 전용, 항상 unsafe) ----
        ("mmio_read8", vec![Ty::U64], Ty::U8, true),
        ("mmio_read16", vec![Ty::U64], Ty::U16, true),
        ("mmio_read32", vec![Ty::U64], Ty::U32, true),
        ("mmio_read64", vec![Ty::U64], Ty::U64, true),
        ("mmio_write8", vec![Ty::U64, Ty::U8], Ty::Unit, true),
        ("mmio_write16", vec![Ty::U64, Ty::U16], Ty::Unit, true),
        ("mmio_write32", vec![Ty::U64, Ty::U32], Ty::Unit, true),
        ("mmio_write64", vec![Ty::U64, Ty::U64], Ty::Unit, true),
        ("port_in8", vec![Ty::U16], Ty::U8, true),
        ("port_in32", vec![Ty::U16], Ty::U32, true),
        ("port_out8", vec![Ty::U16, Ty::U8], Ty::Unit, true),
        ("port_out32", vec![Ty::U16, Ty::U32], Ty::Unit, true),
        ("halt", vec![], Ty::Unit, true),
        ("semihost", vec![Ty::U64, Ty::U64], Ty::U64, true),
        ("ccall0", vec![Ty::ptr(Ty::U8, false)], Ty::U64, true),
        ("ccall1", vec![Ty::ptr(Ty::U8, false), Ty::U64], Ty::U64, true),
        ("ccall2", vec![Ty::ptr(Ty::U8, false), Ty::U64, Ty::U64], Ty::U64, true),
        ("ccall3", vec![Ty::ptr(Ty::U8, false), Ty::U64, Ty::U64, Ty::U64], Ty::U64, true),
        ("ccall4", vec![Ty::ptr(Ty::U8, false), Ty::U64, Ty::U64, Ty::U64, Ty::U64], Ty::U64, true),
        ("efi_system_table", vec![], Ty::ptr(Ty::U8, true), true),
        ("efi_image_handle", vec![], Ty::ptr(Ty::U8, true), true),
    ]
}

/// 내장 함수를 쓸 수 없는 대상이면 그 이유(쓸 수 있는 대상 설명)를 돌려준다.
pub fn builtin_unavailable(name: &str, target: Target) -> Option<&'static str> {
    use crate::codegen::Target::*;
    let (ok, only): (&[Target], &str) = if name.starts_with("mmio_") {
        (&[Uefi, Bios, Aarch64], "firmware targets (uefi, bios, aarch64)")
    } else if name.starts_with("port_") {
        (&[Uefi, Bios], "x86 firmware targets (uefi, bios)")
    } else if name == "halt" {
        (&[Uefi, Bios, Aarch64], "firmware targets (uefi, bios, aarch64)")
    } else if name == "semihost" {
        (&[Aarch64], "the aarch64 target")
    } else if name.starts_with("ccall") || name.starts_with("efi_") {
        (&[Uefi], "the uefi target")
    } else {
        return None;
    };
    if ok.contains(&target) {
        None
    } else {
        Some(only)
    }
}

/// 구조체 선언 순서를 유지하는 테이블.
#[derive(Default, Clone)]
pub struct StructTable {
    pub tys: HashMap<String, Ty>,
    pub fields: HashMap<String, Fields>,
    pub order: Vec<String>,
}

pub struct Checked {
    pub sigs: Sigs,
    pub structs: StructTable,
}

/// 배열/슬라이스의 원소가 될 수 있는 타입: 정수, f64, bool, raw 포인터, 그것들의 배열, 그것들만 가진 구조체.
/// (참조나 힙을 쥔 타입은 아직 원소로 쓸 수 없다.)
fn elem_ok(t: Ty, st: &StructTable) -> bool {
    match t {
        t if t.is_int() => true,
        Ty::F64 | Ty::Bool | Ty::Ptr(..) => true,
        Ty::Array(e, _) => elem_ok(*e, st),
        Ty::Struct(n) => st.fields.get(n).map(|fs| fs.iter().all(|(_, ft)| elem_ok(*ft, st))).unwrap_or(true),
        _ => false,
    }
}

/// 타입 안에 나타나는 배열/슬라이스의 원소 타입이 허용되는지 검사한다.
/// 배열, `&[T; N]`, `&[T]`의 원소 타입.
fn elem_of(t: Ty) -> Option<Ty> {
    match t {
        Ty::Array(e, _) => Some(*e),
        Ty::Ref(inner, _) => match inner {
            Ty::Array(e, _) | Ty::Slice(e) => Some(**e),
            _ => None,
        },
        _ => None,
    }
}

fn validate_ty(st: &StructTable, t: Ty, span: Span) -> Result<(), Diag> {
    match t {
        Ty::Array(e, _) | Ty::Slice(e) => {
            if !elem_ok(*e, st) {
                return Err(Diag::new(
                    format!("`{}` cannot be an array element type", e.name()),
                    span,
                    "elements must be integers, f64, bool, raw pointers, arrays or structs made of those",
                ));
            }
            validate_ty(st, *e, span)
        }
        Ty::Ref(e, _) | Ty::Ptr(e, _) => validate_ty(st, *e, span),
        Ty::Result(a, b) => {
            validate_ty(st, *a, span)?;
            validate_ty(st, *b, span)
        }
        _ => Ok(()),
    }
}

/// 검사 중에 바뀌는 전역 상태: 심볼 테이블과 제네릭 템플릿·인스턴스.
struct Ctx {
    sigs: Sigs,
    structs: StructTable,
    unsafe_fns: HashSet<String>,
    methods: HashSet<String>,
    fn_public: HashMap<String, bool>,
    fn_templates: HashMap<String, Rc<Func>>,
    struct_templates: HashMap<String, Rc<StructDef>>,
    /// 구조체 인스턴스 이름 -> (템플릿 이름, 타입 인자)
    inst_info: HashMap<String, (String, Vec<Ty>)>,
    /// 만들어졌지만 본문을 아직 검사하지 않은 함수 인스턴스
    queue: Vec<Pending>,
    inst_count: usize,
    struct_depth: u32,
}

struct Pending {
    func: Func,
    env: HashMap<String, Ty>,
    ret: Ty,
    /// 인스턴스를 요청한 위치
    span: Span,
    depth: u32,
}

/// 제네릭 인스턴스 수와 중첩 깊이 한계. 자기 자신을 점점 큰 타입으로 인스턴스화하는 재귀를 막는다.
const MAX_INSTANCES: usize = 5000;
const MAX_INST_DEPTH: u32 = 64;

fn inst_name(base: &str, targs: &[Ty]) -> String {
    format!("{base}<{}>", targs.iter().map(|t| t.name()).collect::<Vec<_>>().join(","))
}

/// 진단에 쓰기에는 너무 긴 인스턴스 이름을 줄인다.
fn abbrev(name: &str) -> String {
    if name.chars().count() <= 80 {
        name.to_string()
    } else {
        let head: String = name.chars().take(77).collect();
        format!("{head}...")
    }
}

fn limit_diag(span: Span) -> Diag {
    Diag::new(
        "generic instantiation limit exceeded",
        span,
        "a generic function or struct keeps instantiating itself with ever larger types",
    )
}

fn resolve_ty(ctx: &RefCell<Ctx>, env: &HashMap<String, Ty>, t: &TypeRef) -> Result<Ty, Diag> {
    match &t.kind {
        TypeKind::Ref(m, inner) => {
            if let TypeKind::Slice(elem) = &inner.kind {
                return Ok(Ty::reference(Ty::slice(resolve_ty(ctx, env, elem)?), *m));
            }
            Ok(Ty::reference(resolve_ty(ctx, env, inner)?, *m))
        }
        TypeKind::Array(elem, n) => Ok(Ty::array(resolve_ty(ctx, env, elem)?, *n)),
        TypeKind::ArrayOf(..) => unreachable!("array length constants are resolved before type checking"),
        TypeKind::Slice(_) => Err(Diag::new(
            "a slice type must be behind a reference",
            t.span,
            "write `&[T]` or `&mut [T]`",
        )),
        TypeKind::Ptr(m, inner) => Ok(Ty::ptr(resolve_ty(ctx, env, inner)?, *m)),
        TypeKind::Generic(n, args) => {
            if n == "Option" {
                if args.len() != 1 {
                    return Err(Diag::new(
                        format!("`Option` takes 1 type argument but {} were given", args.len()),
                        t.span,
                        "write `Option<T>`",
                    ));
                }
                return Ok(Ty::option(resolve_ty(ctx, env, &args[0])?));
            }
            if n == "Result" {
                if args.len() != 2 {
                    return Err(Diag::new(
                        format!("`Result` takes 2 type arguments but {} were given", args.len()),
                        t.span,
                        "write `Result<T, E>`",
                    ));
                }
                return Ok(Ty::result(resolve_ty(ctx, env, &args[0])?, resolve_ty(ctx, env, &args[1])?));
            }
            if !ctx.borrow().struct_templates.contains_key(n.as_str()) {
                return Err(Diag::new(
                    format!("unknown generic type `{n}`"),
                    t.span,
                    "the generic types are `Result<T, E>`, `Option<T>` and your own `struct Name<T>`",
                ));
            }
            let targs = args.iter().map(|a| resolve_ty(ctx, env, a)).collect::<Result<Vec<_>, _>>()?;
            instantiate_struct(ctx, n, targs, t.span)
        }
        TypeKind::Named(n) => {
            if let Some(t) = env.get(n.as_str()) {
                return Ok(*t);
            }
            if let Some(t) = Ty::from_name(n) {
                return Ok(t);
            }
            let c = ctx.borrow();
            if let Some(t) = c.structs.tys.get(n.as_str()) {
                return Ok(*t);
            }
            if c.struct_templates.contains_key(n.as_str()) {
                return Err(Diag::new(
                    format!("generic struct `{n}` needs type arguments"),
                    t.span,
                    format!("write `{n}<...>`"),
                ));
            }
            Err(Diag::new(format!("unknown type `{n}`"), t.span, TYPE_HINT))
        }
    }
}

/// `Pair<i32, f64>` 같은 구조체 인스턴스를 (없으면) 만든다.
fn instantiate_struct(ctx: &RefCell<Ctx>, tname: &str, targs: Vec<Ty>, span: Span) -> Result<Ty, Diag> {
    let sd = ctx.borrow().struct_templates.get(tname).cloned().expect("struct template");
    if targs.len() != sd.tparams.len() {
        return Err(Diag::new(
            format!("`{}` takes {} type argument(s) but {} were given", tname.rsplit("::").next().unwrap_or(tname), sd.tparams.len(), targs.len()),
            span,
            format!("write `{}<{}>`", tname.rsplit("::").next().unwrap_or(tname), sd.tparams.join(", ")),
        ));
    }
    let name = inst_name(tname, &targs);
    if let Some(t) = ctx.borrow().structs.tys.get(&name) {
        return Ok(*t);
    }
    {
        let c = ctx.borrow();
        if c.inst_count >= MAX_INSTANCES || c.struct_depth >= MAX_INST_DEPTH {
            return Err(limit_diag(span));
        }
    }
    let ty = Ty::strukt(&name);
    {
        let mut c = ctx.borrow_mut();
        c.structs.tys.insert(name.clone(), ty);
        c.structs.order.push(name.clone());
        c.inst_info.insert(name.clone(), (tname.to_string(), targs.clone()));
        c.inst_count += 1;
        c.struct_depth += 1;
    }
    let env: HashMap<String, Ty> = sd.tparams.iter().cloned().zip(targs.iter().copied()).collect();
    let mut fields: Fields = Vec::new();
    let mut failed = None;
    for f in &sd.fields {
        match resolve_ty(ctx, &env, &f.ty) {
            Ok(t) => fields.push((f.name.clone(), t)),
            Err(e) => {
                failed = Some(e);
                break;
            }
        }
    }
    ctx.borrow_mut().struct_depth -= 1;
    if let Some(e) = failed {
        return Err(e.with_secondary(span, format!("while instantiating `{}`", abbrev(&name))));
    }
    ctx.borrow_mut().structs.fields.insert(name.clone(), fields.clone());
    for (fd, (_, t)) in sd.fields.iter().zip(&fields) {
        validate_ty(&ctx.borrow().structs, *t, fd.ty.span).map_err(|e| e.with_secondary(span, format!("while instantiating `{}`", abbrev(&name))))?;
    }
    // 가장 바깥 인스턴스가 완성되면 자기 자신을 값으로 포함하는지 바로 확인한다.
    let outermost = ctx.borrow().struct_depth == 0;
    if outermost {
        let c = ctx.borrow();
        let mut stack = vec![name.clone()];
        if contains_by_value(&c.structs, &name, &mut stack) {
            return Err(Diag::new(
                format!("struct `{}` contains itself by value", abbrev(&name)),
                sd.span,
                "use a reference (`&T`) or a raw pointer to break the cycle",
            )
            .with_secondary(span, format!("while instantiating `{}`", abbrev(&name))));
        }
    }
    Ok(ty)
}

/// 제네릭 함수(또는 제네릭 `impl`의 메서드)의 인스턴스를 (없으면) 만들고 그 이름을 돌려준다.
fn instantiate_fn(ctx: &RefCell<Ctx>, tname: &str, targs: Vec<Ty>, span: Span, depth: u32) -> Result<String, Diag> {
    let tmpl = ctx.borrow().fn_templates.get(tname).cloned().expect("function template");
    let name = match &tmpl.impl_ty {
        Some(it) if tmpl.impl_tparams > 0 => format!("{}.{}", inst_name(it, &targs), &tmpl.name[it.len() + 1..]),
        _ => inst_name(&tmpl.name, &targs),
    };
    if ctx.borrow().sigs.contains_key(&name) {
        return Ok(name);
    }
    {
        let c = ctx.borrow();
        if c.inst_count >= MAX_INSTANCES || depth >= MAX_INST_DEPTH {
            return Err(limit_diag(span));
        }
    }
    ctx.borrow_mut().inst_count += 1;
    let env: HashMap<String, Ty> = tmpl.tparams.iter().cloned().zip(targs.iter().copied()).collect();
    let note = |e: Diag| e.with_secondary(span, format!("while instantiating `{}`", abbrev(&name)));
    let mut params = Vec::new();
    for a in &tmpl.params {
        let t = resolve_ty(ctx, &env, &a.ty).map_err(note)?;
        validate_ty(&ctx.borrow().structs, t, a.ty.span).map_err(note)?;
        params.push(t);
    }
    let ret = match &tmpl.ret {
        Some(r) => {
            let t = resolve_ty(ctx, &env, r).map_err(note)?;
            validate_ty(&ctx.borrow().structs, t, r.span).map_err(note)?;
            t
        }
        None => Ty::Unit,
    };
    let mut c = ctx.borrow_mut();
    c.sigs.insert(name.clone(), (params, ret));
    if tmpl.unsafe_ {
        c.unsafe_fns.insert(name.clone());
    }
    if tmpl.impl_ty.is_some() && tmpl.params.first().is_some_and(|a| a.name == "self") {
        c.methods.insert(name.clone());
    }
    c.fn_public.insert(name.clone(), tmpl.public);
    let mut func = (*tmpl).clone();
    func.name = name.clone();
    func.tparams.clear();
    func.impl_tparams = 0;
    c.queue.push(Pending { func, env, ret, span, depth: depth + 1 });
    Ok(name)
}

/// 타입 매개변수가 들어 있는 `pat`을 실제 타입 `actual`에 맞춰 `bind`를 채운다.
/// 맞지 않는 부분은 건드리지 않는다. 그 불일치는 인스턴스를 만든 뒤 일반 타입 검사가 보고한다.
fn unify(ctx: &RefCell<Ctx>, pat: &TypeRef, actual: Ty, tparams: &[String], bind: &mut [Option<Ty>]) {
    match &pat.kind {
        TypeKind::Named(n) => {
            if let Some(i) = tparams.iter().position(|p| p == n) {
                if bind[i].is_none() && !matches!(actual, Ty::Never | Ty::Unit | Ty::NoneTag) {
                    bind[i] = Some(actual);
                }
            }
        }
        TypeKind::Ref(_, inner) => {
            if let Ty::Ref(t, _) = actual {
                unify(ctx, inner, *t, tparams, bind);
            }
        }
        TypeKind::Ptr(_, inner) => {
            if let Ty::Ptr(t, _) = actual {
                unify(ctx, inner, *t, tparams, bind);
            }
        }
        TypeKind::Array(inner, _) | TypeKind::Slice(inner) => {
            if let Ty::Array(e, _) | Ty::Slice(e) = actual {
                unify(ctx, inner, *e, tparams, bind);
            }
        }
        TypeKind::ArrayOf(..) => {}
        TypeKind::Generic(n, args) => match n.as_str() {
            "Option" => {
                if let (Ty::Result(t, Ty::NoneTag), [a]) = (actual, args.as_slice()) {
                    unify(ctx, a, *t, tparams, bind);
                }
            }
            "Result" => {
                if let (Ty::Result(t, e), [a, b]) = (actual, args.as_slice()) {
                    if *e != Ty::NoneTag {
                        unify(ctx, a, *t, tparams, bind);
                        unify(ctx, b, *e, tparams, bind);
                    }
                }
            }
            _ => {
                if let Ty::Struct(sn) = actual {
                    let info = ctx.borrow().inst_info.get(sn).cloned();
                    if let Some((tn, targs)) = info {
                        if tn == *n {
                            for (a, t) in args.iter().zip(targs) {
                                unify(ctx, a, t, tparams, bind);
                            }
                        }
                    }
                }
            }
        },
    }
}

fn collect_struct_cycle_check(table: &StructTable, order: &[String], spans: &HashMap<String, Span>) -> Result<(), Diag> {
    for name in order {
        let mut stack = vec![name.clone()];
        if contains_by_value(table, name, &mut stack) {
            return Err(Diag::new(
                format!("struct `{name}` contains itself by value"),
                spans.get(name).copied().unwrap_or_default(),
                "use a reference (`&T`) to break the cycle",
            ));
        }
    }
    Ok(())
}

fn contains_by_value(table: &StructTable, name: &str, stack: &mut Vec<String>) -> bool {
    let Some(fields) = table.fields.get(name) else { return false };
    for (_, ty) in fields {
        if let Ty::Struct(inner) = ty {
            if stack.iter().any(|s| s == inner) {
                return true;
            }
            stack.push(inner.to_string());
            if contains_by_value(table, inner, stack) {
                return true;
            }
            stack.pop();
        }
    }
    false
}

thread_local! {
    static REQUIRE_MAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// 라이브러리 패키지는 `main`이 없어도 검사할 수 있어야 한다.
pub fn set_require_main(v: bool) {
    REQUIRE_MAIN.with(|c| c.set(v));
}

fn require_main() -> bool {
    REQUIRE_MAIN.with(|c| c.get())
}

pub fn check(p: &mut Program) -> Result<Checked, Diag> {
    check_for(p, Target::Host)
}

pub fn check_for(p: &mut Program, target: Target) -> Result<Checked, Diag> {
    // 제네릭(템플릿)은 따로 떼어 둔다. 사용할 때 인스턴스를 만들어 검사한다.
    let mut fn_templates: HashMap<String, Rc<Func>> = HashMap::new();
    let mut plain_fns: Vec<Func> = Vec::new();
    for f in std::mem::take(&mut p.funcs) {
        if f.tparams.is_empty() {
            plain_fns.push(f);
            continue;
        }
        if f.ext.is_some() {
            return Err(Diag::new("an `extern` function cannot be generic", f.span, "C functions have no type parameters"));
        }
        if f.name == "main" {
            return Err(Diag::new("`main` cannot be generic", f.span, "wrong signature"));
        }
        if fn_templates.contains_key(&f.name) {
            return Err(Diag::new(format!("function `{}` is defined more than once", f.name), f.span, "duplicate definition"));
        }
        fn_templates.insert(f.name.clone(), Rc::new(f));
    }
    p.funcs = plain_fns;
    let mut struct_templates: HashMap<String, Rc<StructDef>> = HashMap::new();
    let mut plain_structs: Vec<StructDef> = Vec::new();
    for sd in std::mem::take(&mut p.structs) {
        if sd.tparams.is_empty() {
            plain_structs.push(sd);
            continue;
        }
        if struct_templates.contains_key(&sd.name) || Ty::from_name(&sd.name).is_some() {
            return Err(Diag::new(format!("type `{}` is defined more than once", sd.name), sd.span, "duplicate definition"));
        }
        for (i, f) in sd.fields.iter().enumerate() {
            if sd.fields[..i].iter().any(|g| g.name == f.name) {
                return Err(Diag::new(
                    format!("field `{}` is defined more than once in `{}`", f.name, sd.name),
                    f.span,
                    "duplicate field",
                ));
            }
        }
        struct_templates.insert(sd.name.clone(), Rc::new(sd));
    }
    p.structs = plain_structs;

    let ctx = RefCell::new(Ctx {
        sigs: Sigs::new(),
        structs: StructTable::default(),
        unsafe_fns: HashSet::new(),
        methods: HashSet::new(),
        fn_public: HashMap::new(),
        fn_templates,
        struct_templates,
        inst_info: HashMap::new(),
        queue: Vec::new(),
        inst_count: 0,
        struct_depth: 0,
    });
    let no_env: HashMap<String, Ty> = HashMap::new();

    // 구조체: 이름을 먼저 모두 등록한 뒤 필드를 해석한다.
    for sd in &p.structs {
        let mut c = ctx.borrow_mut();
        if c.structs.tys.contains_key(&sd.name) || c.struct_templates.contains_key(&sd.name) || Ty::from_name(&sd.name).is_some() {
            return Err(Diag::new(format!("type `{}` is defined more than once", sd.name), sd.span, "duplicate definition"));
        }
        c.structs.tys.insert(sd.name.clone(), Ty::strukt(&sd.name));
        c.structs.order.push(sd.name.clone());
    }
    for sd in &p.structs {
        let mut fields: Fields = Vec::new();
        for f in &sd.fields {
            if fields.iter().any(|(n, _)| *n == f.name) {
                return Err(Diag::new(
                    format!("field `{}` is defined more than once in `{}`", f.name, sd.name),
                    f.span,
                    "duplicate field",
                ));
            }
            fields.push((f.name.clone(), resolve_ty(&ctx, &no_env, &f.ty)?));
        }
        ctx.borrow_mut().structs.fields.insert(sd.name.clone(), fields);
    }

    // `impl`은 같은 파일의 구조체에만 붙는다. 제네릭 구조체는 `impl Name<A, B>`로 쓴다.
    {
        let c = ctx.borrow();
        let all_impl: Vec<&Func> = p.funcs.iter().chain(c.fn_templates.values().map(|f| &**f)).collect();
        for f in all_impl {
            let Some(t) = &f.impl_ty else { continue };
            let short = t.rsplit("::").next().unwrap_or(t);
            match c.struct_templates.get(t.as_str()) {
                Some(sd) => {
                    if f.impl_tparams != sd.tparams.len() {
                        return Err(Diag::new(
                            format!("generic struct `{short}` has {} type parameter(s)", sd.tparams.len()),
                            f.span,
                            format!("write `impl {short}<{}>`", sd.tparams.join(", ")),
                        ));
                    }
                }
                None => {
                    if f.impl_tparams > 0 {
                        return Err(Diag::new(
                            format!("struct `{short}` is not generic"),
                            f.span,
                            format!("write `impl {short}` without type parameters"),
                        ));
                    }
                    if !c.structs.tys.contains_key(t.as_str()) {
                        return Err(Diag::new(
                            format!("cannot find type `{short}` for this `impl` block"),
                            f.span,
                            "`impl` is only allowed for structs defined in the same file",
                        ));
                    }
                }
            }
        }
    }

    // 일반 함수의 서명.
    for f in &p.funcs {
        if ctx.borrow().sigs.contains_key(&f.name) || ctx.borrow().fn_templates.contains_key(&f.name) {
            return Err(Diag::new(
                format!("function `{}` is defined more than once", f.name),
                f.span,
                "duplicate definition",
            ));
        }
        let params = f
            .params
            .iter()
            .map(|a| resolve_ty(&ctx, &no_env, &a.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let ret = match &f.ret {
            Some(t) => resolve_ty(&ctx, &no_env, t)?,
            None => Ty::Unit,
        };
        ctx.borrow_mut().sigs.insert(f.name.clone(), (params, ret));
    }
    let mut builtin_unsafe: Vec<String> = Vec::new();
    for (name, params, ret, uns) in builtins() {
        let mut c = ctx.borrow_mut();
        if c.sigs.contains_key(name) || c.fn_templates.contains_key(name) {
            let span = p
                .funcs
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.span)
                .or_else(|| c.fn_templates.get(name).map(|f| f.span))
                .unwrap_or_default();
            return Err(Diag::new(
                format!("`{name}` is a built-in function and cannot be redefined"),
                span,
                "choose another name",
            ));
        }
        c.sigs.insert(name.to_string(), (params, ret));
        if uns {
            builtin_unsafe.push(name.to_string());
        }
    }
    let main_sig = ctx.borrow().sigs.get("main").cloned();
    match main_sig {
        None if !require_main() => {}
        None => {
            return Err(Diag::new(
                "no `main` function found",
                Span::new(1, 1, 1),
                "define `fn main() i32`",
            ))
        }
        Some((params, ret)) => {
            let f = p.funcs.iter().find(|f| f.name == "main").unwrap();
            if !params.is_empty() || ret != Ty::I32 {
                return Err(Diag::new(
                    "`main` must have the signature `fn main() i32`",
                    f.span,
                    "wrong signature",
                ));
            }
        }
    }
    // extern 함수는 C ABI로 부르므로 정수, 실수, bool, raw 포인터만 주고받을 수 있다. 호출은 항상 unsafe다.
    for f in p.funcs.iter().filter(|f| f.ext.is_some()) {
        let (params, ret) = ctx.borrow().sigs[&f.name].clone();
        let ffi_ok = |t: &Ty| t.is_numeric() || matches!(t, Ty::Bool | Ty::Ptr(..));
        for (a, t) in f.params.iter().zip(&params) {
            if !ffi_ok(t) {
                return Err(Diag::new(
                    format!("extern function `{}` cannot take `{}`", f.name, t.name()),
                    a.ty.span,
                    "extern parameters must be integers, f64, bool or raw pointers",
                ));
            }
        }
        if ret != Ty::Unit && !ffi_ok(&ret) {
            return Err(Diag::new(
                format!("extern function `{}` cannot return `{}`", f.name, ret.name()),
                f.ret.as_ref().map(|r| r.span).unwrap_or(f.span),
                "extern results must be integers, f64, bool, raw pointers or nothing",
            ));
        }
    }
    {
        let c = ctx.borrow();
        for sd in &p.structs {
            for fd in &sd.fields {
                validate_ty(&c.structs, c.structs.fields[sd.name.as_str()].iter().find(|(n, _)| *n == fd.name).unwrap().1, fd.ty.span)?;
            }
        }
        for f in &p.funcs {
            let (ptys, ret) = &c.sigs[&f.name];
            for (pt, pd) in ptys.iter().zip(&f.params) {
                validate_ty(&c.structs, *pt, pd.ty.span)?;
            }
            validate_ty(&c.structs, *ret, f.ret.as_ref().map(|r| r.span).unwrap_or(f.span))?;
        }
    }
    {
        let mut c = ctx.borrow_mut();
        c.unsafe_fns.extend(p.funcs.iter().filter(|f| f.unsafe_ || f.ext.is_some()).map(|f| f.name.clone()));
        c.unsafe_fns.extend(builtin_unsafe);
        c.methods.extend(
            p.funcs
                .iter()
                .filter(|f| f.impl_ty.is_some() && f.params.first().map(|a| a.name == "self").unwrap_or(false))
                .map(|f| f.name.clone()),
        );
        c.fn_public.extend(p.funcs.iter().map(|f| (f.name.clone(), f.public)));
    }
    for f in p.funcs.iter_mut().filter(|f| f.ext.is_none()) {
        let cur_name = f.name.clone();
        let ret = ctx.borrow().sigs[&f.name].1;
        Checker {
            ctx: &ctx,
            env: HashMap::new(),
            pending_targs: Vec::new(),
            depth: 0,
            target,
            slice_ok: false,
            cur_fn: &cur_name,
            unsafe_depth: if f.unsafe_ { 1 } else { 0 },
            scopes: Vec::new(),
            ret,
            loops: 0,
        }
        .func(f)?;
    }
    // 검사 중에 요청된 제네릭 함수 인스턴스를 모두 검사한다. (검사 중에 새 인스턴스가 더 생길 수 있다.)
    let mut done: Vec<Func> = Vec::new();
    loop {
        let next = ctx.borrow_mut().queue.pop();
        let Some(mut pend) = next else { break };
        let cur_name = pend.func.name.clone();
        let unsafe_depth = if pend.func.unsafe_ { 1 } else { 0 };
        Checker {
            ctx: &ctx,
            env: pend.env.clone(),
            pending_targs: Vec::new(),
            depth: pend.depth,
            target,
            slice_ok: false,
            cur_fn: &cur_name,
            unsafe_depth,
            scopes: Vec::new(),
            ret: pend.ret,
            loops: 0,
        }
        .func(&mut pend.func)
        .map_err(|d| d.with_secondary(pend.span, format!("while instantiating `{}`", abbrev(&cur_name))))?;
        done.push(pend.func);
    }
    let ctx = ctx.into_inner();
    // 값으로 자기 자신을 포함하면 크기가 무한하다. (참조를 통한 포함은 허용)
    let mut spans: HashMap<String, Span> = p.structs.iter().map(|s| (s.name.clone(), s.span)).collect();
    for (inst, (tn, _)) in &ctx.inst_info {
        if let Some(t) = ctx.struct_templates.get(tn) {
            spans.insert(inst.clone(), t.span);
        }
    }
    collect_struct_cycle_check(&ctx.structs, &ctx.structs.order, &spans)?;
    p.funcs.extend(done);
    Ok(Checked { sigs: ctx.sigs, structs: ctx.structs })
}

struct Var {
    ty: Ty,
    mutable: bool,
}

struct Checker<'a> {
    ctx: &'a RefCell<Ctx>,
    /// 지금 검사하는 제네릭 인스턴스의 타입 매개변수 -> 실제 타입
    env: HashMap<String, Ty>,
    /// `f<T>(..)`의 명시적 타입 인자. 바로 다음 호출/구조체 리터럴이 가져간다.
    pending_targs: Vec<Ty>,
    /// 제네릭 인스턴스 중첩 깊이
    depth: u32,
    target: Target,
    /// `&`가 슬라이스 식(`a[..]`)을 바로 빌리는 중인가.
    slice_ok: bool,
    /// 지금 검사하는 함수의 (맹글링된) 이름
    cur_fn: &'a str,
    /// 0이면 safe 문맥. `unsafe fn` 본문이나 `unsafe { }` 안에서는 1 이상.
    unsafe_depth: u32,
    scopes: Vec<HashMap<String, Var>>,
    ret: Ty,
    /// 지금 감싸고 있는 `while`/`for` 수
    loops: u32,
}

/// `found` 값을 `expected` 타입 자리에 쓸 수 있는가.
/// 같은 타입이거나, 발산(`!`)이거나, `&mut T`를 `&T`로 넘기는 경우다.
fn accepts(expected: Ty, found: Ty) -> bool {
    match (expected, found) {
        (e, f) if e == f => true,
        (_, Ty::Never) => true,
        (Ty::Ref(a, false), Ty::Ref(b, true)) => a == b,
        _ => false,
    }
}

fn mismatch(expected: Ty, found: Ty, span: Span) -> Diag {
    Diag::new(
        format!(
            "mismatched types: expected `{}`, found `{}`",
            expected.name(),
            found.name()
        ),
        span,
        format!("expected `{}`", expected.name()),
    )
}

/// 반대편 타입을 따라가도 되는 리터럴인가 (`5`, `-5`).
fn is_lit(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) => true,
        ExprKind::Unary(UnOp::Neg, inner) => matches!(inner.kind, ExprKind::Int(_)),
        _ => false,
    }
}

/// 블록 안에서 이 블록을 감싸는 반복문을 끝내는 `break`가 있는가(안쪽 반복문의 `break`는 세지 않는다).
fn block_has_break(b: &Block) -> bool {
    fn expr_has(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::If { then, els, .. } => block_has_break(then) || els.as_ref().is_some_and(block_has_break),
            ExprKind::Match { ok, err, .. } => block_has_break(&ok.body) || block_has_break(&err.body),
            ExprKind::Unsafe(b) => block_has_break(b),
            _ => false,
        }
    }
    b.stmts.iter().any(|s| match s {
        Stmt::Break { .. } => true,
        Stmt::Expr(e) => expr_has(e),
        Stmt::Let { init, .. } => expr_has(init),
        Stmt::Assign { target, value, .. } => expr_has(target) || expr_has(value),
        Stmt::Return { value: Some(v), .. } => expr_has(v),
        Stmt::While { .. } | Stmt::Continue { .. } | Stmt::Return { value: None, .. } => false,
    }) || b.tail.as_ref().is_some_and(|t| expr_has(t))
}

fn op_text(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Rem => "%",
        Eq => "==",
        Ne => "!=",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        And => "&&",
        Or => "||",
        WrapAdd => "wrapping_add",
        WrapSub => "wrapping_sub",
        WrapMul => "wrapping_mul",
        BitAnd => "&",
        BitOr => "|",
        BitXor => "^",
        Shl => "<<",
        Shr => ">>",
    }
}

impl<'a> Checker<'a> {
    fn res(&self, t: &TypeRef) -> Result<Ty, Diag> {
        let ty = resolve_ty(self.ctx, &self.env, t)?;
        validate_ty(&self.ctx.borrow().structs, ty, t.span)?;
        Ok(ty)
    }

    /// 이 인자는 먼저 타입을 알아내려 하지 않고 나중에 기대 타입으로 정한다.
    fn deferred(a: &Expr) -> bool {
        is_lit(a)
            || matches!(a.kind, ExprKind::Ctor(..) | ExprKind::NoneTag)
            || matches!(&a.kind, ExprKind::Var(n) if n == "None")
            || matches!(&a.kind, ExprKind::Call(n, _) if matches!(n.as_str(), "Ok" | "Err" | "Some"))
    }

    /// 제네릭 함수 호출의 타입 인자를 정한다. 우선순위: 명시한 것, 인자 타입, 기대하는 반환 타입, 정수 리터럴(`i32`).
    fn infer_fn_targs(&mut self, tname: &str, explicit: Vec<Ty>, args: &mut [Expr], expected: Option<Ty>, span: Span) -> Result<Vec<Ty>, Diag> {
        let tmpl = self.ctx.borrow().fn_templates.get(tname).cloned().expect("function template");
        let short = tname.rsplit("::").next().unwrap_or(tname).to_string();
        let tparams = tmpl.tparams.clone();
        if explicit.len() > tparams.len() {
            return Err(Diag::new(
                format!("`{short}` takes {} type argument(s) but {} were given", tparams.len(), explicit.len()),
                span,
                "too many type arguments",
            ));
        }
        if args.len() != tmpl.params.len() {
            return Err(Diag::new(
                format!("`{tname}` takes {} argument(s) but {} were given", tmpl.params.len(), args.len()),
                span,
                "wrong number of arguments",
            ));
        }
        let mut bind: Vec<Option<Ty>> = vec![None; tparams.len()];
        for (i, t) in explicit.iter().enumerate() {
            bind[i] = Some(*t);
        }
        for (p, a) in tmpl.params.iter().zip(args.iter_mut()) {
            if bind.iter().all(Option::is_some) {
                break;
            }
            if Self::deferred(a) {
                continue;
            }
            let at = self.expr(a, None)?;
            unify(self.ctx, &p.ty, at, &tparams, &mut bind);
        }
        if let (Some(exp), Some(r)) = (expected, &tmpl.ret) {
            unify(self.ctx, r, exp, &tparams, &mut bind);
        }
        for (p, a) in tmpl.params.iter().zip(args.iter()) {
            if is_lit(a) {
                if let TypeKind::Named(n) = &p.ty.kind {
                    if let Some(i) = tparams.iter().position(|t| t == n) {
                        bind[i].get_or_insert(Ty::I32);
                    }
                }
            }
        }
        bind.into_iter()
            .enumerate()
            .map(|(i, b)| {
                b.ok_or_else(|| {
                    Diag::new(
                        format!("cannot infer type parameter `{}` of `{short}`", tparams[i]),
                        span,
                        format!("specify it explicitly, e.g. `{short}<{}>(..)`", vec!["..."; tparams.len()].join(", ")),
                    )
                })
            })
            .collect()
    }

    /// 제네릭 구조체 리터럴의 타입 인자를 정한다. 우선순위: 명시한 것, 필드 값의 타입, 기대 타입, 정수 리터럴(`i32`).
    fn infer_struct_targs(&mut self, tname: &str, explicit: Vec<Ty>, inits: &mut [FieldInit], expected: Option<Ty>, span: Span) -> Result<Vec<Ty>, Diag> {
        let sd = self.ctx.borrow().struct_templates.get(tname).cloned().expect("struct template");
        let short = tname.rsplit("::").next().unwrap_or(tname).to_string();
        let tparams = sd.tparams.clone();
        if explicit.len() > tparams.len() {
            return Err(Diag::new(
                format!("`{short}` takes {} type argument(s) but {} were given", tparams.len(), explicit.len()),
                span,
                "too many type arguments",
            ));
        }
        let mut bind: Vec<Option<Ty>> = vec![None; tparams.len()];
        for (i, t) in explicit.iter().enumerate() {
            bind[i] = Some(*t);
        }
        if let Some(Ty::Struct(sn)) = expected {
            let info = self.ctx.borrow().inst_info.get(sn).cloned();
            if let Some((tn, targs)) = info {
                if tn == tname {
                    for (i, t) in targs.into_iter().enumerate() {
                        bind[i].get_or_insert(t);
                    }
                }
            }
        }
        for init in inits.iter_mut() {
            if bind.iter().all(Option::is_some) {
                break;
            }
            let Some(fd) = sd.fields.iter().find(|f| f.name == init.name) else { continue };
            if Self::deferred(&init.value) {
                continue;
            }
            let at = self.expr(&mut init.value, None)?;
            unify(self.ctx, &fd.ty, at, &tparams, &mut bind);
        }
        for init in inits.iter() {
            if is_lit(&init.value) {
                if let Some(fd) = sd.fields.iter().find(|f| f.name == init.name) {
                    if let TypeKind::Named(n) = &fd.ty.kind {
                        if let Some(i) = tparams.iter().position(|t| t == n) {
                            bind[i].get_or_insert(Ty::I32);
                        }
                    }
                }
            }
        }
        bind.into_iter()
            .enumerate()
            .map(|(i, b)| {
                b.ok_or_else(|| {
                    Diag::new(
                        format!("cannot infer type parameter `{}` of struct `{short}`", tparams[i]),
                        span,
                        format!("write the type arguments, e.g. `{short}<{}> {{ .. }}`", vec!["..."; tparams.len()].join(", ")),
                    )
                })
            })
            .collect()
    }

    /// place 표현식을 통해 값을 바꿀 수 있는가. (`expr()` 이후에 호출한다.)
    fn place_mutable(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Var(n) => self.lookup(n).map(|v| v.mutable).unwrap_or(false),
            ExprKind::Deref(inner) => matches!(inner.ty, Ty::Ref(_, true) | Ty::Ptr(_, true)),
            ExprKind::Field(base, _) => match base.ty {
                Ty::Ref(Ty::Str | Ty::String, _) | Ty::Str | Ty::String => false,
                Ty::Ref(Ty::Array(..) | Ty::Slice(_), _) | Ty::Array(..) => false,
                Ty::Ref(_, m) => m,
                _ => self.place_mutable(base),
            },
            ExprKind::Index(base, _) | ExprKind::Slice(base, _, _) => match base.ty {
                Ty::Ref(_, m) => m,
                _ => self.place_mutable(base),
            },
            _ => false,
        }
    }

    fn root_var(e: &Expr) -> Option<&str> {
        match &e.kind {
            ExprKind::Var(n) => Some(n),
            ExprKind::Field(base, _) | ExprKind::Index(base, _) | ExprKind::Slice(base, _, _) if !matches!(base.ty, Ty::Ref(..)) => Self::root_var(base),
            _ => None,
        }
    }

    /// 가변이 아닌 place를 바꾸거나 `&mut`로 빌리려 할 때의 오류.
    fn immutable_diag(e: &Expr, assign: bool) -> Diag {
        let phrase = |subject: String| {
            if assign {
                format!("cannot assign to {subject}")
            } else {
                format!("cannot borrow {subject} as mutable")
            }
        };
        match &e.kind {
            ExprKind::Var(n) => Diag::new(
                phrase(format!("immutable variable `{n}`")),
                e.span,
                format!("declare it with `mut {n} = ...`"),
            ),
            ExprKind::Field(base, _) if matches!(base.ty, Ty::Ref(_, false)) => Diag::new(
                phrase("a field behind a `&` reference".to_string()),
                e.span,
                "this reference is not `&mut`",
            ),
            ExprKind::Index(base, _) | ExprKind::Slice(base, _, _) if matches!(base.ty, Ty::Ref(_, false)) => Diag::new(
                phrase("an element behind a `&` reference".to_string()),
                e.span,
                "this reference is not `&mut`",
            ),
            ExprKind::Index(..) | ExprKind::Slice(..) => match Self::root_var(e) {
                Some(n) => Diag::new(
                    phrase(format!("an element of immutable variable `{n}`")),
                    e.span,
                    format!("declare it with `mut {n} = ...`"),
                ),
                None => Diag::new(phrase("an immutable place".to_string()), e.span, ""),
            },
            ExprKind::Field(..) => match Self::root_var(e) {
                Some(n) => Diag::new(
                    phrase(format!("a field of immutable variable `{n}`")),
                    e.span,
                    format!("declare it with `mut {n} = ...`"),
                ),
                None => Diag::new(phrase("an immutable place".to_string()), e.span, ""),
            },
            ExprKind::Deref(inner) if matches!(inner.ty, Ty::Ptr(..)) => Diag::new(
                phrase("data behind a `*const` pointer".to_string()),
                e.span,
                "use a `*mut` pointer",
            ),
            ExprKind::Deref(_) => Diag::new(
                phrase("data behind a `&` reference".to_string()),
                e.span,
                "use a `&mut` reference",
            ),
            _ => Diag::new(phrase("an immutable place".to_string()), e.span, ""),
        }
    }

    fn lookup(&self, name: &str) -> Option<&Var> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn func(&mut self, f: &mut Func) -> Result<(), Diag> {
        self.scopes.push(HashMap::new());
        for p in &f.params {
            let ty = self.res(&p.ty)?;
            self.scopes
                .last_mut()
                .unwrap()
                .insert(p.name.clone(), Var { ty, mutable: false });
        }
        let want = if self.ret == Ty::Unit { None } else { Some(self.ret) };
        let body_ty = self.block(&mut f.body, want)?;
        self.scopes.pop();
        if self.ret != Ty::Unit && !accepts(self.ret, body_ty) {
            let span = f.body.tail.as_ref().map(|t| t.span).unwrap_or(f.body.span);
            let mut d = mismatch(self.ret, body_ty, span);
            if body_ty == Ty::Unit {
                d.msg = format!(
                    "function `{}` must return `{}` but its body has no final value",
                    f.name,
                    self.ret.name()
                );
                d.label = "add a final expression or `return`".to_string();
            }
            return Err(d);
        }
        Ok(())
    }

    fn loop_kw(&self, kw: &str, span: Span) -> Result<(), Diag> {
        if self.loops == 0 {
            return Err(Diag::new(
                format!("`{kw}` outside of a loop"),
                span,
                "only allowed inside `while` and `for`",
            ));
        }
        Ok(())
    }

    fn block(&mut self, b: &mut Block, expected: Option<Ty>) -> Result<Ty, Diag> {
        self.scopes.push(HashMap::new());
        let r = self.block_inner(b, expected);
        self.scopes.pop();
        r
    }

    fn block_inner(&mut self, b: &mut Block, expected: Option<Ty>) -> Result<Ty, Diag> {
        for s in &mut b.stmts {
            self.stmt(s)?;
        }
        let ends_in_return = matches!(
            b.stmts.last(),
            Some(Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. })
        ) || matches!(
            b.stmts.last(),
            Some(Stmt::While { cond: Expr { kind: ExprKind::Bool(true), .. }, body, .. }) if !block_has_break(body)
        );
        match &mut b.tail {
            Some(t) => self.expr(t, expected),
            None => Ok(if ends_in_return { Ty::Never } else { Ty::Unit }),
        }
    }

    fn stmt(&mut self, s: &mut Stmt) -> Result<(), Diag> {
        match s {
            Stmt::Let {
                name,
                mutable,
                ty,
                var_ty,
                init,
                ..
            } => {
                let annotated = match ty {
                    Some(t) => Some(self.res(t)?),
                    None => None,
                };
                let init_ty = self.expr(init, annotated)?;
                let final_ty = match annotated {
                    Some(want) => {
                        if !accepts(want, init_ty) {
                            return Err(mismatch(want, init_ty, init.span));
                        }
                        want
                    }
                    None => init_ty,
                };
                if matches!(final_ty, Ty::Unit | Ty::Never) {
                    return Err(Diag::new(
                        format!("cannot bind `{name}` to a value-less expression"),
                        init.span,
                        "this expression has no value",
                    ));
                }
                *var_ty = final_ty;
                self.scopes.last_mut().unwrap().insert(
                    name.clone(),
                    Var {
                        ty: final_ty,
                        mutable: *mutable,
                    },
                );
                Ok(())
            }
            Stmt::Assign { target, value, .. } => {
                let tt = self.expr(target, None)?;
                if !target.is_place() {
                    return Err(Diag::new(
                        "invalid assignment target",
                        target.span,
                        "this expression is not a place",
                    ));
                }
                if !self.place_mutable(target) {
                    return Err(Self::immutable_diag(target, true));
                }
                let vty = self.expr(value, Some(tt))?;
                if !accepts(tt, vty) {
                    return Err(mismatch(tt, vty, value.span));
                }
                Ok(())
            }
            Stmt::Return { value, span } => {
                match value {
                    Some(v) => {
                        if self.ret == Ty::Unit {
                            return Err(Diag::new(
                                "cannot return a value from a function with no return type",
                                v.span,
                                "this function returns nothing",
                            ));
                        }
                        let t = self.expr(v, Some(self.ret))?;
                        if !accepts(self.ret, t) {
                            return Err(mismatch(self.ret, t, v.span));
                        }
                    }
                    None => {
                        if self.ret != Ty::Unit {
                            return Err(Diag::new(
                                format!("`return` needs a value of type `{}`", self.ret.name()),
                                *span,
                                "missing value",
                            ));
                        }
                    }
                }
                Ok(())
            }
            Stmt::While { cond, body, step, .. } => {
                let ct = self.expr(cond, Some(Ty::Bool))?;
                if ct != Ty::Bool {
                    return Err(mismatch(Ty::Bool, ct, cond.span));
                }
                self.loops += 1;
                let r = self.block(body, None);
                let r = r.and_then(|_| {
                    for s in step.iter_mut() {
                        self.stmt(s)?;
                    }
                    Ok(())
                });
                self.loops -= 1;
                r
            }
            Stmt::Break { span } => self.loop_kw("break", *span),
            Stmt::Continue { span } => self.loop_kw("continue", *span),
            Stmt::Expr(e) => self.expr(e, None).map(|_| ()),
        }
    }

    fn expr(&mut self, e: &mut Expr, expected: Option<Ty>) -> Result<Ty, Diag> {
        let ty = self.expr_kind(e, expected)?;
        e.ty = ty;
        Ok(ty)
    }

    fn int_target(expected: Option<Ty>) -> Ty {
        match expected {
            Some(t) if t.is_int() => t,
            _ => Ty::I32,
        }
    }

    fn expr_kind(&mut self, e: &mut Expr, expected: Option<Ty>) -> Result<Ty, Diag> {
        let mut expected = expected;
        // `f<T>(..)`, `Type<T>::f(..)`, `Type<T> { .. }`: 명시적 타입 인자를 해석해 안쪽 식이 가져가게 한다.
        if let ExprKind::Generic(targs, inner) = &mut e.kind {
            let mut tys = Vec::new();
            for t in targs.iter() {
                tys.push(self.res(t)?);
            }
            let span = e.span;
            let inner = std::mem::replace(&mut **inner, Expr { kind: ExprKind::Int(0), span, ty: Ty::Unit });
            match &inner.kind {
                ExprKind::Call(n, _) if matches!(n.as_str(), "Ok" | "Err" | "Some" | "wrapping_add" | "wrapping_sub" | "wrapping_mul") => {
                    return Err(Diag::new(format!("`{n}` takes no type arguments"), span, "it is not generic"));
                }
                ExprKind::Call(..) | ExprKind::StructLit(..) => {}
                _ => return Err(Diag::new("type arguments can only be given to a function call or struct literal", span, "")),
            }
            self.pending_targs = tys;
            e.kind = inner.kind;
        }
        // `Ok(x)` / `Err(x)`는 내장 생성자다.
        // `None`은 변수가 아니라 내장 값이다.
        if matches!(&e.kind, ExprKind::Var(n) if n == "None") && self.lookup("None").is_none() {
            if let Some(Ty::Result(_, er)) = expected {
                if *er != Ty::NoneTag {
                    return Err(Diag::new(
                        "`None` builds an `Option`, but a `Result` is expected here",
                        e.span,
                        "use `Err(..)`",
                    ));
                }
            }
            let sp = e.span;
            e.kind = ExprKind::Ctor(false, Box::new(Expr { kind: ExprKind::NoneTag, span: sp, ty: Ty::Unit }));
        }
        // `wrapping_add/sub/mul(a, b)`는 감싸는 산술 연산이다.
        if let ExprKind::Call(name, args) = &mut e.kind {
            let op = match name.as_str() {
                "wrapping_add" => Some(BinOp::WrapAdd),
                "wrapping_sub" => Some(BinOp::WrapSub),
                "wrapping_mul" => Some(BinOp::WrapMul),
                _ => None,
            };
            if let Some(op) = op.filter(|_| !self.ctx.borrow().sigs.contains_key(name.as_str())) {
                if args.len() != 2 {
                    return Err(Diag::new(
                        format!("`{name}` takes exactly 2 arguments"),
                        e.span,
                        "wrong number of arguments",
                    ));
                }
                let r = args.pop().unwrap();
                let l = args.pop().unwrap();
                e.kind = ExprKind::Binary(op, Box::new(l), Box::new(r));
            }
        }
        let ctor = match &mut e.kind {
            ExprKind::Call(name, args)
                if matches!(name.as_str(), "Ok" | "Err" | "Some") && !self.ctx.borrow().sigs.contains_key(name.as_str()) =>
            {
                Some((name.clone(), std::mem::take(args)))
            }
            _ => None,
        };
        if let Some((name, mut args)) = ctor {
            if args.len() != 1 {
                return Err(Diag::new(
                    format!("`{name}` takes exactly 1 argument"),
                    e.span,
                    "wrong number of arguments",
                ));
            }
            if let Some(Ty::Result(_, er)) = expected {
                let opt = *er == Ty::NoneTag;
                if name == "Some" && !opt {
                    return Err(Diag::new("`Some` builds an `Option`, but a `Result` is expected here", e.span, "use `Ok(..)`"));
                }
                if name != "Some" && opt {
                    return Err(Diag::new(
                        format!("`{name}` builds a `Result`, but an `Option` is expected here"),
                        e.span,
                        "use `Some(..)` or `None`",
                    ));
                }
            }
            let mut payload = args.pop().unwrap();
            // 기대 타입이 없으면 `Some(x)`는 인자 타입에서 `Option<T>`를 만든다.
            if name == "Some" && !matches!(expected, Some(Ty::Result(..))) {
                let t = self.expr(&mut payload, None)?;
                if !matches!(t, Ty::Unit | Ty::Never) {
                    expected = Some(Ty::option(t));
                }
            }
            e.kind = ExprKind::Ctor(name != "Err", Box::new(payload));
        }
        let span = e.span;
        match &mut e.kind {
            ExprKind::Int(n) => {
                let ty = Self::int_target(expected);
                if !ty.fits(*n as i128) {
                    return Err(Diag::new(
                        format!("integer literal out of range for `{}`", ty.name()),
                        span,
                        "value does not fit",
                    ));
                }
                Ok(ty)
            }
            ExprKind::Float(_) => Ok(Ty::F64),
            ExprKind::Str(_) => Ok(Ty::Str),
            ExprKind::NoneTag => Ok(Ty::NoneTag),
            ExprKind::Ctor(is_ok, arg) => {
                let Some(Ty::Result(t, er)) = expected else {
                    let none = matches!(arg.kind, ExprKind::NoneTag);
                    return Err(Diag::new(
                        format!("cannot infer the type of `{}`", if none { "None" } else if *is_ok { "Ok(..)" } else { "Err(..)" }),
                        span,
                        "annotate the type, e.g. `let r: Result<i32, i32> = ...` or `let o: Option<i32> = ...`",
                    ));
                };
                let want = if *is_ok { *t } else { *er };
                let at = self.expr(arg, Some(want))?;
                self.want(want, at, arg.span)?;
                Ok(expected.unwrap())
            }
            ExprKind::Try(inner) => {
                let it = self.expr(inner, None)?;
                let Ty::Result(t, er) = it else {
                    return Err(Diag::new(
                        format!("the `?` operator needs a `Result` or an `Option`, found `{}`", it.name()),
                        inner.span,
                        "not a `Result` or `Option`",
                    ));
                };
                match self.ret {
                    Ty::Result(_, re) if re == er => Ok(*t),
                    Ty::Result(_, re) if *re == Ty::NoneTag || *er == Ty::NoneTag => Err(Diag::new(
                        "`?` cannot mix `Option` and `Result`",
                        span,
                        "the operand and the function's return type must both be `Option` or both `Result`",
                    )),
                    Ty::Result(_, re) => Err(Diag::new(
                        format!(
                            "`?` cannot convert error type `{}` into `{}`",
                            er.name(),
                            re.name()
                        ),
                        span,
                        "error types must match exactly",
                    )),
                    _ => Err(Diag::new(
                        "`?` can only be used in a function that returns `Result` or `Option`",
                        span,
                        "this function returns neither",
                    )),
                }
            }
            ExprKind::Match { scrut, ok, err } => {
                let st = self.expr(scrut, None)?;
                let Ty::Result(t, er) = st else {
                    return Err(Diag::new(
                        format!("`match` needs a `Result` or an `Option`, found `{}`", st.name()),
                        scrut.span,
                        "only `Result` and `Option` can be matched",
                    ));
                };
                let is_opt = *er == Ty::NoneTag;
                for arm in [&*ok, &*err] {
                    if arm.option != is_opt {
                        return Err(Diag::new(
                            format!(
                                "this `match` is on a `{}` but uses {} patterns",
                                if is_opt { "Option" } else { "Result" },
                                if is_opt { "`Ok`/`Err`" } else { "`Some`/`None`" }
                            ),
                            arm.span,
                            if is_opt { "use `Some(..)` and `None`" } else { "use `Ok(..)` and `Err(..)`" },
                        ));
                    }
                }
                let mut tys = Vec::new();
                for (arm, bt) in [(ok, *t), (err, *er)] {
                    arm.bind_ty = bt;
                    let mut scope = HashMap::new();
                    scope.insert(arm.bind.clone(), Var { ty: bt, mutable: false });
                    self.scopes.push(scope);
                    // 첫 팔의 수치 타입이 둘째 팔의 리터럴 타입을 정한다.
                    let hint = expected.or_else(|| tys.first().copied().filter(|t: &Ty| t.is_numeric()));
                    let r = self.block(&mut arm.body, hint);
                    self.scopes.pop();
                    tys.push(r?);
                }
                match (tys[0], tys[1]) {
                    (Ty::Never, x) | (x, Ty::Never) => Ok(x),
                    (a, b) if a == b => Ok(a),
                    (a, b) => Err(mismatch(a, b, err.body.span)),
                }
            }
            ExprKind::TypedInt(n, t) => {
                let (n, t) = (*n, *t);
                e.kind = ExprKind::Int(n);
                Ok(t)
            }
            ExprKind::RepeatOf(..) | ExprKind::Generic(..) => unreachable!("resolved before the type check"),
            ExprKind::Bool(_) => Ok(Ty::Bool),
            ExprKind::Var(name) => match self.lookup(name) {
                Some(v) => Ok(v.ty),
                None => Err(Diag::new(
                    format!("cannot find variable `{name}`"),
                    span,
                    "not found in this scope",
                )),
            },
            ExprKind::Field(base, name) => {
                let bt = self.expr(base, None)?;
                let inner_ty = match bt {
                    Ty::Ref(t, _) => *t,
                    t => t,
                };
                if matches!(inner_ty, Ty::Str | Ty::String) {
                    return match name.as_str() {
                        "len" => Ok(Ty::U64),
                        "ptr" => Ok(Ty::ptr(Ty::U8, false)),
                        _ => Err(Diag::new(
                            format!("no field `{name}` on type `{}`", inner_ty.name()),
                            span,
                            "available fields: len, ptr",
                        )),
                    };
                }
                if let Ty::Array(elem, _) | Ty::Slice(elem) = inner_ty {
                    return match name.as_str() {
                        "len" => Ok(Ty::U64),
                        "ptr" => {
                            let m = matches!(bt, Ty::Ref(_, true)) || (matches!(bt, Ty::Array(..)) && self.place_mutable(base));
                            Ok(Ty::ptr(*elem, m))
                        }
                        _ => Err(Diag::new(
                            format!("no field `{name}` on type `{}`", inner_ty.name()),
                            span,
                            "available fields: len, ptr",
                        )),
                    };
                }
                let sname = match bt {
                    Ty::Struct(n) => n,
                    Ty::Ref(Ty::Struct(n), _) => n,
                    other => {
                        let hint = if matches!(other, Ty::Ptr(..)) {
                            "dereference the pointer first: `(*p).field` in `unsafe`"
                        } else {
                            "field access needs a struct or a reference to one"
                        };
                        return Err(Diag::new(
                            format!("type `{}` has no fields", other.name()),
                            base.span,
                            hint,
                        ))
                    }
                };
                let fields = self.ctx.borrow().structs.fields[sname].clone();
                match fields.iter().find(|(n, _)| n == name) {
                    Some((_, t)) => Ok(*t),
                    None => Err(Diag::new(
                        format!("no field `{name}` on type `{sname}`"),
                        span,
                        format!(
                            "available fields: {}",
                            fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    )),
                }
            }
            ExprKind::Array(xs) => {
                let mut ety = match expected {
                    Some(Ty::Array(t, _)) => Some(*t),
                    _ => None,
                };
                let mut first: Option<Ty> = None;
                for x in xs.iter_mut() {
                    let t = self.expr(x, ety)?;
                    match first {
                        None => {
                            first = Some(t);
                            ety = Some(t);
                        }
                        Some(f) => self.want(f, t, x.span)?,
                    }
                }
                let et = first.expect("parser guarantees at least one element");
                validate_ty(&self.ctx.borrow().structs, Ty::array(et, 1), span)?;
                Ok(Ty::array(et, xs.len() as u64))
            }
            ExprKind::Repeat(x, n) => {
                let hint = match expected {
                    Some(Ty::Array(t, _)) => Some(*t),
                    _ => None,
                };
                let et = self.expr(x, hint)?;
                validate_ty(&self.ctx.borrow().structs, Ty::array(et, 1), span)?;
                Ok(Ty::array(et, *n))
            }
            ExprKind::Index(base, idx) => {
                let bt = self.expr(base, None)?;
                let it = self.expr(idx, Some(Ty::U64))?;
                if !it.is_int() && it != Ty::Never {
                    return Err(Diag::new(
                        format!("an index must be an integer, found `{}`", it.name()),
                        idx.span,
                        "use an integer expression (`x as u64` converts)",
                    ));
                }
                let arr_len = match bt {
                    Ty::Array(_, n) => Some(n),
                    Ty::Ref(Ty::Array(_, n), _) => Some(*n),
                    _ => None,
                };
                if let (Some(n), ExprKind::Int(k)) = (arr_len, &idx.kind) {
                    if *k as u64 >= n {
                        return Err(Diag::new(
                            format!("index out of bounds: the array has {n} element(s) but the index is {k}"),
                            idx.span,
                            "this index is always out of range",
                        ));
                    }
                }
                match elem_of(bt) {
                    Some(t) => Ok(t),
                    None => Err(Diag::new(
                        format!("cannot index into a value of type `{}`", bt.name()),
                        base.span,
                        "only arrays, `&[T; N]` and slices can be indexed",
                    )),
                }
            }
            ExprKind::Slice(base, lo, hi) => {
                if !std::mem::take(&mut self.slice_ok) {
                    return Err(Diag::new(
                        "a slice must be borrowed",
                        span,
                        "write `&a[..]` or `&mut a[lo..hi]`",
                    ));
                }
                let bt = self.expr(base, None)?;
                for x in [lo, hi].into_iter().flatten() {
                    let it = self.expr(x, Some(Ty::U64))?;
                    if !it.is_int() && it != Ty::Never {
                        return Err(Diag::new(
                            format!("a slice bound must be an integer, found `{}`", it.name()),
                            x.span,
                            "use an integer expression",
                        ));
                    }
                }
                match elem_of(bt) {
                    Some(t) => Ok(Ty::slice(t)),
                    None => Err(Diag::new(
                        format!("cannot slice a value of type `{}`", bt.name()),
                        base.span,
                        "only arrays, `&[T; N]` and slices can be sliced",
                    )),
                }
            }
            ExprKind::StructLit(name, inits) => {
                let explicit = std::mem::take(&mut self.pending_targs);
                // 제네릭 구조체: 타입 인자를 (명시, 기대 타입, 필드 값 순으로) 정해 인스턴스를 만든다.
                if self.ctx.borrow().struct_templates.contains_key(name.as_str()) {
                    let targs = self.infer_struct_targs(name, explicit, inits, expected, span)?;
                    let Ty::Struct(inst) = instantiate_struct(self.ctx, name, targs, span)? else { unreachable!() };
                    *name = inst.to_string();
                } else if !explicit.is_empty() {
                    return Err(Diag::new(format!("struct `{name}` takes no type arguments"), span, "it is not generic"));
                }
                let found = {
                    let c = self.ctx.borrow();
                    c.structs.tys.get(name.as_str()).copied().map(|t| (t, c.structs.fields[name.as_str()].clone()))
                };
                let Some((sty, fields)) = found else {
                    return Err(Diag::new(
                        format!("cannot find struct `{name}`"),
                        span,
                        "not defined",
                    ));
                };
                let mut seen: Vec<&str> = Vec::new();
                for init in inits.iter_mut() {
                    let Some((_, fty)) = fields.iter().find(|(n, _)| *n == init.name) else {
                        return Err(Diag::new(
                            format!("struct `{name}` has no field `{}`", init.name),
                            init.span,
                            "unknown field",
                        ));
                    };
                    if seen.contains(&init.name.as_str()) {
                        return Err(Diag::new(
                            format!("field `{}` is initialized more than once", init.name),
                            init.span,
                            "duplicate field",
                        ));
                    }
                    let vt = self.expr(&mut init.value, Some(*fty))?;
                    if !accepts(*fty, vt) {
                        return Err(mismatch(*fty, vt, init.value.span));
                    }
                    seen.push(init.name.as_str());
                }
                let missing: Vec<&str> = fields
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .filter(|n| !seen.contains(n))
                    .collect();
                if !missing.is_empty() {
                    return Err(Diag::new(
                        format!("missing field(s) in `{name}`: {}", missing.join(", ")),
                        span,
                        "all fields must be initialized",
                    ));
                }
                Ok(sty)
            }
            ExprKind::AddrOf(mutable, inner) => {
                let hint = match expected {
                    Some(Ty::Ref(t, _)) => Some(*t),
                    _ => None,
                };
                self.slice_ok = matches!(inner.kind, ExprKind::Slice(..));
                let t = self.expr(inner, hint)?;
                self.slice_ok = false;
                if !inner.is_place() {
                    return Err(Diag::new(
                        "cannot take the address of a temporary value",
                        inner.span,
                        "bind it to a variable first",
                    ));
                }
                if matches!(t, Ty::Unit | Ty::Never) {
                    return Err(Diag::new("cannot borrow a value-less expression", inner.span, ""));
                }
                if *mutable && !self.place_mutable(inner) {
                    return Err(Self::immutable_diag(inner, false));
                }
                Ok(Ty::reference(t, *mutable))
            }
            ExprKind::Deref(inner) => {
                let t = self.expr(inner, None)?;
                match t {
                    Ty::Ref(Ty::Slice(_), _) => Err(Diag::new(
                        "cannot dereference a slice reference",
                        span,
                        "index it (`s[i]`) or reborrow it (`&s[..]`)",
                    )),
                    Ty::Ref(target, _) => Ok(*target),
                    Ty::Ptr(target, _) => {
                        if self.unsafe_depth == 0 {
                            return Err(Diag::new(
                                "dereference of a raw pointer is unsafe",
                                span,
                                "needs an `unsafe` block or `unsafe fn`",
                            ));
                        }
                        Ok(*target)
                    }
                    Ty::Never => Ok(Ty::Never),
                    other => Err(Diag::new(
                        format!("cannot dereference a value of type `{}`", other.name()),
                        inner.span,
                        "only references and raw pointers can be dereferenced",
                    )),
                }
            }
            ExprKind::Cast(inner, target) => {
                let to = self.res(target)?;
                let hint = if matches!(to, Ty::Ptr(..)) { Some(Ty::U64) } else { None };
                let from = self.expr(inner, hint)?;
                let ok = from == to
                    || (from.is_numeric() && to.is_numeric())
                    || (from == Ty::Bool && to.is_int())
                    // 참조 -> raw 포인터 (같은 대상 타입, `&T`를 `*mut`로는 못 바꾼다)
                    || matches!((from, to), (Ty::Ref(a, fm), Ty::Ptr(b, tm)) if a == b && (fm || !tm))
                    || matches!((from, to), (Ty::Ptr(..), Ty::Ptr(..)))
                    || matches!((from, to), (Ty::Ptr(..), Ty::I64 | Ty::U64))
                    || matches!((from, to), (Ty::I64 | Ty::U64, Ty::Ptr(..)));
                if !ok && from != Ty::Never {
                    return Err(Diag::new(
                        format!("cannot cast `{}` to `{}`", from.name(), to.name()),
                        span,
                        "invalid cast",
                    ));
                }
                Ok(to)
            }
            ExprKind::Unary(UnOp::Neg, inner) => {
                if let ExprKind::Int(n) = inner.kind {
                    let ty = Self::int_target(expected);
                    if !ty.is_signed() {
                        return Err(Diag::new(
                            format!("cannot negate a literal of unsigned type `{}`", ty.name()),
                            span,
                            "unsigned types have no negative values",
                        ));
                    }
                    if !ty.fits(-(n as i128)) {
                        return Err(Diag::new(
                            format!("integer literal out of range for `{}`", ty.name()),
                            span,
                            "value does not fit",
                        ));
                    }
                    inner.ty = ty;
                    return Ok(ty);
                }
                let t = self.expr(inner, expected)?;
                if t.is_signed() || t == Ty::F64 || t == Ty::Never {
                    Ok(t)
                } else {
                    Err(Diag::new(
                        format!("cannot apply unary `-` to `{}`", t.name()),
                        inner.span,
                        "expected a signed integer or f64",
                    ))
                }
            }
            ExprKind::Unary(UnOp::BitNot, inner) => {
                let t = self.expr(inner, expected.filter(|t| t.is_int()))?;
                if !t.is_int() && t != Ty::Never {
                    return Err(self.bad_operand_un("~", t, inner.span));
                }
                Ok(t)
            }
            ExprKind::Unary(UnOp::Not, inner) => {
                let t = self.expr(inner, Some(Ty::Bool))?;
                self.want(Ty::Bool, t, inner.span)?;
                Ok(Ty::Bool)
            }
            ExprKind::Binary(op, l, r) => {
                let op = *op;
                use BinOp::*;
                let arithmetic = matches!(op, Add | Sub | Mul | Div | Rem | WrapAdd | WrapSub | WrapMul | BitAnd | BitOr | BitXor | Shl | Shr);
                let hint = if arithmetic {
                    expected.filter(|t| t.is_numeric())
                } else {
                    None
                };
                let (lt, rt);
                if is_lit(l) && !is_lit(r) && !matches!(op, Shl | Shr) {
                    rt = self.expr(r, hint)?;
                    lt = self.expr(l, Some(rt).filter(|t| t.is_numeric()))?;
                } else {
                    lt = self.expr(l, hint)?;
                    rt = self.expr(r, Some(lt).filter(|t| t.is_numeric()))?;
                }
                if let Ty::Ptr(..) = lt {
                    return self.ptr_binary(op, lt, rt, l.span, r.span, span);
                }
                if lt == Ty::Str && matches!(op, Eq | Ne) {
                    if rt != Ty::Str {
                        return Err(mismatch(Ty::Str, rt, r.span));
                    }
                    return Ok(Ty::Bool);
                }
                match op {
                    Add | Sub | Mul | Div | Rem | Lt | Le | Gt | Ge => {
                        if !lt.is_numeric() && lt != Ty::Never {
                            return Err(self.bad_operand(op, lt, l.span));
                        }
                        self.want(lt, rt, r.span)?;
                        Ok(if arithmetic { lt } else { Ty::Bool })
                    }
                    Eq | Ne => {
                        if !(lt.is_numeric() || lt == Ty::Bool || lt == Ty::Never) {
                            return Err(self.bad_operand(op, lt, l.span));
                        }
                        self.want(lt, rt, r.span)?;
                        Ok(Ty::Bool)
                    }
                    BitAnd | BitOr | BitXor | WrapAdd | WrapSub | WrapMul => {
                        if !lt.is_int() && lt != Ty::Never {
                            return Err(self.bad_operand(op, lt, l.span));
                        }
                        self.want(lt, rt, r.span)?;
                        Ok(lt)
                    }
                    Shl | Shr => {
                        if !lt.is_int() && lt != Ty::Never {
                            return Err(self.bad_operand(op, lt, l.span));
                        }
                        // 시프트 양은 어떤 정수형이어도 된다.
                        if !rt.is_int() && rt != Ty::Never {
                            return Err(self.bad_operand(op, rt, r.span));
                        }
                        Ok(lt)
                    }
                    And | Or => {
                        self.want(Ty::Bool, lt, l.span)?;
                        self.want(Ty::Bool, rt, r.span)?;
                        Ok(Ty::Bool)
                    }
                }
            }
            ExprKind::MethodCall(recv, mname, args) => {
                let rt = self.expr(recv, None)?;
                let inner = match rt {
                    Ty::Ref(t, _) => *t,
                    t => t,
                };
                if let Ty::Result(okt, errt) = inner {
                    let (okt, errt) = (*okt, *errt);
                    let is_opt = errt == Ty::NoneTag;
                    let kind_name = if is_opt { "Option" } else { "Result" };
                    let (mname, span) = (mname.clone(), e.span);
                    let mk = |kind: ExprKind| Expr { kind, span, ty: Ty::Unit };
                    let blk = |tail: Expr| Block { stmts: Vec::new(), tail: Some(Box::new(tail)), span };
                    let valid = match mname.as_str() {
                        "is_some" | "is_none" => is_opt,
                        "is_ok" | "is_err" => !is_opt,
                        "unwrap" | "unwrap_or" => true,
                        _ => false,
                    };
                    if !valid {
                        return Err(Diag::new(
                            format!("no method `{mname}` on type `{}`", inner.name()),
                            span,
                            if is_opt {
                                "`Option` has: is_some, is_none, unwrap, unwrap_or"
                            } else {
                                "`Result` has: is_ok, is_err, unwrap, unwrap_or"
                            },
                        ));
                    }
                    let want_args = usize::from(mname == "unwrap_or");
                    if args.len() != want_args {
                        return Err(Diag::new(
                            format!("`{mname}` takes {want_args} argument(s) but {} were given", args.len()),
                            span,
                            "wrong number of arguments",
                        ));
                    }
                    if matches!(rt, Ty::Ref(..)) {
                        return Err(Diag::new(
                            format!("`{mname}` takes the {kind_name} by value and cannot be called through a reference"),
                            recv.span,
                            "use `match`, or call it on the value itself",
                        ));
                    }
                    if mname.starts_with("is_") && ({ let c = self.ctx.borrow(); needs_drop(okt, &c.structs) || needs_drop(errt, &c.structs) }) {
                        return Err(Diag::new(
                            format!("`{mname}` would consume a {kind_name} that owns heap data"),
                            recv.span,
                            "use `match` instead",
                        ));
                    }
                    let (ok_body, err_body) = match mname.as_str() {
                        "is_some" | "is_ok" => (mk(ExprKind::Bool(true)), mk(ExprKind::Bool(false))),
                        "is_none" | "is_err" => (mk(ExprKind::Bool(false)), mk(ExprKind::Bool(true))),
                        "unwrap" => (
                            mk(ExprKind::Var("$v".to_string())),
                            mk(ExprKind::Call(
                                "panic".to_string(),
                                vec![mk(ExprKind::Str(format!(
                                    "called `unwrap()` on {} value",
                                    if is_opt { "a `None`" } else { "an `Err`" }
                                )))],
                            )),
                        ),
                        _ => (mk(ExprKind::Var("$v".to_string())), args.remove(0)),
                    };
                    let scrut = std::mem::replace(&mut **recv, mk(ExprKind::Int(0)));
                    let arm = |bind: &str, body: Expr| Arm {
                        bind: bind.to_string(),
                        bind_ty: Ty::Unit,
                        body: blk(body),
                        span,
                        option: is_opt,
                    };
                    e.kind = ExprKind::Match {
                        scrut: Box::new(scrut),
                        ok: arm("$v", ok_body),
                        err: arm("$e", err_body),
                    };
                    return self.expr_kind(e, expected);
                }
                let Ty::Struct(sname) = inner else {
                    return Err(Diag::new(
                        format!("type `{}` has no methods", inner.name()),
                        recv.span,
                        "methods exist only on structs",
                    ));
                };
                let full = format!("{sname}.{mname}");
                if !self.ctx.borrow().sigs.contains_key(full.as_str()) {
                    // 제네릭 구조체의 메서드는 이 인스턴스에 맞춰 처음 부를 때 만든다.
                    let inst = self.ctx.borrow().inst_info.get(sname).cloned();
                    let mut made = false;
                    if let Some((tn, targs)) = inst {
                        let tm = format!("{tn}.{mname}");
                        if self.ctx.borrow().fn_templates.contains_key(&tm) {
                            instantiate_fn(self.ctx, &tm, targs, span, self.depth)?;
                            made = true;
                        }
                    }
                    if !made {
                        return Err(Diag::new(
                            format!("no method `{mname}` on type `{sname}`"),
                            span,
                            "define it in an `impl` block",
                        ));
                    }
                }
                if !self.ctx.borrow().methods.contains(&full) {
                    return Err(Diag::new(
                        format!("`{mname}` is an associated function, not a method"),
                        span,
                        format!("call it as `{sname}::{mname}(..)`"),
                    ));
                }
                // 제네릭 인스턴스 이름(`m::Pair<..>.f`)의 모듈은 `<` 앞부분으로 정한다.
                let module_of = |n: &str| n.split('<').next().unwrap_or(n).rsplit_once("::").map(|x| x.0.to_string()).unwrap_or_default();
                let is_public = self.ctx.borrow().fn_public.get(&full).copied().unwrap_or(true);
                if !is_public && module_of(&full) != module_of(self.cur_fn) {
                    return Err(Diag::new(
                        format!("method `{mname}` of `{sname}` is private"),
                        span,
                        "mark it `pub` in its `impl` block",
                    ));
                }
                let self_param = self.ctx.borrow().sigs[full.as_str()].0[0];
                let recv_expr = std::mem::replace(&mut **recv, Expr { kind: ExprKind::Int(0), span, ty: Ty::Unit });
                let first = match (self_param, rt) {
                    // `&self`/`&mut self`: 값이면 빌리고, 이미 참조면 그대로 넘긴다.
                    (Ty::Ref(_, want_mut), Ty::Ref(..)) => {
                        if want_mut && !matches!(rt, Ty::Ref(_, true)) {
                            return Err(Diag::new(
                                format!("cannot call `&mut self` method `{mname}` through a `&` reference"),
                                recv_expr.span,
                                "this reference is not `&mut`",
                            ));
                        }
                        recv_expr
                    }
                    (Ty::Ref(_, want_mut), _) => {
                        let sp = recv_expr.span;
                        Expr { kind: ExprKind::AddrOf(want_mut, Box::new(recv_expr)), span: sp, ty: Ty::Unit }
                    }
                    // `self` (값): 참조 뒤의 값은 꺼낼 수 없으므로 거절은 소유권 검사에 맡기지 않고 여기서 한다.
                    (_, Ty::Ref(..)) => {
                        return Err(Diag::new(
                            format!("method `{mname}` takes `self` by value and cannot be called through a reference"),
                            recv_expr.span,
                            "use a `&self` method, or call it on the value itself",
                        ))
                    }
                    _ => recv_expr,
                };
                let mut all = vec![first];
                all.extend(std::mem::take(args));
                e.kind = ExprKind::Call(full, all);
                self.expr_kind(e, expected)
            }
            ExprKind::Call(name, args) => {
                let explicit = std::mem::take(&mut self.pending_targs);
                let is_template = {
                    let c = self.ctx.borrow();
                    !c.sigs.contains_key(name.as_str()) && c.fn_templates.contains_key(name.as_str())
                };
                if is_template {
                    let targs = self.infer_fn_targs(name, explicit, args, expected, span)?;
                    *name = instantiate_fn(self.ctx, name, targs, span, self.depth)?;
                } else if !explicit.is_empty() {
                    return Err(Diag::new(format!("`{name}` takes no type arguments"), span, "it is not generic"));
                }
                let found = self.ctx.borrow().sigs.get(name.as_str()).cloned();
                let Some((params, ret)) = found else {
                    return Err(Diag::new(
                        format!("cannot find function `{name}`"),
                        span,
                        "not defined",
                    ));
                };
                if let Some(only) = builtin_unavailable(name, self.target) {
                    return Err(Diag::new(
                        format!("`{name}` is not available when targeting {}", self.target.name()),
                        span,
                        format!("it exists only on {only}"),
                    ));
                }
                if self.ctx.borrow().unsafe_fns.contains(name.as_str()) && self.unsafe_depth == 0 {
                    return Err(Diag::new(
                        format!("call to unsafe function `{name}` is unsafe"),
                        span,
                        "needs an `unsafe` block or `unsafe fn`",
                    ));
                }
                if params.len() != args.len() {
                    return Err(Diag::new(
                        format!(
                            "`{name}` takes {} argument(s) but {} were given",
                            params.len(),
                            args.len()
                        ),
                        span,
                        "wrong number of arguments",
                    ));
                }
                for (p, a) in params.iter().zip(args.iter_mut()) {
                    let mut at = self.expr(a, Some(*p))?;
                    // `&arr` / `&mut arr`를 `&[T]` / `&mut [T]` 자리에 쓰면 전체 슬라이스로 바꿔 준다.
                    if let (Ty::Ref(Ty::Slice(se), pm), Ty::Ref(Ty::Array(ae, _), am)) = (*p, at) {
                        if se == ae && (am || !pm) {
                            if let ExprKind::AddrOf(m, inner) = &mut a.kind {
                                let inner_e = std::mem::replace(&mut **inner, Expr { kind: ExprKind::Int(0), span: a.span, ty: Ty::Unit });
                                let sp = inner_e.span;
                                **inner = Expr { kind: ExprKind::Slice(Box::new(inner_e), None, None), span: sp, ty: Ty::Unit };
                                let _ = m;
                                at = self.expr(a, Some(*p))?;
                            }
                        }
                    }
                    self.want(*p, at, a.span)?;
                }
                Ok(ret)
            }
            ExprKind::Unsafe(block) => {
                self.unsafe_depth += 1;
                let r = self.block(block, expected);
                self.unsafe_depth -= 1;
                r
            }
            ExprKind::If { cond, then, els } => {
                let ct = self.expr(cond, Some(Ty::Bool))?;
                if ct != Ty::Bool {
                    return Err(mismatch(Ty::Bool, ct, cond.span));
                }
                let tt = self.block(then, expected)?;
                match els {
                    None => Ok(Ty::Unit),
                    Some(eb) => {
                        let et = self.block(eb, expected)?;
                        match (tt, et) {
                            (Ty::Never, x) | (x, Ty::Never) => Ok(x),
                            (a, b) if a == b => Ok(a),
                            (a, b) => Err(mismatch(a, b, eb.span)),
                        }
                    }
                }
            }
        }
    }

    /// 좌변이 raw 포인터인 이항 연산. `p + n`, `p - n`은 unsafe이고, `==`/`!=`는 safe다.
    fn ptr_binary(&self, op: BinOp, lt: Ty, rt: Ty, ls: Span, rs: Span, span: Span) -> Result<Ty, Diag> {
        match op {
            BinOp::Eq | BinOp::Ne => {
                self.want(lt, rt, rs)?;
                Ok(Ty::Bool)
            }
            BinOp::Add | BinOp::Sub => {
                if self.unsafe_depth == 0 {
                    return Err(Diag::new(
                        "pointer arithmetic is unsafe",
                        span,
                        "needs an `unsafe` block or `unsafe fn`",
                    ));
                }
                if !rt.is_int() {
                    return Err(Diag::new(
                        format!("pointer offset must be an integer, found `{}`", rt.name()),
                        rs,
                        "expected an integer",
                    ));
                }
                Ok(lt)
            }
            _ => Err(self.bad_operand(op, lt, ls)),
        }
    }

    fn bad_operand_un(&self, op: &str, t: Ty, span: Span) -> Diag {
        Diag::new(
            format!("operator `{op}` cannot be applied to `{}`", t.name()),
            span,
            "unsupported operand type",
        )
    }

    fn bad_operand(&self, op: BinOp, t: Ty, span: Span) -> Diag {
        Diag::new(
            format!("operator `{}` cannot be applied to `{}`", op_text(op), t.name()),
            span,
            "unsupported operand type",
        )
    }

    fn want(&self, expected: Ty, found: Ty, span: Span) -> Result<(), Diag> {
        if accepts(expected, found) || expected == Ty::Never {
            Ok(())
        } else {
            Err(mismatch(expected, found, span))
        }
    }
}
