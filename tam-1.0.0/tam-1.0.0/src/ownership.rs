//! 소유권(이동) 검사. 타입 검사를 통과한 AST(`Expr::ty` 채워짐)에서 동작한다.
//!
//! 규칙 (M6):
//! - 정수·실수·`bool`·참조는 Copy, 구조체는 이동 타입이다.
//! - 이동 타입 값을 값으로 쓰면(`let b = a`, 인자 전달, 반환, 구조체 필드 초기화 등) 이동한다.
//! - 이동한 값(또는 그 일부)을 다시 쓰거나 빌리면 오류다.
//! - 참조 뒤에 있는 이동 타입 값은 꺼낼 수 없다 (`*r`, `r.field`).
//! - 분기는 "한쪽에서라도 이동했으면 이동됨"으로 합친다. 루프는 두 번 돌려서
//!   "이전 반복에서 이동됨"을 잡는다. 재대입은 값을 되살린다.
//!
//! 참조의 배타성(`&mut` 중복 등)과 dangling은 M7에서 검사한다.

use std::collections::{BTreeMap, HashMap};

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::types::Ty;
use crate::typeck::{needs_drop, Checked, StructTable};

type VarId = usize;
type Path = (VarId, Vec<String>);

pub fn is_move(ty: Ty) -> bool {
    match ty {
        Ty::Struct(_) | Ty::String | Ty::Array(..) => true,
        Ty::Result(t, e) => is_move(*t) || is_move(*e),
        _ => false,
    }
}

pub fn check(p: &Program, ck: &Checked) -> Result<(), Diag> {
    for f in &p.funcs {
        let mut o = Own {
            structs: &ck.structs,
            names: Vec::new(),
            scopes: vec![HashMap::new()],
            state: Some(State::default()),
            error: None,
            silent: false,
            loops: Vec::new(),
        };
        for param in &f.params {
            o.declare(&param.name);
        }
        o.block(&f.body);
        if let Some(d) = o.error {
            return Err(d);
        }
    }
    Ok(())
}

#[derive(Clone, Default)]
struct State {
    /// 이동된 경로 -> 이동된 위치
    moved: BTreeMap<Path, Span>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    /// 값으로 소비 (이동 타입이면 이동)
    Value,
    /// `&`/`&mut`로 빌림
    Borrow,
}

struct PlaceInfo {
    root: Option<VarId>,
    fields: Vec<String>,
    through_ref: bool,
    /// 배열/슬라이스 원소를 거친 place (원소는 이동할 수 없다)
    through_index: bool,
}

struct Own<'a> {
    structs: &'a StructTable,
    names: Vec<String>,
    scopes: Vec<HashMap<String, VarId>>,
    /// `None`이면 도달 불가능한 지점(return 이후 등)
    state: Option<State>,
    error: Option<Diag>,
    /// 루프의 첫 번째 패스처럼 오류를 기록하지 않는 모드
    silent: bool,
    /// 감싸고 있는 루프들의 break/continue 시점 상태
    loops: Vec<LoopCtx>,
}

#[derive(Default)]
struct LoopCtx {
    brk: Option<State>,
    cont: Option<State>,
}

fn is_prefix(a: &[String], b: &[String]) -> bool {
    a.len() <= b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

fn merge(a: Option<State>, b: Option<State>) -> Option<State> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(mut a), Some(b)) => {
            for (k, v) in b.moved {
                a.moved.entry(k).or_insert(v);
            }
            Some(a)
        }
    }
}

impl<'a> Own<'a> {
    fn declare(&mut self, name: &str) -> VarId {
        let id = self.names.len();
        self.names.push(name.to_string());
        self.scopes.last_mut().unwrap().insert(name.to_string(), id);
        id
    }

    fn lookup(&self, name: &str) -> Option<VarId> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn display(&self, root: VarId, fields: &[String]) -> String {
        let mut s = self.names[root].clone();
        for f in fields {
            s.push('.');
            s.push_str(f);
        }
        s
    }

    fn report(&mut self, d: Diag) {
        if !self.silent && self.error.is_none() && self.state.is_some() {
            self.error = Some(d);
        }
    }

    // ---- 블록과 문장 ----

