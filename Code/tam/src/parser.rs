//! 재귀 하강 파서. 문장 종료는 `Newline`/`;` 토큰이다.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{lex_file, Tok, Token};
use crate::types::Ty;

pub fn parse(src: &str) -> Result<Program, Diag> {
    parse_file(src, 0)
}

/// `file`은 진단에 찍히는 파일 id.
pub fn parse_file(src: &str, file: u16) -> Result<Program, Diag> {
    let toks = lex_file(src, file)?;
    Parser {
        toks,
        pos: 0,
        no_struct: false,
        depth: 0,
        self_ty: None,
        pending: Vec::new(),
        nfor: 0,
    }
    .program()
}

/// 구문 중첩 한계. 재귀하는 검사기·코드 생성기가 기본 스택으로 안전하게 처리하는 범위다.
const MAX_DEPTH: u32 = 200;
/// 한 식 안의 연산자 사슬 길이 한계(`a + b + c + …`은 왼쪽으로 깊어지는 트리가 된다).
const MAX_CHAIN: u32 = 2000;

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// `if`/`while` 조건 안에서는 `Name {`를 구조체 리터럴로 읽지 않는다(블록과 구분).
    no_struct: bool,
    /// `impl T { }` 안이면 `T`. (`Self`와 `self` 매개변수가 이 타입을 가리킨다.)
    self_ty: Option<String>,
    /// `for` 같은 desugar가 한 문장 앞에 끼워 넣는 문장들.
    pending: Vec<Stmt>,
    nfor: u32,
    /// 중첩 깊이(괄호, 블록, 단항 연산자, 타입). 너무 깊으면 이후 단계가 스택을 넘기므로 여기서 막는다.
    depth: u32,
}

enum Item {
    Stmt(Stmt),
    Expr(Expr),
}

/// 두 번 평가해도 부작용이 없는 place인가. (`a[i] += 1`처럼 대상을 복제하는 desugar용)
fn pure_place(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_) | ExprKind::Int(_) | ExprKind::Bool(_) => true,
        ExprKind::Field(b, _) | ExprKind::Deref(b) => pure_place(b),
        ExprKind::Index(b, i) => pure_place(b) && pure_place(i),
        ExprKind::Binary(_, a, b) => pure_place(a) && pure_place(b),
        ExprKind::Unary(_, a) => pure_place(a),
        ExprKind::Cast(a, _) => pure_place(a),
        _ => false,
    }
}

fn binop_info(t: &Tok) -> Option<(BinOp, u8)> {
    Some(match t {
        Tok::OrOr => (BinOp::Or, 1),
        Tok::AndAnd => (BinOp::And, 2),
        Tok::EqEq => (BinOp::Eq, 3),
        Tok::Ne => (BinOp::Ne, 3),
        Tok::Lt => (BinOp::Lt, 4),
        Tok::Le => (BinOp::Le, 4),
        Tok::Gt => (BinOp::Gt, 4),
        Tok::Ge => (BinOp::Ge, 4),
        Tok::Pipe => (BinOp::BitOr, 5),
        Tok::Caret => (BinOp::BitXor, 6),
        Tok::Amp => (BinOp::BitAnd, 7),
        Tok::Shl => (BinOp::Shl, 8),
        Tok::Plus => (BinOp::Add, 9),
        Tok::Minus => (BinOp::Sub, 9),
        Tok::Star => (BinOp::Mul, 10),
        Tok::Slash => (BinOp::Div, 10),
        Tok::Percent => (BinOp::Rem, 10),
        _ => return None,
    })
}

impl Parser {
    // ---- 토큰 유틸 ----

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn unexpected(&self, expected: &str) -> Diag {
        let tok = &self.toks[self.pos];
        Diag::new(
            format!("expected {expected}, found {}", tok.tok.describe()),
            tok.span,
            format!("expected {expected}"),
        )
    }

