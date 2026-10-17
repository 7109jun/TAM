//! TAM AST. M1~M3 범위: 함수, 변수, 산술/비교/논리, if/while, 호출.

use crate::diag::Span;
use crate::types::Ty;

#[derive(Debug, Clone)]
pub struct Program {
    pub uses: Vec<UseDecl>,
    pub structs: Vec<StructDef>,
    pub funcs: Vec<Func>,
    /// 전역 상수. 이름 해석(`modules`) 단계에서 값으로 치환되어 사라진다.
    pub consts: Vec<ConstDef>,
}

/// `const NAME: T = 식` — 컴파일 시간에 계산되는 전역 상수.
#[derive(Debug, Clone)]
pub struct ConstDef {
    pub public: bool,
    pub name: String,
    pub ty: TypeRef,
    pub value: Expr,
    pub span: Span,
}

/// `use a.b` — 파일 `<root>/a/b.tam`을 불러오고 `b::item`으로 참조한다.
#[derive(Debug, Clone)]
pub struct UseDecl {
    pub path: Vec<String>,
    pub alias: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub public: bool,
    pub name: String,
    /// `struct Pair<A, B>`의 `A`, `B`. 비어 있으면 일반 구조체.
    pub tparams: Vec<String>,
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Func {
    pub public: bool,
    pub unsafe_: bool,
    /// `extern "lib" fn ...`: 본문 없이 외부 라이브러리의 함수를 선언한다. (`lib`: `"c"` 또는 DLL 이름)
    pub ext: Option<String>,
    /// extern 함수의 실제 심볼 이름. 없으면 선언한 이름 그대로.
    pub link: Option<String>,
    pub name: String,
    /// 타입 매개변수. `impl Pair<A, B>` 안의 함수는 `A`, `B`가 앞에 온다.
    pub tparams: Vec<String>,
    /// `tparams` 중 앞의 몇 개가 `impl` 헤더에서 온 것인가.
    pub impl_tparams: usize,
    pub params: Vec<Param>,
    pub ret: Option<TypeRef>,
    /// `impl T { }` 안의 함수면 `T`. 이름은 `T.name`으로 바뀌어 있다.
    pub impl_ty: Option<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeRef {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Named(String),
    /// `&T` 또는 `&mut T`
    Ref(bool, Box<TypeRef>),
    /// `*const T`(false) 또는 `*mut T`(true)
    Ptr(bool, Box<TypeRef>),
    /// `[T; NAME]` — 길이가 상수 이름인 배열. 이름 해석 단계에서 `Array`로 바뀐다.
    ArrayOf(Box<TypeRef>, String),
    /// `Name<A, B>` (내장 `Result<T, E>`와 `Option<T>`, 사용자 제네릭 구조체)
    Generic(String, Vec<TypeRef>),
    /// `[T; N]`
    Array(Box<TypeRef>, u64),
    /// `[T]` (참조 뒤에서만 쓸 수 있다)
    Slice(Box<TypeRef>),
}

/// `{ stmts... tail }`. `tail`은 블록의 값이 되는 마지막 표현식이다.
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let x = e` 또는 `mut x = e` (타입 표기는 선택)
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeRef>,
        /// 타입 검사가 채우는 변수의 최종 타입
        var_ty: Ty,
        init: Expr,
        span: Span,
    },
    /// `target = value`. `target`은 place 표현식(`x`, `p.x`, `*r`)이다.
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        /// `for`에서 온 증가 문장들. `continue`가 여기로 뛴다.
        step: Vec<Stmt>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// 타입 검사가 채운다. 검사 전에는 `Ty::Unit`.
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i128),
    /// 타입이 정해진 정수 상수 값(`const N: u64`). 타입 검사가 `Int`로 바꾼다.
    TypedInt(i128, Ty),
    Float(f64),
    Bool(bool),
    /// 문자열 리터럴 (`str`)
    Str(String),
    Cast(Box<Expr>, TypeRef),
    /// `[a, b, c]`
    Array(Vec<Expr>),
    /// `[v; N]`
    Repeat(Box<Expr>, u64),
    /// `[v; NAME]` — 길이가 상수 이름. 이름 해석 단계에서 `Repeat`로 바뀐다.
    RepeatOf(Box<Expr>, String),
    /// `name<T, U>(..)`, `Type<T>::f(..)`, `Type<T> { .. }`의 명시적 타입 인자.
    /// 타입 검사가 인스턴스를 만들고 안쪽 식으로 바꾼다.
    Generic(Vec<TypeRef>, Box<Expr>),
    /// `base[index]`
    Index(Box<Expr>, Box<Expr>),
    /// `base[lo..hi]` (양쪽 모두 생략 가능). 반드시 `&`/`&mut`로 빌려서 쓴다.
    Slice(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),
    /// `base.name` (base가 참조이면 자동으로 역참조한다)
    Field(Box<Expr>, String),
    StructLit(String, Vec<FieldInit>),
    /// `&e` / `&mut e`
    AddrOf(bool, Box<Expr>),
    /// `*e`
    Deref(Box<Expr>),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    /// `recv.name(args)`. 타입 검사가 `Call("T.name", [self, args..])`로 바꾼다.
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// `unsafe { ... }`
    Unsafe(Block),
    /// `e?` — 오류면 바로 반환하고, 아니면 성공 값이 된다.
    Try(Box<Expr>),
    /// `Ok(e)` / `Err(e)` (true = Ok). 타입 검사가 `Call`을 이 노드로 바꾼다.
    Ctor(bool, Box<Expr>),
    /// `None`의 페이로드(크기 0). 타입 검사가 `None`을 `Ctor(false, NoneTag)`로 바꾼다.
    NoneTag,
    /// `match r { Ok(a) => .. Err(b) => .. }`
    Match {
        scrut: Box<Expr>,
        ok: Arm,
        err: Arm,
    },
    If {
        cond: Box<Expr>,
        then: Block,
        els: Option<Block>,
    },
}

#[derive(Debug, Clone)]
pub struct Arm {
    pub bind: String,
    /// 타입 검사가 채우는 바인딩의 타입
    pub bind_ty: Ty,
    pub body: Block,
    pub span: Span,
    /// `Some`/`None` 패턴이면 true (`Ok`/`Err`이면 false)
    pub option: bool,
}

#[derive(Debug, Clone)]
pub struct FieldInit {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

impl Expr {
    /// 주소를 가질 수 있는 표현식인가. 타입 검사 이후에 호출해야 한다(`ty`를 쓴다).
    pub fn is_place(&self) -> bool {
        match &self.kind {
            ExprKind::Var(_) | ExprKind::Deref(_) => true,
            ExprKind::Field(base, _) => {
                let inner = match base.ty {
                    Ty::Ref(t, _) => *t,
                    t => t,
                };
                // 배열/슬라이스의 `.len`, `.ptr`은 값이다.
                !matches!(inner, Ty::Array(..) | Ty::Slice(_)) && (base.is_place() || matches!(base.ty, Ty::Ref(..)))
            }
            ExprKind::Index(base, _) | ExprKind::Slice(base, _, _) => base.is_place() || matches!(base.ty, Ty::Ref(..)),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    /// 비트 반전 `~x`
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    /// `wrapping_add/sub/mul(a, b)`: 오버플로를 trap하지 않고 감싼다.
    WrapAdd,
    WrapSub,
    WrapMul,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}
