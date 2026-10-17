//! 빌림(borrow) 검사. 소유권 검사 뒤에 도는 패스다.
//!
//! 방식: 함수 본문을 평가 순서의 **이벤트 목록**으로 낮춘 뒤(`lower`), 목록을 훑으며 검사한다(`analyze`).
//!
//! - `&place` / `&mut place`는 **loan**을 만든다. loan은 그것을 쥔 변수가 이후에 쓰이는 동안,
//!   또는 아직 소비되지 않은 임시값인 동안 살아 있다 (NLL 방식: 마지막 사용까지만).
//! - 살아 있는 loan과 겹치는 접근이 충돌하면 오류다.
//!   (읽기/`&`는 `&mut` loan과, 쓰기/이동/`&mut`는 모든 loan과 충돌)
//! - `&mut` 참조를 값으로 쓰면 암묵적 재차용(reborrow)이다. 그래서 `&mut`는 복사해도 배타적이다.
//! - dangling: 지역 변수를 가리키는 loan이 반환되거나, 변수의 스코프가 끝난 뒤에도 살아 있으면 오류다.
//! - 함수가 반환하는 참조가 어느 매개변수에서 왔는지는 본문 분석으로 추론한다 (lifetime 표기 없음).
//! - `while`은 본문을 두 번 펼쳐서 반복 간 충돌을 잡는다.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::typeck::{Checked, StructTable};
use crate::types::Ty;

type VarId = usize;
type LoanId = usize;

#[derive(Clone, PartialEq, Eq, Debug)]
enum Proj {
    Field(String),
    Deref,
    /// 배열/슬라이스 원소. 어느 원소인지는 구분하지 않는다(보수적).
    Index,
}

#[derive(Clone, Debug)]
struct Place {
    root: VarId,
    projs: Vec<Proj>,
}

impl Place {
    fn has_deref(&self) -> bool {
        self.projs.contains(&Proj::Deref)
    }