    fn expect(&mut self, t: Tok, what: &str) -> Result<Token, Diag> {
        if self.at(&t) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(what))
        }
    }

    fn ident(&mut self, what: &str) -> Result<(String, Span), Diag> {
        if let Tok::Ident(name) = self.peek().clone() {
            let span = self.bump().span;
            Ok((name, span))
        } else {
            Err(self.unexpected(what))
        }
    }

    fn skip_terms(&mut self) {
        while matches!(self.peek(), Tok::Newline | Tok::Semi) {
            self.bump();
        }
    }

    fn skip_newlines(&mut self) {
        while self.at(&Tok::Newline) {
            self.bump();
        }
    }

    // ---- 최상위 ----

    fn program(&mut self) -> Result<Program, Diag> {
        let mut uses = Vec::new();
        let mut structs = Vec::new();
        let mut funcs = Vec::new();
        self.skip_terms();
        while !self.at(&Tok::Eof) {
            if self.at(&Tok::Use) {
                uses.push(self.use_decl()?);
                self.skip_terms();
                continue;
            }
            let public = self.eat(&Tok::Pub);
            match self.peek() {
                Tok::Struct => {
                    let mut s = self.struct_def()?;
                    s.public = public;
                    structs.push(s);
                }
                Tok::Fn | Tok::Unsafe | Tok::Extern => {
                    let mut f = self.func()?;
                    f.public = public;
                    funcs.push(f);
                }
                Tok::Impl => {
                    if public {
                        return Err(Diag::new("`pub` cannot be applied to an `impl` block", self.toks[self.pos].span, "mark the methods `pub` instead"));
                    }
                    self.impl_block(&mut funcs)?;
                }
                _ => return Err(self.unexpected("`fn`, `struct`, `impl` or `use`")),
            }
            self.skip_terms();
        }
        Ok(Program {
            uses,
            structs,
            funcs,
        })
    }

    /// `impl T { fn .. }`: 각 함수를 `T.name`이라는 일반 함수로 풀어 놓는다.
    fn impl_block(&mut self, funcs: &mut Vec<Func>) -> Result<(), Diag> {
        self.expect(Tok::Impl, "`impl`")?;
        let (ty, _) = self.path_name("a type name")?;
        let open = self.expect(Tok::LBrace, "`{`")?.span;
        self.self_ty = Some(ty.clone());
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            if self.at(&Tok::Eof) {
                self.self_ty = None;
                return Err(Diag::new("unclosed `impl` block", open, "block opened here"));
            }
            let public = self.eat(&Tok::Pub);
            if !matches!(self.peek(), Tok::Fn | Tok::Unsafe) {
                self.self_ty = None;
                return Err(self.unexpected("`fn` inside an `impl` block"));
            }
            let mut f = match self.func() {
                Ok(f) => f,
                Err(e) => {
                    self.self_ty = None;
                    return Err(e);
                }
            };
            f.public = public;
            f.name = format!("{ty}.{}", f.name);
            f.impl_ty = Some(ty.clone());
            funcs.push(f);
        }
        self.self_ty = None;
        self.expect(Tok::RBrace, "`}`")?;
        Ok(())
    }

    fn use_decl(&mut self) -> Result<UseDecl, Diag> {
        let kw = self.expect(Tok::Use, "`use`")?;
        let (first, mut end) = self.ident("a module path")?;
        let mut path = vec![first];
        while self.eat(&Tok::Dot) {
            let (seg, sp) = self.ident("a module name")?;
            end = sp;
            path.push(seg);
        }
        self.end_of_stmt()?;
        let alias = path.last().cloned().unwrap_or_default();
        Ok(UseDecl {
            path,
            alias,
            span: kw.span.to(end),
        })
    }

    /// `a::b::c` 형태의 경로를 하나의 문자열로 읽는다.
    fn path_name(&mut self, what: &str) -> Result<(String, Span), Diag> {
        let (mut name, mut span) = self.ident(what)?;
        if name == "Self" {
            match &self.self_ty {
                Some(t) => name = t.clone(),
                None => {
                    return Err(Diag::new("`Self` is only available inside an `impl` block", span, "no impl type here"))
                }
            }
        }
        while self.at(&Tok::ColonColon) {
            self.bump();
            let (seg, sp) = self.ident("a name after `::`")?;
            name.push_str("::");
            name.push_str(&seg);
            span = span.to(sp);
        }
        Ok((name, span))
    }

    fn struct_def(&mut self) -> Result<StructDef, Diag> {
        let kw = self.expect(Tok::Struct, "`struct`")?;
        let (name, name_span) = self.ident("struct name")?;
        let open = self.expect(Tok::LBrace, "`{`")?.span;
        let mut fields = Vec::new();
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(Diag::new("unclosed struct", open, "struct opened here"));
            }
            let (fname, fspan) = self.ident("field name")?;
            self.expect(Tok::Colon, "`:` and a type")?;
            let ty = self.type_ref()?;
            fields.push(FieldDef {
                name: fname,
                ty,
                span: fspan,
            });
            // 필드는 `,`, 줄바꿈, `;` 중 아무거나로 구분한다.
            if !self.eat(&Tok::Comma) {
                self.end_of_stmt()?;
            }
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(StructDef {
            public: false,
            name,
            fields,
            span: kw.span.to(name_span),
        })
    }

    fn func(&mut self) -> Result<Func, Diag> {
        let ext = if self.eat(&Tok::Extern) {
            match self.peek().clone() {
                Tok::Str(lib) => {
                    self.bump();
                    Some(lib)
                }
                _ => return Err(self.unexpected("a library name string after `extern`, e.g. `extern \"kernel32\" fn`")),
            }
        } else {
            None
        };
        let unsafe_ = self.eat(&Tok::Unsafe);
        let kw = self.expect(Tok::Fn, "`fn`")?;
        let (name, name_span) = self.ident("function name")?;
        self.expect(Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            if params.is_empty() {
                if let Some(sp) = self.self_param(&mut params) {
                    let _ = sp;
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                    continue;
                }
            }
            let (pname, pspan) = self.ident("parameter name")?;
            self.expect(Tok::Colon, "`:` and a type")?;
            let ty = self.type_ref()?;
            params.push(Param {
                name: pname,
                ty,
                span: pspan,
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        let no_ret = if ext.is_some() {
            matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::Eof | Tok::Assign)
        } else {
            self.at(&Tok::LBrace)
        };
        let ret = if no_ret { None } else { Some(self.type_ref()?) };
        let mut link = None;
        let body = if ext.is_some() {
            if self.eat(&Tok::Assign) {
                match self.peek().clone() {
                    Tok::Str(sym) => {
                        self.bump();
                        link = Some(sym);
                    }
                    _ => return Err(self.unexpected("the symbol name as a string, e.g. `= \"write\"`")),
                }
            }
            self.end_of_stmt()?;
            Block {
                stmts: Vec::new(),
                tail: None,
                span: kw.span,
            }
        } else {
            self.block()?
        };
        Ok(Func {
            public: false,
            unsafe_,
            ext,
            link,
            name,
            params,
            ret,
            impl_ty: None,
            body,
            span: kw.span.to(name_span),
        })
    }

    /// `impl` 안 첫 매개변수 `self`, `&self`, `&mut self`를 읽는다. (아니면 아무것도 소비하지 않는다.)
    fn self_param(&mut self, params: &mut Vec<Param>) -> Option<()> {
        let ty = self.self_ty.clone()?;
        let mut k = self.pos;
        let amp = self.toks[k].tok == Tok::Amp;
        let mut mutable = false;
        if amp {
            k += 1;
            if self.toks[k].tok == Tok::Mut {
                mutable = true;
                k += 1;
            }
        }
        if self.toks[k].tok != Tok::Ident("self".to_string()) {
            return None;
        }
        let span = self.toks[k].span;
        self.pos = k + 1;
        let named = TypeRef { kind: TypeKind::Named(ty), span };
        let ty = if amp { TypeRef { kind: TypeKind::Ref(mutable, Box::new(named)), span } } else { named };
        params.push(Param { name: "self".to_string(), ty, span });
        Some(())
    }

    fn type_ref(&mut self) -> Result<TypeRef, Diag> {
        self.enter()?;
        let r = self.type_ref_inner();
        self.depth -= 1;
        r
    }

    fn type_ref_inner(&mut self) -> Result<TypeRef, Diag> {
        if self.at(&Tok::Amp) {
            let amp = self.bump();
            let mutable = self.eat(&Tok::Mut);
            let inner = self.type_ref()?;
            let span = amp.span.to(inner.span);
            return Ok(TypeRef {
                kind: TypeKind::Ref(mutable, Box::new(inner)),
                span,
            });
        }
        if self.at(&Tok::AndAnd) {
            // `&&T`는 `& &T`다.
            let amp = self.bump();
            let mutable = self.eat(&Tok::Mut);
            let inner = self.type_ref()?;
            let span = amp.span.to(inner.span);
            let mid = TypeRef { kind: TypeKind::Ref(mutable, Box::new(inner)), span };
            return Ok(TypeRef { kind: TypeKind::Ref(false, Box::new(mid)), span });
        }
        if self.at(&Tok::Star) {
            let star = self.bump();
            let mutable = if self.eat(&Tok::Mut) {
                true
            } else if self.eat(&Tok::Const) {
                false
            } else {
                return Err(self.unexpected("`const` or `mut` after `*` in a pointer type"));
            };
            let inner = self.type_ref()?;
            let span = star.span.to(inner.span);
            return Ok(TypeRef {
                kind: TypeKind::Ptr(mutable, Box::new(inner)),
                span,
            });
        }
        if self.at(&Tok::LBracket) {
            let open = self.bump();
            let elem = self.type_ref()?;
            if self.eat(&Tok::Semi) {
                let nt = self.toks[self.pos].clone();
                let Tok::Int(n) = nt.tok else {
                    return Err(self.unexpected("an array length (integer literal)"));
                };
                self.bump();
                let close = self.expect(Tok::RBracket, "`]`")?;
                if n < 1 {
                    return Err(Diag::new("array length must be at least 1", nt.span, "zero-length arrays are not supported"));
                }
                return Ok(TypeRef {
                    kind: TypeKind::Array(Box::new(elem), n as u64),
                    span: open.span.to(close.span),
                });
            }
            let close = self.expect(Tok::RBracket, "`;` and a length, or `]`")?;
            return Ok(TypeRef {
                kind: TypeKind::Slice(Box::new(elem)),
                span: open.span.to(close.span),
            });
        }
        let (name, mut span) = self.path_name("a type")?;
        if self.at(&Tok::Lt) {
            self.bump();
            let mut args = Vec::new();
            loop {
                args.push(self.type_ref()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            let close = self.expect(Tok::Gt, "`>`")?;
            span = span.to(close.span);
            return Ok(TypeRef {
                kind: TypeKind::Generic(name, args),
                span,
            });
        }
        Ok(TypeRef {
            kind: TypeKind::Named(name),
            span,
        })
    }

    fn enter(&mut self) -> Result<(), Diag> {
        if self.depth >= MAX_DEPTH {
            let span = self.toks.get(self.pos).map(|t| t.span).unwrap_or(Span::new(1, 1, 1));
            return Err(Diag::new(
                format!("nesting is too deep (more than {MAX_DEPTH} levels)"),
                span,
                "split this into smaller functions or statements",
            ));
        }
        self.depth += 1;
        Ok(())
    }

    // ---- 블록과 문장 ----

    fn block(&mut self) -> Result<Block, Diag> {
        self.enter()?;
        let r = self.block_inner();
        self.depth -= 1;
        r
    }

    fn block_inner(&mut self) -> Result<Block, Diag> {
        let open = self.expect(Tok::LBrace, "`{`")?.span;
        let mut stmts = Vec::new();
        let mut tail = None;
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(Diag::new("unclosed block", open, "block opened here"));
            }
            match self.item()? {
                Item::Stmt(s) => {
                    self.end_of_stmt()?;
                    stmts.append(&mut self.pending);
                    stmts.push(s);
                }
                Item::Expr(e) => {
                    // 줄바꿈 뒤에 `}`가 오면 tail. `;`로 끝난 표현식은 값을 버리는 문장이다.
                    let save = self.pos;
                    self.skip_newlines();
                    if self.at(&Tok::RBrace) {
                        tail = Some(Box::new(e));
                        break;
                    }
                    self.pos = save;
                    self.end_of_stmt()?;
                    stmts.push(Stmt::Expr(e));
                }
            }
        }
        let close = self.expect(Tok::RBrace, "`}`")?.span;
        Ok(Block {
            stmts,
            tail,
            span: open.to(close),
        })
    }

    fn end_of_stmt(&mut self) -> Result<(), Diag> {
        if matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace) {
            Ok(())
        } else {
            Err(self.unexpected("end of line or `;`"))
        }
    }

    fn item(&mut self) -> Result<Item, Diag> {
        match self.peek().clone() {
            Tok::Let | Tok::Mut => self.let_stmt().map(Item::Stmt),
            Tok::Return => {
                let kw = self.bump();
                let value = if matches!(
                    self.peek(),
                    Tok::Newline | Tok::Semi | Tok::RBrace | Tok::Eof
                ) {
                    None
                } else {
                    Some(self.expr()?)
                };
                let span = match &value {
                    Some(v) => kw.span.to(v.span),
                    None => kw.span,
                };
                Ok(Item::Stmt(Stmt::Return { value, span }))
            }
            Tok::While => {
                let kw = self.bump();
                let cond = self.cond_expr()?;
                let body = self.block()?;
                let span = kw.span.to(body.span);
                Ok(Item::Stmt(Stmt::While { cond, body, step: Vec::new(), span }))
            }
            Tok::Break => {
                let kw = self.bump();
                Ok(Item::Stmt(Stmt::Break { span: kw.span }))
            }
            Tok::Continue => {
                let kw = self.bump();
                Ok(Item::Stmt(Stmt::Continue { span: kw.span }))
            }
            Tok::For => self.for_stmt().map(Item::Stmt),
            _ => {
                let e = self.expr()?;
                let compound = if self.at_shr_eq() {
                    Some(BinOp::Shr)
                } else {
                    match self.peek() {
                        Tok::PlusEq => Some(BinOp::Add),
                        Tok::MinusEq => Some(BinOp::Sub),
                        Tok::StarEq => Some(BinOp::Mul),
                        Tok::SlashEq => Some(BinOp::Div),
                        Tok::PercentEq => Some(BinOp::Rem),
                        Tok::AmpEq => Some(BinOp::BitAnd),
                        Tok::PipeEq => Some(BinOp::BitOr),
                        Tok::CaretEq => Some(BinOp::BitXor),
                        Tok::ShlEq => Some(BinOp::Shl),
                        _ => None,
                    }
                };
                if let Some(op) = compound {
                    if !matches!(
                        e.kind,
                        ExprKind::Var(_) | ExprKind::Field(..) | ExprKind::Deref(_) | ExprKind::Index(..)
                    ) {
                        return Err(Diag::new(
                            "invalid assignment target",
                            e.span,
                            "only variables, fields, elements and `*ref` can be assigned",
                        ));
                    }
                    if !pure_place(&e) {
                        return Err(Diag::new(
                            "compound assignment target has side effects",
                            e.span,
                            "the target is evaluated twice; write `t = t op v` after binding it to a variable",
                        ));
                    }
                    if op == BinOp::Shr {
                        self.bump();
                        self.bump();
                    } else {
                        self.bump();
                    }
                    let rhs = self.expr()?;
                    let span = e.span.to(rhs.span);
                    let value = Expr {
                        ty: Ty::Unit,
                        kind: ExprKind::Binary(op, Box::new(e.clone()), Box::new(rhs)),
                        span,
                    };
                    return Ok(Item::Stmt(Stmt::Assign { target: e, value, span }));
                }
                if !self.at(&Tok::Assign) {
                    return Ok(Item::Expr(e));
                }
                if !matches!(
                    e.kind,
                    ExprKind::Var(_) | ExprKind::Field(..) | ExprKind::Deref(_) | ExprKind::Index(..)
                ) {
                    return Err(Diag::new(
                        "invalid assignment target",
                        e.span,
                        "only variables, fields, elements and `*ref` can be assigned",
                    ));
                }
                self.bump(); // `=`
                let value = self.expr()?;
                let span = e.span.to(value.span);
                Ok(Item::Stmt(Stmt::Assign {
                    target: e,
                    value,
                    span,
                }))
            }
        }
    }

    /// `>>=`: 맞붙은 `>` `>=`.
    fn at_shr_eq(&self) -> bool {
        match (self.toks.get(self.pos), self.toks.get(self.pos + 1)) {
            (Some(a), Some(b)) => {
                a.tok == Tok::Gt && b.tok == Tok::Ge && a.span.line == b.span.line && a.span.col + 1 == b.span.col
            }
            _ => false,
        }
    }

    fn mk(&self, kind: ExprKind, span: Span) -> Expr {
        Expr { kind, span, ty: Ty::Unit }
    }

    fn let_hidden(&self, name: &str, mutable: bool, init: Expr) -> Stmt {
        let span = init.span;
        Stmt::Let { name: name.to_string(), mutable, ty: None, var_ty: Ty::Unit, init, span }
    }

    /// `for i in a..b { }`, `for i in a..=b { }`, `for x in slice { }`을 `while`로 풀어 쓴다.
    fn for_stmt(&mut self) -> Result<Stmt, Diag> {
        let kw = self.bump();
        let (var, vspan) = self.ident("loop variable")?;
        self.expect(Tok::In, "`in`")?;
        let first = self.cond_expr()?;
        self.nfor += 1;
        let n = self.nfor;
        let (hi_name, cnt) = (format!("$end{n}"), format!("$i{n}"));
        let var_expr = |s: &Parser, name: &str, span: Span| s.mk(ExprKind::Var(name.to_string()), span);
        let range = matches!(self.peek(), Tok::DotDot | Tok::DotDotEq);
        let (cond, mut step, bind_init, body_span_hint);
        if range {
            let inclusive = self.bump().tok == Tok::DotDotEq;
            let hi = self.cond_expr()?;
            let sp = first.span.to(hi.span);
            let is_lit = |e: &Expr| match &e.kind {
                ExprKind::Int(_) => true,
                ExprKind::Unary(UnOp::Neg, i) => matches!(i.kind, ExprKind::Int(_)),
                _ => false,
            };
            // 리터럴의 타입이 반대편 끝점을 따라가도록 `x - x + lit` 꼴로 만든다.
            let sub_self = |s: &Parser, name: &str, plus: Expr, span: Span| {
                let v1 = var_expr(s, name, span);
                let v2 = var_expr(s, name, span);
                let z = s.mk(ExprKind::Binary(BinOp::Sub, Box::new(v1), Box::new(v2)), span);
                s.mk(ExprKind::Binary(BinOp::Add, Box::new(z), Box::new(plus)), span)
            };
            if is_lit(&first) {
                let st = self.let_hidden(&hi_name, false, hi);
                self.pending.push(st);
                let init = sub_self(self, &hi_name, first, sp);
                let st = self.let_hidden(&cnt, true, init);
                self.pending.push(st);
            } else {
                let st = self.let_hidden(&cnt, true, first);
                self.pending.push(st);
                let init = sub_self(self, &cnt, hi, sp);
                let st = self.let_hidden(&hi_name, false, init);
                self.pending.push(st);
            }
            let op = if inclusive { BinOp::Le } else { BinOp::Lt };
            cond = self.mk(
                ExprKind::Binary(op, Box::new(var_expr(self, &cnt, sp)), Box::new(var_expr(self, &hi_name, sp))),
                sp,
            );
            let one = self.mk(ExprKind::Int(1), sp);
            let inc = Stmt::Assign {
                target: var_expr(self, &cnt, sp),
                value: self.mk(ExprKind::Binary(BinOp::Add, Box::new(var_expr(self, &cnt, sp)), Box::new(one)), sp),
                span: sp,
            };
            step = Vec::new();
            if inclusive {
                // 끝점이 정수 최댓값이어도 넘치지 않도록 마지막 값에서 먼저 나간다.
                let eq = self.mk(
                    ExprKind::Binary(BinOp::Eq, Box::new(var_expr(self, &cnt, sp)), Box::new(var_expr(self, &hi_name, sp))),
                    sp,
                );
                let brk = Block { stmts: vec![Stmt::Break { span: sp }], tail: None, span: sp };
                let iff = self.mk(ExprKind::If { cond: Box::new(eq), then: brk, els: None }, sp);
                step.push(Stmt::Expr(iff));
            }
            step.push(inc);
            bind_init = var_expr(self, &cnt, sp);
            body_span_hint = sp;
        } else {
            // 슬라이스/배열 참조 순회: 원소를 값으로 복사해 바인딩한다.
            let sp = first.span;
            let slice_name = format!("$s{n}");
            let st = self.let_hidden(&slice_name, false, first);
            self.pending.push(st);
            let zero = self.mk(ExprKind::Int(0), sp);
            let len = self.mk(ExprKind::Field(Box::new(var_expr(self, &slice_name, sp)), "len".to_string()), sp);
            let st = self.let_hidden(&hi_name, false, len);
            self.pending.push(st);
            let init = {
                let v1 = var_expr(self, &hi_name, sp);
                let v2 = var_expr(self, &hi_name, sp);
                let z = self.mk(ExprKind::Binary(BinOp::Sub, Box::new(v1), Box::new(v2)), sp);
                self.mk(ExprKind::Binary(BinOp::Add, Box::new(z), Box::new(zero)), sp)
            };
            let st = self.let_hidden(&cnt, true, init);
            self.pending.push(st);
            cond = self.mk(
                ExprKind::Binary(BinOp::Lt, Box::new(var_expr(self, &cnt, sp)), Box::new(var_expr(self, &hi_name, sp))),
                sp,
            );
            let one = self.mk(ExprKind::Int(1), sp);
            step = vec![Stmt::Assign {
                target: var_expr(self, &cnt, sp),
                value: self.mk(ExprKind::Binary(BinOp::Add, Box::new(var_expr(self, &cnt, sp)), Box::new(one)), sp),
                span: sp,
            }];
            bind_init = self.mk(
                ExprKind::Index(Box::new(var_expr(self, &slice_name, sp)), Box::new(var_expr(self, &cnt, sp))),
                sp,
            );
            body_span_hint = sp;
        }
        let _ = body_span_hint;
        let saved_pending = std::mem::take(&mut self.pending);
        let mut body = self.block()?;
        self.pending = saved_pending;
        let bind = Stmt::Let {
            name: var,
            mutable: false,
            ty: None,
            var_ty: Ty::Unit,
            init: bind_init,
            span: vspan,
        };
        body.stmts.insert(0, bind);
        let span = kw.span.to(body.span);
        // 구분 때문에 mut 대신 변수 이름은 사용자에게 보이지 않는다.
        let _ = &mut step;
        Ok(Stmt::While { cond, body, step, span })
    }

    fn let_stmt(&mut self) -> Result<Stmt, Diag> {
        let kw = self.bump();
        let mutable = kw.tok == Tok::Mut;
        let (name, _) = self.ident("variable name")?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.type_ref()?)
        } else {
            None
        };
        self.expect(Tok::Assign, "`=` and an initial value")?;
        let init = self.expr()?;
        let span = kw.span.to(init.span);
        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            var_ty: Ty::Unit,
            init,
            span,
        })
    }

    // ---- 표현식 ----

    /// `if`/`while` 조건: 구조체 리터럴을 허용하지 않는다.
    fn cond_expr(&mut self) -> Result<Expr, Diag> {
        let saved = std::mem::replace(&mut self.no_struct, true);
        let r = self.expr();
        self.no_struct = saved;
        r
    }

    /// 괄호/인자/필드 값 안에서는 다시 구조체 리터럴을 허용한다.
    fn expr_nested(&mut self) -> Result<Expr, Diag> {
        let saved = std::mem::replace(&mut self.no_struct, false);
        let r = self.expr();
        self.no_struct = saved;
        r
    }

    fn expr(&mut self) -> Result<Expr, Diag> {
        self.binary(0)
    }

    /// 맞붙은 `>` `>`는 오른쪽 시프트다. (렉서가 `>>`를 한 토큰으로 만들지 않는 것은
    /// `Result<Result<i32, i32>, i32>`처럼 제네릭 타입을 닫는 `>>`를 읽어야 하기 때문이다.)
    fn at_shr(&self) -> bool {
        match (self.toks.get(self.pos), self.toks.get(self.pos + 1)) {
            (Some(a), Some(b)) => {
                a.tok == Tok::Gt && b.tok == Tok::Gt && a.span.line == b.span.line && a.span.col + 1 == b.span.col
            }
            _ => false,
        }
    }

    fn binary(&mut self, min_prec: u8) -> Result<Expr, Diag> {
        let mut lhs = self.unary()?;
        let mut chain = 0u32;
        loop {
            if self.at_shr_eq() {
                break;
            }
            let shr = self.at_shr();
            let Some((op, prec)) = (if shr { Some((BinOp::Shr, 8)) } else { binop_info(self.peek()) }) else {
                break;
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            if shr {
                self.bump();
            }
            chain += 1;
            if chain > MAX_CHAIN {
                return Err(Diag::new(
                    format!("expression has more than {MAX_CHAIN} operators in a row"),
                    lhs.span,
                    "split it into several statements",
                ));
            }
            let rhs = self.binary(prec + 1)?;
            let span = lhs.span.to(rhs.span);
            lhs = Expr {
                ty: Ty::Unit,
                kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                span,
            };
        }
        Ok(lhs)
    }

    /// 단항 연산자 다음에 `as` 형 변환이 붙을 수 있다. (`-x as i64` == `(-x) as i64`)
    fn unary(&mut self) -> Result<Expr, Diag> {
        let mut e = self.unary_base()?;
        while self.eat(&Tok::As) {
            let ty = self.type_ref()?;
            let span = e.span.to(ty.span);
            e = Expr {
                ty: Ty::Unit,
                kind: ExprKind::Cast(Box::new(e), ty),
                span,
            };
        }
        Ok(e)
    }

    fn unary_base(&mut self) -> Result<Expr, Diag> {
        self.enter()?;
        let r = self.unary_base_inner();
        self.depth -= 1;
        r
    }

    fn unary_base_inner(&mut self) -> Result<Expr, Diag> {
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Bang => UnOp::Not,
            Tok::Tilde => UnOp::BitNot,
            Tok::Star => {
                let tok = self.bump();
                let operand = self.unary_base()?;
                let span = tok.span.to(operand.span);
                return Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Deref(Box::new(operand)),
                    span,
                });
            }
            Tok::AndAnd => {
                // `&&x`는 `& &x`다.
                let tok = self.bump();
                let mutable = self.eat(&Tok::Mut);
                let operand = self.unary_base()?;
                let span = tok.span.to(operand.span);
                let inner = Expr { ty: Ty::Unit, kind: ExprKind::AddrOf(mutable, Box::new(operand)), span };
                return Ok(Expr { ty: Ty::Unit, kind: ExprKind::AddrOf(false, Box::new(inner)), span });
            }
            Tok::Amp => {
                let tok = self.bump();
                let mutable = self.eat(&Tok::Mut);
                let operand = self.unary_base()?;
                let span = tok.span.to(operand.span);
                return Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::AddrOf(mutable, Box::new(operand)),
                    span,
                });
            }
            _ => return self.postfix(),
        };
        let tok = self.bump();
        let operand = self.unary_base()?;
        let span = tok.span.to(operand.span);
        Ok(Expr {
            ty: Ty::Unit,
            kind: ExprKind::Unary(op, Box::new(operand)),
            span,
        })
    }

    /// `primary` 뒤에 `.field`가 계속 붙을 수 있다.
    fn postfix(&mut self) -> Result<Expr, Diag> {
        let mut e = self.primary()?;
        while self.at(&Tok::Dot) || self.at(&Tok::Question) || self.at(&Tok::LBracket) {
            if self.at(&Tok::LBracket) {
                self.bump();
                let saved = std::mem::replace(&mut self.no_struct, false);
                let lo = if self.at(&Tok::DotDot) { None } else { Some(Box::new(self.expr()?)) };
                let kind = if self.eat(&Tok::DotDot) {
                    let hi = if self.at(&Tok::RBracket) { None } else { Some(Box::new(self.expr()?)) };
                    ExprKind::Slice(Box::new(e.clone()), lo, hi)
                } else {
                    ExprKind::Index(Box::new(e.clone()), lo.unwrap())
                };
                self.no_struct = saved;
                let close = self.expect(Tok::RBracket, "`]`")?;
                let span = e.span.to(close.span);
                e = Expr { ty: Ty::Unit, kind, span };
                continue;
            }
            if self.at(&Tok::Question) {
                let q = self.bump();
                let span = e.span.to(q.span);
                e = Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Try(Box::new(e)),
                    span,
                };
                continue;
            }
            self.bump();
            let (name, nspan) = self.ident("a field name")?;
            if self.at(&Tok::LParen) {
                self.bump();
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) {
                    args.push(self.expr_nested()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let close = self.expect(Tok::RParen, "`)`")?;
                let span = e.span.to(close.span);
                e = Expr { ty: Ty::Unit, kind: ExprKind::MethodCall(Box::new(e), name, args), span };
                continue;
            }
            let span = e.span.to(nspan);
            e = Expr {
                ty: Ty::Unit,
                kind: ExprKind::Field(Box::new(e), name),
                span,
            };
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, Diag> {
        let t = self.toks[self.pos].clone();
        match t.tok {
            Tok::Int(n) => {
                self.bump();
                Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Int(n),
                    span: t.span,
                })
            }
            Tok::Float(x) => {
                self.bump();
                Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Float(x),
                    span: t.span,
                })
            }
            Tok::True | Tok::False => {
                self.bump();
                Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Bool(t.tok == Tok::True),
                    span: t.span,
                })
            }
            Tok::LBracket => {
                self.bump();
                let saved = std::mem::replace(&mut self.no_struct, false);
                let first = self.expr()?;
                let kind = if self.eat(&Tok::Semi) {
                    let nt = self.toks[self.pos].clone();
                    let Tok::Int(n) = nt.tok else {
                        return Err(self.unexpected("an array length (integer literal)"));
                    };
                    self.bump();
                    if n < 1 {
                        return Err(Diag::new("array length must be at least 1", nt.span, "zero-length arrays are not supported"));
                    }
                    ExprKind::Repeat(Box::new(first), n as u64)
                } else {
                    let mut elems = vec![first];
                    while self.eat(&Tok::Comma) {
                        self.skip_newlines();
                        if self.at(&Tok::RBracket) {
                            break;
                        }
                        elems.push(self.expr()?);
                    }
                    ExprKind::Array(elems)
                };
                self.no_struct = saved;
                let close = self.expect(Tok::RBracket, "`]`")?;
                Ok(Expr { ty: Ty::Unit, kind, span: t.span.to(close.span) })
            }
            Tok::Ident(_) => {
                let (name, nspan) = self.path_name("a name")?;
                let t = Token {
                    tok: Tok::Ident(name.clone()),
                    span: nspan,
                };
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    while !self.at(&Tok::RParen) {
                        args.push(self.expr_nested()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    let close = self.expect(Tok::RParen, "`)`")?;
                    Ok(Expr {
                        ty: Ty::Unit,
                        kind: ExprKind::Call(name, args),
                        span: t.span.to(close.span),
                    })
                } else if self.at(&Tok::LBrace) && !self.no_struct {
                    self.struct_lit(name, t.span)
                } else {
                    Ok(Expr {
                        ty: Ty::Unit,
                        kind: ExprKind::Var(name),
                        span: t.span,
                    })
                }
            }
            Tok::LParen => {
                self.bump();
                let inner = self.expr_nested()?;
                let close = self.expect(Tok::RParen, "`)`")?;
                Ok(Expr {
                    ty: Ty::Unit,
                    kind: inner.kind,
                    span: t.span.to(close.span),
                })
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Str(s),
                    span: t.span,
                })
            }
            Tok::Match => self.match_expr(),
            Tok::If => self.if_expr(),
            Tok::Unsafe => {
                let kw = self.bump();
                let saved = std::mem::replace(&mut self.no_struct, false);
                let block = self.block();
                self.no_struct = saved;
                let block = block?;
                let span = kw.span.to(block.span);
                Ok(Expr {
                    ty: Ty::Unit,
                    kind: ExprKind::Unsafe(block),
                    span,
                })
            }
            _ => Err(self.unexpected("an expression")),
        }
    }

    fn struct_lit(&mut self, name: String, start: Span) -> Result<Expr, Diag> {
        let open = self.expect(Tok::LBrace, "`{`")?.span;
        let mut fields = Vec::new();
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(Diag::new("unclosed struct literal", open, "opened here"));
            }
            let (fname, fspan) = self.ident("a field name")?;
            self.expect(Tok::Colon, "`:` and a value")?;
            let value = self.expr_nested()?;
            fields.push(FieldInit {
                name: fname,
                value,
                span: fspan,
            });
            if !self.eat(&Tok::Comma) {
                self.end_of_stmt()?;
            }
        }
        let close = self.expect(Tok::RBrace, "`}`")?;
        Ok(Expr {
            ty: Ty::Unit,
            kind: ExprKind::StructLit(name, fields),
            span: start.to(close.span),
        })
    }

    /// `match scrut { Ok(a) => body  Err(b) => body }` (순서 무관, 각각 정확히 한 번)
    fn match_expr(&mut self) -> Result<Expr, Diag> {
        let kw = self.bump();
        let scrut = self.cond_expr()?;
        let open = self.expect(Tok::LBrace, "`{`")?.span;
        let mut ok: Option<Arm> = None;
        let mut err: Option<Arm> = None;
        let mut saw_option: Option<bool> = None;
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(Diag::new("unclosed `match`", open, "opened here"));
            }
            let (variant, vspan) = self.ident("`Ok`, `Err`, `Some` or `None`")?;
            if !matches!(variant.as_str(), "Ok" | "Err" | "Some" | "None") {
                return Err(Diag::new(
                    format!("unknown pattern `{variant}`"),
                    vspan,
                    "`match` supports `Ok(name)`, `Err(name)`, `Some(name)` and `None` patterns",
                ));
            }
            let option = variant == "Some" || variant == "None";
            match saw_option {
                Some(o) if o != option => {
                    return Err(Diag::new(
                        format!("`{variant}` cannot be mixed with `{}` patterns in one `match`", if o { "Some`/`None" } else { "Ok`/`Err" }),
                        vspan,
                        "a `match` handles either a `Result` or an `Option`",
                    ))
                }
                _ => saw_option = Some(option),
            }
            let bind = if variant == "None" {
                "_".to_string()
            } else {
                self.expect(Tok::LParen, "`(`")?;
                let (b, _) = self.ident("a binding name")?;
                self.expect(Tok::RParen, "`)`")?;
                b
            };
            self.expect(Tok::FatArrow, "`=>`")?;
            let body = if self.at(&Tok::LBrace) {
                self.block()?
            } else {
                let e = self.expr_nested()?;
                let span = e.span;
                Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(e)),
                    span,
                }
            };
            let arm = Arm {
                bind,
                bind_ty: Ty::Unit,
                body,
                span: vspan,
                option,
            };
            let slot = if variant == "Ok" || variant == "Some" { &mut ok } else { &mut err };
            if slot.is_some() {
                return Err(Diag::new(
                    format!("duplicate `{variant}` arm"),
                    vspan,
                    "already handled above",
                ));
            }
            *slot = Some(arm);
            if !self.eat(&Tok::Comma) {
                self.end_of_stmt()?;
            }
        }
        let close = self.expect(Tok::RBrace, "`}`")?.span;
        let span = kw.span.to(close);
        match (ok, err) {
            (Some(ok), Some(err)) => Ok(Expr {
                ty: Ty::Unit,
                kind: ExprKind::Match {
                    scrut: Box::new(scrut),
                    ok,
                    err,
                },
                span,
            }),
            (ok, _) => Err(Diag::new(
                format!(
                    "`match` is missing the `{}` arm",
                    match (ok.is_some(), saw_option == Some(true)) {
                        (true, false) => "Err",
                        (false, false) => "Ok",
                        (true, true) => "None",
                        (false, true) => "Some",
                    }
                ),
                span,
                "all variants must be handled: `Ok`/`Err` or `Some`/`None`",
            )),
        }
    }

    fn if_expr(&mut self) -> Result<Expr, Diag> {
        let kw = self.bump();
        let cond = self.cond_expr()?;
        let then = self.block()?;
        // `else`는 다음 줄에 와도 된다. 없으면 위치를 되돌려 문장 종료를 보존한다.
        let save = self.pos;
        self.skip_newlines();
        let els = if self.eat(&Tok::Else) {
            if self.at(&Tok::If) {
                let nested = self.if_expr()?;
                let span = nested.span;
                Some(Block {
                    stmts: Vec::new(),
                    tail: Some(Box::new(nested)),
                    span,
                })
            } else {
                Some(self.block()?)
            }
        } else {
            self.pos = save;
            None
        };
        let end = els.as_ref().map(|b| b.span).unwrap_or(then.span);
        Ok(Expr {
            ty: Ty::Unit,
            kind: ExprKind::If {
                cond: Box::new(cond),
                then,
                els,
            },
            span: kw.span.to(end),
        })
    }
}