    fn block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        for s in &b.stmts {
            self.stmt(s);
        }
        if let Some(t) = &b.tail {
            self.value(t);
        }
        self.scopes.pop();
    }

    /// 본문 + (continue가 합류하는) 증가 문장들.
    fn loop_body(&mut self, body: &Block, step: &[Stmt]) {
        self.block(body);
        let cont = self.loops.last_mut().and_then(|c| c.cont.take());
        self.state = merge(self.state.take(), cont);
        for s in step {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { name, init, .. } => {
                self.value(init);
                self.declare(name);
            }
            Stmt::Assign { target, value, .. } => {
                self.value(value);
                self.assign_target(target);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.value(v);
                }
                self.state = None;
            }
            Stmt::While { cond, body, step, .. } => {
                let entry = self.state.clone();
                // 1패스(조용히): 본문이 만드는 이동을 모은다.
                let saved = std::mem::replace(&mut self.silent, true);
                self.loops.push(LoopCtx::default());
                self.value(cond);
                self.loop_body(body, step);
                let out1 = self.state.take();
                self.loops.pop();
                self.silent = saved;
                // 2패스: 이전 반복에서 이동된 값을 쓰는지 검사한다.
                self.state = merge(entry, out1);
                self.value(cond);
                let exit = self.state.clone();
                self.loops.push(LoopCtx::default());
                self.loop_body(body, step);
                let ctx = self.loops.pop().unwrap();
                self.state = merge(exit, ctx.brk);
            }
            Stmt::Break { .. } => {
                let st = self.state.take();
                if let Some(c) = self.loops.last_mut() {
                    c.brk = merge(c.brk.take(), st);
                }
            }
            Stmt::Continue { .. } => {
                let st = self.state.take();
                if let Some(c) = self.loops.last_mut() {
                    c.cont = merge(c.cont.take(), st);
                }
            }
            Stmt::Expr(e) => self.value(e),
        }
    }

    /// 재대입: 값을 되살린다.
    fn assign_target(&mut self, target: &Expr) {
        let pi = self.place_info(target);
        if pi.through_index {
            // 원소에 대입해도 이동된 배열이 되살아나지는 않는다.
            if let Some(root) = pi.root {
                self.check_not_moved(root, &pi.fields, Use::Borrow, target.span);
            }
            return;
        }
        let Some(root) = pi.root else { return };
        let Some(st) = self.state.as_ref() else { return };
        // 통째로(또는 상위가) 이동된 값의 일부에는 대입할 수 없다.
        let blocker = st
            .moved
            .iter()
            .find(|((r, mf), _)| *r == root && mf.len() < pi.fields.len() && is_prefix(mf, &pi.fields))
            .map(|((r, mf), sp)| (*r, mf.clone(), *sp));
        if let Some((r, mf, moved_at)) = blocker {
            let what = self.display(r, &mf);
            self.report(
                Diag::new(
                    format!("cannot assign to part of moved value `{what}`"),
                    target.span,
                    "assigned here",
                )
                .with_secondary(moved_at, "value moved here"),
            );
            return;
        }
        if let Some(st) = self.state.as_mut() {
            st.moved
                .retain(|(r, mf), _| !(*r == root && is_prefix(&pi.fields, mf)));
        }
    }

    // ---- 표현식 ----

    /// 표현식을 평가해서 부모가 값으로 소비한다.
    fn value(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::NoneTag => {}
            ExprKind::Try(x) | ExprKind::Ctor(_, x) => self.value(x),
            ExprKind::Var(_) | ExprKind::Field(..) | ExprKind::Deref(_) | ExprKind::Index(..) | ExprKind::Slice(..) => {
                self.use_place(e, Use::Value);
            }
            ExprKind::Array(xs) => {
                for x in xs {
                    self.value(x);
                }
            }
            ExprKind::TypedInt(..) | ExprKind::RepeatOf(..) | ExprKind::Generic(..) => unreachable!("resolved before ownership"),
            ExprKind::Repeat(x, _) => self.value(x),
            ExprKind::Unary(_, x) | ExprKind::Cast(x, _) => self.value(x),
            ExprKind::Binary(_, l, r) => {
                self.value(l);
                self.value(r);
            }
            ExprKind::Call(_, args) => {
                for a in args {
                    self.value(a);
                }
            }
            ExprKind::StructLit(_, inits) => {
                for i in inits {
                    self.value(&i.value);
                }
            }
            ExprKind::AddrOf(_, inner) => self.use_place(inner, Use::Borrow),
            ExprKind::MethodCall(..) => unreachable!("typeck rewrites method calls into plain calls"),
            ExprKind::Unsafe(b) => self.block(b),
            ExprKind::Match { scrut, ok, err } => {
                self.value(scrut);
                let entry = self.state.clone();
                let mut outs = Vec::new();
                for arm in [ok, err] {
                    self.state = entry.clone();
                    self.scopes.push(HashMap::new());
                    self.declare(&arm.bind);
                    self.block(&arm.body);
                    self.scopes.pop();
                    outs.push(self.state.take());
                }
                let b = outs.pop().unwrap();
                let a = outs.pop().unwrap();
                self.state = merge(a, b);
            }
            ExprKind::If { cond, then, els } => {
                self.value(cond);
                let entry = self.state.clone();
                self.block(then);
                let after_then = self.state.take();
                self.state = entry;
                if let Some(eb) = els {
                    self.block(eb);
                }
                let after_else = self.state.take();
                self.state = merge(after_then, after_else);
            }
        }
    }

    /// place를 분해한다. 도중에 평가해야 하는 하위 표현식은 여기서 평가한다.
    fn place_info(&mut self, e: &Expr) -> PlaceInfo {
        match &e.kind {
            ExprKind::Var(n) => PlaceInfo {
                root: self.lookup(n),
                fields: Vec::new(),
                through_ref: false,
                through_index: false,
            },
            ExprKind::Deref(inner) => {
                self.value(inner);
                PlaceInfo {
                    root: None,
                    fields: Vec::new(),
                    through_ref: true,
                    through_index: false,
                }
            }
            ExprKind::Index(base, idx) => {
                self.value(idx);
                self.index_place(base, e)
            }
            ExprKind::Slice(base, lo, hi) => {
                for x in [lo, hi].into_iter().flatten() {
                    self.value(x);
                }
                self.index_place(base, e)
            }
            ExprKind::Field(base, name) => {
                if matches!(base.ty, Ty::Ref(..)) {
                    self.value(base);
                    PlaceInfo {
                        root: None,
                        fields: Vec::new(),
                        through_ref: true,
                        through_index: false,
                    }
                } else if base.is_place() {
                    let mut pi = self.place_info(base);
                    pi.fields.push(name.clone());
                    pi
                } else {
                    // 임시값의 필드 (`make().x`): 추적하지 않는다. 힙을 쥔 임시값이면 해제할 수 없으므로 막는다.
                    if needs_drop(base.ty, self.structs) {
                        self.report(Diag::new(
                            format!("cannot access a field of a temporary `{}` that owns heap data", base.ty.name()),
                            e.span,
                            "bind it to a variable first (`let t = ...`)",
                        ));
                    }
                    self.value(base);
                    PlaceInfo {
                        root: None,
                        fields: Vec::new(),
                        through_ref: false,
                        through_index: false,
                    }
                }
            }
            _ => {
                self.value(e);
                PlaceInfo {
                    root: None,
                    fields: Vec::new(),
                    through_ref: false,
                    through_index: false,
                }
            }
        }
    }

    /// `base[..]`의 place. 원소를 값으로 꺼내 이동하는 것은 막고, 배열 자체가 이동됐는지는 본다.
    fn index_place(&mut self, base: &Expr, _e: &Expr) -> PlaceInfo {
        let mut pi = if matches!(base.ty, Ty::Ref(..)) || !base.is_place() {
            self.value(base);
            PlaceInfo { root: None, fields: Vec::new(), through_ref: true, through_index: true }
        } else {
            self.place_info(base)
        };
        pi.through_ref = true;
        pi.through_index = true;
        pi
    }

    fn use_place(&mut self, e: &Expr, how: Use) {
        let pi = self.place_info(e);

        if let Some(root) = pi.root {
            self.check_not_moved(root, &pi.fields, how, e.span);
        }

        if how == Use::Value && is_move(e.ty) {
            if !pi.fields.is_empty() && !pi.through_ref && needs_drop(e.ty, self.structs) {
                self.report(Diag::new(
                    format!("cannot move `{}` out of a field: it owns heap data", e.ty.name()),
                    e.span,
                    "the containing value is released as a whole; move the whole value instead",
                ));
            } else if pi.through_index {
                self.report(Diag::new(
                    format!("cannot move out of an array element of type `{}`", e.ty.name()),
                    e.span,
                    "borrow it (`&a[i]`) or use one of its fields (`a[i].x`)",
                ));
            } else if pi.through_ref {
                self.report(Diag::new(
                    format!("cannot move out of a value of type `{}` behind a reference", e.ty.name()),
                    e.span,
                    "move occurs here",
                ));
            } else if let Some(root) = pi.root {
                if let Some(st) = self.state.as_mut() {
                    st.moved.insert((root, pi.fields), e.span);
                }
            }
        }
    }

    fn check_not_moved(&mut self, root: VarId, fields: &[String], how: Use, at: Span) {
        let Some(st) = self.state.as_ref() else { return };
        let mut found: Option<(String, Span, bool)> = None;
        for ((r, mf), moved_at) in &st.moved {
            if *r != root {
                continue;
            }
            if is_prefix(mf, fields) {
                // 이동된 값 자체이거나 그 안쪽을 쓴다.
                found = Some((self.display(*r, mf), *moved_at, false));
                break;
            }
            if is_prefix(fields, mf) {
                // 일부가 이동된 값을 통째로 쓴다.
                found = Some((self.display(root, fields), *moved_at, true));
                break;
            }
        }
        let Some((what, moved_at, partial)) = found else { return };

        let (verb, label) = match how {
            Use::Value => ("use", "value used here after move"),
            Use::Borrow => ("borrow", "value borrowed here after move"),
        };
        let kind = if partial { "partially moved" } else { "moved" };
        let in_loop = (moved_at.line, moved_at.col) >= (at.line, at.col);
        let note = if in_loop {
            "value moved here, in previous iteration of loop"
        } else {
            "value moved here"
        };
        self.report(
            Diag::new(format!("{verb} of {kind} value `{what}`"), at, label)
                .with_secondary(moved_at, note),
        );
    }
}