    fn overlaps(&self, other: &Place) -> bool {
        self.root == other.root
            && self
                .projs
                .iter()
                .zip(&other.projs)
                .all(|(a, b)| a == b)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AKind {
    Read,
    Write,
    Move,
    BorrowShared,
    BorrowMut,
}

struct Loan {
    /// `None`이면 매개변수에서 온 외부 출처(`param`)다.
    place: Option<Place>,
    mutable: bool,
    span: Span,
    param: Option<usize>,
}

enum EvKind {
    Access {
        place: Place,
        kind: AKind,
        loan: Option<LoanId>,
    },
    Hold {
        var: VarId,
        loans: Vec<LoanId>,
    },
    Release(Vec<LoanId>),
    Retemp(Vec<LoanId>),
    ScopeEnd(Vec<VarId>),
    Return(Vec<LoanId>),
    /// 참조를 통해 바깥으로 저장된 값
    Escape(Vec<LoanId>),
}

struct Ev {
    kind: EvKind,
    span: Span,
    arms: Vec<(usize, u8)>,
}

/// 함수의 참조 흐름 요약 (lifetime 표기 없이 본문에서 추론한다).
#[derive(Clone, PartialEq, Default, Debug)]
struct Summary {
    /// 반환값이 빌려 오는 매개변수 번호
    ret: Vec<usize>,
    /// `(j, i)`: 매개변수 j가 든 참조가 `&mut` 매개변수 i가 가리키는 곳에 저장될 수 있다
    flows: Vec<(usize, usize)>,
}

pub fn check(p: &Program, ck: &Checked) -> Result<(), Diag> {
    let refs = ref_structs(&ck.structs);
    let carries = |ty: Ty| carries_ref(ty, &refs);

    // 함수별 "반환 참조가 어느 매개변수에서 오는가" 요약. 가장 보수적인 값(참조를 가진 모든 매개변수)에서
    // 시작해 본문 분석으로 좁힌다. 각 단계의 결과는 이전 요약이 안전하다는 가정 아래 안전하다.
    let mut summaries: HashMap<String, Summary> = HashMap::new();
    for f in &p.funcs {
        let (ptys, _) = &ck.sigs[&f.name];
        let ret = (0..ptys.len()).filter(|i| carries(ptys[*i])).collect();
        let mut flows = Vec::new();
        for i in 0..ptys.len() {
            // `&mut X` (X가 참조를 담을 수 있음) 안으로는 다른 인자의 참조가 저장될 수 있다.
            if let Ty::Ref(inner, true) = ptys[i] {
                if carries(*inner) {
                    for j in 0..ptys.len() {
                        if j != i && carries(ptys[j]) {
                            flows.push((j, i));
                        }
                    }
                }
            }
        }
        summaries.insert(f.name.clone(), Summary { ret, flows });
    }
    for (name, ..) in crate::typeck::builtins() {
        // `as_str(&String)`만 인자의 빌림을 반환값으로 넘긴다.
        let ret = if name == "as_str" { vec![0] } else { Vec::new() };
        summaries.insert(name.to_string(), Summary { ret, flows: Vec::new() });
    }
    for _ in 0..=p.funcs.len() {
        let mut changed = false;
        for f in &p.funcs {
            let s = lower(f, ck, &refs, &summaries).summary();
            if summaries[&f.name] != s {
                summaries.insert(f.name.clone(), s);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for f in &p.funcs {
        lower(f, ck, &refs, &summaries).analyze()?;
    }
    Ok(())
}

/// 참조를 (간접적으로) 포함하는 구조체 이름들.
fn ref_structs(t: &StructTable) -> HashSet<String> {
    let mut set: HashSet<String> = HashSet::new();
    loop {
        let before = set.len();
        for name in &t.order {
            if set.contains(name) {
                continue;
            }
            if t.fields[name].iter().any(|(_, ty)| carries_ref(*ty, &set)) {
                set.insert(name.clone());
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

fn carries_ref(ty: Ty, refs: &HashSet<String>) -> bool {
    match ty {
        Ty::Ref(..) | Ty::Str => true,
        Ty::Struct(n) => refs.contains(n),
        Ty::Result(t, e) => carries_ref(*t, refs) || carries_ref(*e, refs),
        _ => false,
    }
}

struct Low<'a> {
    ck: &'a Checked,
    refs: &'a HashSet<String>,
    summaries: &'a HashMap<String, Summary>,
    events: Vec<Ev>,
    loans: Vec<Loan>,
    var_names: Vec<String>,
    var_loans: HashMap<VarId, Vec<LoanId>>,
    scopes: Vec<(HashMap<String, VarId>, Vec<VarId>)>,
    arms: Vec<(usize, u8)>,
    next_if: usize,
    /// (if id, arm) -> 그 분기가 `return`으로 끝나는가
    div: HashMap<(usize, u8), bool>,
    ret: Ty,
    /// 이 함수 본문에서 관찰된 `(j, i)` 흐름
    flows: Vec<(usize, usize)>,
}

fn lower<'a>(
    f: &Func,
    ck: &'a Checked,
    refs: &'a HashSet<String>,
    summaries: &'a HashMap<String, Summary>,
) -> Low<'a> {
    let (ptys, ret) = ck.sigs[&f.name].clone();
    let mut lw = Low {
        ck,
        refs,
        summaries,
        events: Vec::new(),
        loans: Vec::new(),
        var_names: Vec::new(),
        var_loans: HashMap::new(),
        scopes: vec![(HashMap::new(), Vec::new())],
        arms: Vec::new(),
        next_if: 0,
        div: HashMap::new(),
        ret,
        flows: Vec::new(),
    };
    for (i, (p, ty)) in f.params.iter().zip(&ptys).enumerate() {
        let v = lw.declare(&p.name);
        if lw.carries(*ty) {
            let l = lw.new_loan(None, false, p.span, Some(i));
            lw.var_loans.insert(v, vec![l]);
        }
    }
    lw.fn_body(&f.body);
    lw
}

fn block_diverges(b: &Block) -> bool {
    if matches!(b.stmts.last(), Some(Stmt::Return { .. })) {
        return true;
    }
    match b.tail.as_deref() {
        Some(Expr {
            kind: ExprKind::If { then, els: Some(e), .. },
            ..
        }) => block_diverges(then) && block_diverges(e),
        _ => false,
    }
}

impl<'a> Low<'a> {
    // ------------------------------------------------------------------ 낮추기

    fn carries(&self, ty: Ty) -> bool {
        carries_ref(ty, self.refs)
    }

    fn emit(&mut self, kind: EvKind, span: Span) {
        let arms = self.arms.clone();
        self.events.push(Ev { kind, span, arms });
    }

    fn new_loan(&mut self, place: Option<Place>, mutable: bool, span: Span, param: Option<usize>) -> LoanId {
        self.loans.push(Loan {
            place,
            mutable,
            span,
            param,
        });
        self.loans.len() - 1
    }

    fn declare(&mut self, name: &str) -> VarId {
        let id = self.var_names.len();
        self.var_names.push(name.to_string());
        let (map, list) = self.scopes.last_mut().unwrap();
        map.insert(name.to_string(), id);
        list.push(id);
        id
    }

    fn lookup(&self, name: &str) -> VarId {
        self.scopes
            .iter()
            .rev()
            .find_map(|(m, _)| m.get(name).copied())
            .expect("typeck guarantees variables exist")
    }

    fn loans_of_var(&self, v: VarId) -> Vec<LoanId> {
        self.var_loans.get(&v).cloned().unwrap_or_default()
    }

    fn release(&mut self, loans: &[LoanId], span: Span) {
        if !loans.is_empty() {
            self.emit(EvKind::Release(loans.to_vec()), span);
        }
    }

    fn fn_body(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
        if let Some(t) = &b.tail {
            let want = if self.ret == Ty::Unit { None } else { Some(self.ret) };
            let ls = self.value(t, want);
            self.emit(EvKind::Return(ls), t.span);
        }
    }

    fn block(&mut self, b: &Block, want: Option<Ty>) -> Vec<LoanId> {
        self.scopes.push((HashMap::new(), Vec::new()));
        for s in &b.stmts {
            self.stmt(s);
        }
        let ls = match &b.tail {
            Some(t) => self.value(t, want),
            None => Vec::new(),
        };
        let (_, vars) = self.scopes.pop().unwrap();
        if !vars.is_empty() {
            self.emit(EvKind::ScopeEnd(vars), b.span);
        }
        ls
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let {
                name,
                init,
                var_ty,
                span,
                ..
            } => {
                let ls = self.value(init, Some(*var_ty));
                let v = self.declare(name);
                let ls = if self.carries(*var_ty) {
                    ls
                } else {
                    self.release(&ls, *span);
                    Vec::new()
                };
                self.var_loans.insert(v, ls.clone());
                self.emit(EvKind::Hold { var: v, loans: ls }, *span);
            }
            Stmt::Assign { target, value, span } => {
                let ls = self.value(value, Some(target.ty));
                match self.place_of(target) {
                    Some(pl) => {
                        self.emit(
                            EvKind::Access {
                                place: pl.clone(),
                                kind: AKind::Write,
                                loan: None,
                            },
                            target.span,
                        );
                        if pl.has_deref() {
                            // 참조가 가리키는 곳에 저장되는 값
                            self.flow_into_ref(pl.root, &ls, *span, 0);
                            self.release(&ls, *span);
                        } else {
                            let entry = self.var_loans.entry(pl.root).or_default();
                            for l in &ls {
                                if !entry.contains(l) {
                                    entry.push(*l);
                                }
                            }
                            self.emit(EvKind::Hold { var: pl.root, loans: ls }, *span);
                        }
                    }
                    None => self.release(&ls, *span),
                }
            }
            Stmt::Return { value, span } => {
                let ls = match value {
                    Some(v) => self.value(v, Some(self.ret)),
                    None => Vec::new(),
                };
                let sp = value.as_ref().map(|v| v.span).unwrap_or(*span);
                self.emit(EvKind::Return(ls), sp);
            }
            Stmt::While { cond, body, step, span } => {
                // 반복 간 효과를 보려고 두 번 펼친다. 변수와 loan은 복사마다 새로 만들어진다.
                // break/continue는 아무 사건도 만들지 않는다: 이후 사건이 모두 도달 가능하다고
                // 보는 것은 실제 경로의 상위 집합이라 안전한 쪽(오탐은 가능, 누락은 없음)이다.
                for _ in 0..2 {
                    let c = self.value(cond, None);
                    self.release(&c, *span);
                    let ls = self.block(body, None);
                    self.release(&ls, *span);
                    for s in step {
                        self.stmt(s);
                    }
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Expr(e) => {
                let ls = self.value(e, None);
                self.release(&ls, e.span);
            }
        }
    }

    /// place 표현식의 (뿌리 변수, 경로). 임시값 위의 place면 `None`.
    fn place_of(&mut self, e: &Expr) -> Option<Place> {
        match &e.kind {
            ExprKind::Var(n) => Some(Place {
                root: self.lookup(n),
                projs: Vec::new(),
            }),
            ExprKind::Deref(inner) if matches!(inner.ty, Ty::Ptr(..)) => {
                // raw 포인터 역참조는 추적하지 않는다 (unsafe의 책임).
                let ls = self.value(inner, None);
                self.release(&ls, inner.span);
                None
            }
            ExprKind::Deref(inner) => {
                if inner.is_place() {
                    let mut p = self.place_of(inner)?;
                    p.projs.push(Proj::Deref);
                    Some(p)
                } else {
                    let ls = self.value(inner, None);
                    self.release(&ls, inner.span);
                    None
                }
            }
            ExprKind::Index(base, idx) => {
                let l = self.value(idx, None);
                self.release(&l, idx.span);
                self.index_place(base)
            }
            ExprKind::Slice(base, lo, hi) => {
                for x in [lo, hi].into_iter().flatten() {
                    let l = self.value(x, None);
                    self.release(&l, x.span);
                }
                self.index_place(base)
            }
            ExprKind::Field(base, name) => {
                if base.is_place() {
                    let mut p = self.place_of(base)?;
                    if matches!(base.ty, Ty::Ref(..)) {
                        p.projs.push(Proj::Deref);
                    }
                    p.projs.push(Proj::Field(name.clone()));
                    Some(p)
                } else {
                    let ls = self.value(base, None);
                    self.release(&ls, base.span);
                    None
                }
            }
            _ => None,
        }
    }

    fn index_place(&mut self, base: &Expr) -> Option<Place> {
        if base.is_place() {
            let mut p = self.place_of(base)?;
            if matches!(base.ty, Ty::Ref(..)) {
                p.projs.push(Proj::Deref);
            }
            p.projs.push(Proj::Index);
            Some(p)
        } else {
            let ls = self.value(base, None);
            self.release(&ls, base.span);
            None
        }
    }

    /// 표현식을 평가한다. 돌려주는 것은 그 값이 들고 있는 loan들이다.
    /// `want`는 값이 놓일 자리의 타입이다 (`&mut T` -> `&T` 강등 판단용).
    fn value(&mut self, e: &Expr, want: Option<Ty>) -> Vec<LoanId> {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::NoneTag => Vec::new(),
            ExprKind::Var(_) | ExprKind::Field(..) | ExprKind::Deref(_) | ExprKind::Index(..) => self.place_value(e, want),
            ExprKind::Array(xs) => {
                for x in xs {
                    let l = self.value(x, None);
                    self.release(&l, x.span);
                }
                Vec::new()
            }
            ExprKind::TypedInt(..) | ExprKind::RepeatOf(..) | ExprKind::Generic(..) => unreachable!("resolved before borrowck"),
            ExprKind::Repeat(x, _) => {
                let l = self.value(x, None);
                self.release(&l, x.span);
                Vec::new()
            }
            ExprKind::Slice(..) => unreachable!("typeck only allows a slice expression directly under `&`"),
            ExprKind::Unary(_, x) | ExprKind::Cast(x, _) => {
                let l = self.value(x, None);
                self.release(&l, e.span);
                Vec::new()
            }
            ExprKind::Binary(_, l, r) => {
                let a = self.value(l, None);
                self.release(&a, e.span);
                let b = self.value(r, None);
                self.release(&b, e.span);
                Vec::new()
            }
            ExprKind::Call(name, args) => {
                let (ptys, ret) = self.ck.sigs[name.as_str()].clone();
                let mut per_arg: Vec<Vec<LoanId>> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    per_arg.push(self.value(a, Some(ptys[i])));
                }
                // 인자 참조가 `&mut` 인자 안으로 저장되는 흐름
                for (j, i) in self.summaries[name.as_str()].flows.clone() {
                    let src = per_arg[j].clone();
                    if src.is_empty() {
                        continue;
                    }
                    for l in per_arg[i].clone() {
                        let Some(pl) = self.loans[l].place.clone() else { continue };
                        if pl.has_deref() {
                            self.flow_into_ref(pl.root, &src, e.span, 0);
                        } else {
                            self.store_into_var(pl.root, &src, e.span);
                        }
                    }
                }
                let mut result: Vec<LoanId> = Vec::new();
                if self.carries(ret) {
                    for i in self.summaries[name.as_str()].ret.clone() {
                        for l in &per_arg[i] {
                            if !result.contains(l) {
                                result.push(*l);
                            }
                        }
                    }
                }
                let rest: Vec<LoanId> = per_arg
                    .iter()
                    .flatten()
                    .filter(|l| !result.contains(l))
                    .copied()
                    .collect();
                self.release(&rest, e.span);
                result
            }
            ExprKind::StructLit(name, inits) => {
                let fields = self.ck.structs.fields[name.as_str()].clone();
                let mut out: Vec<LoanId> = Vec::new();
                for init in inits {
                    let fty = fields.iter().find(|(n, _)| *n == init.name).map(|(_, t)| *t);
                    for l in self.value(&init.value, fty) {
                        if !out.contains(&l) {
                            out.push(l);
                        }
                    }
                }
                out
            }
            ExprKind::AddrOf(m, inner) => self.borrow(inner, *m, e.span),
            ExprKind::Str(_) => Vec::new(),
            ExprKind::MethodCall(..) => unreachable!("typeck rewrites method calls into plain calls"),
            ExprKind::Ctor(_, x) => self.value(x, None),
            ExprKind::Try(inner) => {
                let ls = self.value(inner, None);
                // 오류 값이 참조를 들고 있으면 `?`는 그 참조를 호출자에게 돌려보낸다.
                if let Ty::Result(_, er) = inner.ty {
                    if self.carries(*er) {
                        self.emit(EvKind::Return(ls.clone()), e.span);
                    }
                }
                ls
            }
            ExprKind::Match { scrut, ok, err } => {
                let ls = self.value(scrut, None);
                let id = self.next_if;
                self.next_if += 1;
                let mut result: Vec<LoanId> = Vec::new();
                for (k, arm) in [ok, err].into_iter().enumerate() {
                    self.arms.push((id, k as u8));
                    self.scopes.push((HashMap::new(), Vec::new()));
                    let v = self.declare(&arm.bind);
                    let held = if self.carries(arm.bind_ty) {
                        ls.clone()
                    } else {
                        self.release(&ls, e.span);
                        Vec::new()
                    };
                    self.var_loans.insert(v, held.clone());
                    self.emit(EvKind::Hold { var: v, loans: held }, arm.span);
                    let r = self.block(&arm.body, want);
                    self.release(&r, e.span);
                    let (_, vars) = self.scopes.pop().unwrap();
                    self.emit(EvKind::ScopeEnd(vars), arm.span);
                    self.arms.pop();
                    let d = block_diverges(&arm.body);
                    self.div.insert((id, k as u8), d);
                    if !d {
                        for l in r {
                            if !result.contains(&l) {
                                result.push(l);
                            }
                        }
                    }
                }
                if !result.is_empty() {
                    self.emit(EvKind::Retemp(result.clone()), e.span);
                }
                result
            }
            ExprKind::Unsafe(b) => self.block(b, want),
            ExprKind::If { cond, then, els } => {
                let c = self.value(cond, None);
                self.release(&c, e.span);
                let id = self.next_if;
                self.next_if += 1;
                let mut result: Vec<LoanId> = Vec::new();

                self.arms.push((id, 0));
                let t = self.block(then, want);
                self.release(&t, e.span);
                self.arms.pop();
                let td = block_diverges(then);
                self.div.insert((id, 0), td);
                if !td {
                    result.extend(t);
                }

                if let Some(eb) = els {
                    self.arms.push((id, 1));
                    let x = self.block(eb, want);
                    self.release(&x, e.span);
                    self.arms.pop();
                    let ed = block_diverges(eb);
                    self.div.insert((id, 1), ed);
                    if !ed {
                        for l in x {
                            if !result.contains(&l) {
                                result.push(l);
                            }
                        }
                    }
                }
                if !result.is_empty() {
                    self.emit(EvKind::Retemp(result.clone()), e.span);
                }
                result
            }
        }
    }

    fn place_value(&mut self, e: &Expr, want: Option<Ty>) -> Vec<LoanId> {
        let Some(pl) = self.place_of(e) else {
            return Vec::new();
        };
        let ty = e.ty;
        if let Ty::Ref(_, true) = ty {
            // `&mut`를 값으로 쓰면 암묵적 재차용이다. `&T` 자리면 공유 재차용이 된다.
            let shared = matches!(want, Some(Ty::Ref(_, false)));
            let mut p2 = pl.clone();
            p2.projs.push(Proj::Deref);
            let loan = self.new_loan(Some(p2.clone()), !shared, e.span, None);
            self.emit(
                EvKind::Access {
                    place: p2,
                    kind: if shared { AKind::BorrowShared } else { AKind::BorrowMut },
                    loan: Some(loan),
                },
                e.span,
            );
            let mut ls = vec![loan];
            ls.extend(self.loans_of_var(pl.root));
            return ls;
        }
        let kind = if crate::ownership::is_move(ty) {
            AKind::Move
        } else {
            AKind::Read
        };
        self.emit(
            EvKind::Access {
                place: pl.clone(),
                kind,
                loan: None,
            },
            e.span,
        );
        if self.carries(ty) {
            self.loans_of_var(pl.root)
        } else {
            Vec::new()
        }
    }

    fn borrow(&mut self, inner: &Expr, mutable: bool, span: Span) -> Vec<LoanId> {
        let Some(pl) = self.place_of(inner) else {
            return Vec::new();
        };
        let loan = self.new_loan(Some(pl.clone()), mutable, span, None);
        self.emit(
            EvKind::Access {
                place: pl.clone(),
                kind: if mutable { AKind::BorrowMut } else { AKind::BorrowShared },
                loan: Some(loan),
            },
            span,
        );
        let mut ls = vec![loan];
        if pl.has_deref() {
            // 참조를 거쳐 빌렸으므로 그 참조의 출처도 이어받는다.
            ls.extend(self.loans_of_var(pl.root));
        }
        ls
    }

    /// 변수 `root`가 `ls`를 쥐게 된다.
    fn store_into_var(&mut self, root: VarId, ls: &[LoanId], span: Span) {
        let entry = self.var_loans.entry(root).or_default();
        for l in ls {
            if !entry.contains(l) {
                entry.push(*l);
            }
        }
        self.emit(
            EvKind::Hold {
                var: root,
                loans: ls.to_vec(),
            },
            span,
        );
    }

    /// 참조 변수 `ref_var`가 가리키는 곳에 `ls`가 저장된다.
    /// 가리키는 곳이 지역 변수면 그 변수가 쥐게 되고, 매개변수면 호출자에게 보이는 흐름이 된다.
    fn flow_into_ref(&mut self, ref_var: VarId, ls: &[LoanId], span: Span, depth: usize) {
        if depth > 8 {
            return;
        }
        for l in self.loans_of_var(ref_var) {
            let param = self.loans[l].param;
            let place = self.loans[l].place.clone();
            match (param, place) {
                (Some(i), _) => {
                    for m in ls {
                        if let Some(j) = self.loans[*m].param {
                            if !self.flows.contains(&(j, i)) {
                                self.flows.push((j, i));
                            }
                        }
                    }
                    self.emit(EvKind::Escape(ls.to_vec()), span);
                }
                (None, Some(pl)) => {
                    if pl.has_deref() {
                        self.flow_into_ref(pl.root, ls, span, depth + 1);
                    } else {
                        self.store_into_var(pl.root, ls, span);
                    }
                }
                _ => {}
            }
        }
    }

    fn summary(&self) -> Summary {
        let mut ret: Vec<usize> = Vec::new();
        for ev in &self.events {
            if let EvKind::Return(ls) = &ev.kind {
                for l in ls {
                    if let Some(i) = self.loans[*l].param {
                        if !ret.contains(&i) {
                            ret.push(i);
                        }
                    }
                }
            }
        }
        ret.sort_unstable();
        let mut flows = self.flows.clone();
        flows.sort_unstable();
        Summary { ret, flows }
    }

    // ------------------------------------------------------------------ 검사

    fn show(&self, p: &Place) -> String {
        let mut s = self.var_names[p.root].clone();
        for (i, pr) in p.projs.iter().enumerate() {
            match pr {
                Proj::Field(f) => {
                    s.push('.');
                    s.push_str(f);
                }
                Proj::Index => s.push_str("[..]"),
                Proj::Deref => {
                    // 뒤에 필드나 인덱스가 오면 자동 역참조로 보이게 둔다 (`r.x`, `r[i]`).
                    if !matches!(p.projs.get(i + 1), Some(Proj::Field(_) | Proj::Index)) {
                        s = format!("*{s}");
                    }
                }
            }
        }
        s
    }

    fn reach(&self, p: usize, u: usize) -> bool {
        if u <= p {
            return false;
        }
        let ap = &self.events[p].arms;
        let au = &self.events[u].arms;
        for (i, a) in ap {
            for (j, b) in au {
                if i == j && a != b {
                    return false; // 같은 if의 서로 다른 분기
                }
            }
        }
        for (i, a) in ap {
            let diverges = self.div.get(&(*i, *a)).copied().unwrap_or(false);
            if diverges && !au.contains(&(*i, *a)) {
                return false; // return으로 끝나는 분기에서는 빠져나갈 수 없다
            }
        }
        true
    }

    fn analyze(&self) -> Result<(), Diag> {
        // 변수 사용 위치, loan 생성 위치, loan 보유자
        let mut uses: HashMap<VarId, Vec<(usize, Span)>> = HashMap::new();
        let mut created: HashMap<LoanId, usize> = HashMap::new();
        let mut holders: HashMap<LoanId, Vec<(VarId, usize)>> = HashMap::new();
        for (i, ev) in self.events.iter().enumerate() {
            match &ev.kind {
                EvKind::Access { place, loan, .. } => {
                    uses.entry(place.root).or_default().push((i, ev.span));
                    if let Some(l) = loan {
                        created.insert(*l, i);
                    }
                }
                EvKind::Hold { var, loans } => {
                    for l in loans {
                        holders.entry(*l).or_default().push((*var, i));
                    }
                }
                _ => {}
            }
        }

        // loan이 p 시점에 살아 있는가: 임시값이거나, 보유자가 이후에 쓰인다.
        let later_use = |l: LoanId, p: usize| -> Option<Span> {
            for (v, h) in holders.get(&l).into_iter().flatten() {
                if *h < p && self.reach(*h, p) {
                    for (u, sp) in uses.get(v).into_iter().flatten() {
                        if *u > p && self.reach(p, *u) {
                            return Some(*sp);
                        }
                    }
                }
            }
            None
        };

        let mut temp: HashSet<LoanId> = HashSet::new();
        for (p, ev) in self.events.iter().enumerate() {
            match &ev.kind {
                EvKind::Access { place, kind, loan } => {
                    for (l, info) in self.loans.iter().enumerate() {
                        let Some(lp) = &info.place else { continue };
                        if Some(l) == *loan {
                            continue;
                        }
                        let Some(&c) = created.get(&l) else { continue };
                        if c >= p || !self.reach(c, p) || !lp.overlaps(place) {
                            continue;
                        }
                        let conflict = match kind {
                            AKind::Read | AKind::BorrowShared => info.mutable,
                            AKind::Write | AKind::Move | AKind::BorrowMut => true,
                        };
                        if !conflict {
                            continue;
                        }
                        let later = later_use(l, p);
                        if !temp.contains(&l) && later.is_none() {
                            continue;
                        }
                        return Err(self.conflict_diag(place, *kind, ev.span, lp, info, later));
                    }
                    if let Some(l) = loan {
                        temp.insert(*l);
                    }
                }
                EvKind::Hold { loans, .. } => {
                    for l in loans {
                        temp.remove(l);
                    }
                }
                EvKind::Release(loans) => {
                    for l in loans {
                        temp.remove(l);
                    }
                }
                EvKind::Retemp(loans) => {
                    for l in loans {
                        temp.insert(*l);
                    }
                }
                EvKind::ScopeEnd(vars) => {
                    for (l, info) in self.loans.iter().enumerate() {
                        let Some(lp) = &info.place else { continue };
                        if lp.has_deref() || !vars.contains(&lp.root) {
                            continue;
                        }
                        let Some(&c) = created.get(&l) else { continue };
                        if c >= p || !self.reach(c, p) {
                            continue;
                        }
                        let later = later_use(l, p);
                        if !temp.contains(&l) && later.is_none() {
                            continue;
                        }
                        let name = self.show(lp);
                        let mut d = Diag::new(
                            format!("`{}` does not live long enough", self.var_names[lp.root]),
                            info.span,
                            "borrowed value does not live long enough",
                        );
                        if let Some(u) = later {
                            d = d.with_secondary(u, "borrow later used here");
                        } else {
                            d = d.with_secondary(
                                ev.span,
                                format!("`{name}` goes out of scope here while still borrowed"),
                            );
                        }
                        return Err(d);
                    }
                }
                EvKind::Return(loans) => {
                    for l in loans {
                        let info = &self.loans[*l];
                        let Some(lp) = &info.place else { continue };
                        if lp.has_deref() {
                            continue;
                        }
                        return Err(Diag::new(
                            format!(
                                "cannot return reference to local variable `{}`",
                                self.var_names[lp.root]
                            ),
                            ev.span,
                            "returns a reference to data owned by the current function",
                        )
                        .with_secondary(
                            info.span,
                            format!("`{}` is borrowed here", self.var_names[lp.root]),
                        ));
                    }
                }
                EvKind::Escape(loans) => {
                    for l in loans {
                        let info = &self.loans[*l];
                        let Some(lp) = &info.place else { continue };
                        if lp.has_deref() {
                            continue;
                        }
                        return Err(Diag::new(
                            format!("`{}` does not live long enough", self.var_names[lp.root]),
                            info.span,
                            "borrowed value does not live long enough",
                        )
                        .with_secondary(
                            ev.span,
                            "the reference is stored where it outlives this function's locals",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn conflict_diag(
        &self,
        place: &Place,
        kind: AKind,
        at: Span,
        loan_place: &Place,
        loan: &Loan,
        later: Option<Span>,
    ) -> Diag {
        let what = self.show(place);
        let held = self.show(loan_place);
        let (msg, label, note) = match (kind, loan.mutable) {
            (AKind::BorrowMut, true) => (
                format!("cannot borrow `{what}` as mutable more than once at a time"),
                "second mutable borrow occurs here".to_string(),
                "first mutable borrow occurs here".to_string(),
            ),
            (AKind::BorrowMut, false) => (
                format!("cannot borrow `{what}` as mutable because it is also borrowed as immutable"),
                "mutable borrow occurs here".to_string(),
                "immutable borrow occurs here".to_string(),
            ),
            (AKind::BorrowShared, _) => (
                format!("cannot borrow `{what}` as immutable because it is also borrowed as mutable"),
                "immutable borrow occurs here".to_string(),
                "mutable borrow occurs here".to_string(),
            ),
            (AKind::Write, _) => (
                format!("cannot assign to `{what}` because it is borrowed"),
                format!("assignment to borrowed `{what}` occurs here"),
                format!("`{held}` is borrowed here"),
            ),
            (AKind::Move, _) => (
                format!("cannot move out of `{what}` because it is borrowed"),
                "move out occurs here".to_string(),
                format!("`{held}` is borrowed here"),
            ),
            (AKind::Read, _) => (
                format!("cannot use `{what}` because it was mutably borrowed"),
                format!("use of borrowed `{what}`"),
                "mutable borrow occurs here".to_string(),
            ),
        };
        let mut d = Diag::new(msg, at, label).with_secondary(loan.span, note);
        if let Some(u) = later {
            d = d.with_secondary(u, "first borrow later used here");
        }
        d
    }
}
