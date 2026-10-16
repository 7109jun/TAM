//! TAM의 기본 타입.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F64,
    Bool,
    Unit,
    Never,
    /// 구조체 값. 이름으로 구분한다.
    Struct(&'static str),
    /// 참조 `&T` / `&mut T`. 컴파일러는 단명 프로세스라 `Ty`를 `Copy`로 유지하려고 타입을 leak한다.
    Ref(&'static Ty, bool),
    /// raw 포인터 `*const T`(false) / `*mut T`(true). 빌림 검사 대상이 아니다.
    Ptr(&'static Ty, bool),
    /// 문자열 조각 `str`: (포인터, 길이) 쌍. 문자열 리터럴의 타입이며 Copy다. `.ptr`, `.len`으로 읽는다.
    Str,
    /// 힙에 사는 소유 문자열 `String`: (포인터, 길이, 용량). 이동 타입이며 스코프가 끝나면 해제된다.
    String,
    /// 내장 `Result<T, E>`. `(성공 타입, 오류 타입)`
    Result(&'static Ty, &'static Ty),
    /// 고정 길이 배열 `[T; N]` (N >= 1). 이동 타입이다.
    Array(&'static Ty, u64),
    /// 슬라이스의 "가리키는 쪽" 타입 `[T]`. 항상 참조 뒤(`&[T]`, `&mut [T]`)에만 나타나며,
    /// 그 참조는 (포인터, 길이) 쌍이다.
    Slice(&'static Ty),
    /// `Option<T>`의 "없음" 쪽 페이로드. `Option<T>`는 `Result<T, NoneTag>`로 표현된다.
    NoneTag,
}

impl Ty {
    pub fn reference(inner: Ty, mutable: bool) -> Ty {
        Ty::Ref(Box::leak(Box::new(inner)), mutable)
    }

    pub fn ptr(inner: Ty, mutable: bool) -> Ty {
        Ty::Ptr(Box::leak(Box::new(inner)), mutable)
    }

    pub fn result(ok: Ty, err: Ty) -> Ty {
        Ty::Result(Box::leak(Box::new(ok)), Box::leak(Box::new(err)))
    }

    pub fn option(t: Ty) -> Ty {
        Ty::result(t, Ty::NoneTag)
    }

    pub fn array(elem: Ty, n: u64) -> Ty {
        Ty::Array(Box::leak(Box::new(elem)), n)
    }

    pub fn slice(elem: Ty) -> Ty {
        Ty::Slice(Box::leak(Box::new(elem)))
    }

    pub fn strukt(name: &str) -> Ty {
        Ty::Struct(Box::leak(name.to_string().into_boxed_str()))
    }

    pub fn from_name(s: &str) -> Option<Ty> {
        Some(match s {
            "i8" => Ty::I8,
            "i16" => Ty::I16,
            "i32" => Ty::I32,
            "i64" => Ty::I64,
            "u8" => Ty::U8,
            "u16" => Ty::U16,
            "u32" => Ty::U32,
            "u64" => Ty::U64,
            "f64" => Ty::F64,
            "bool" => Ty::Bool,
            "str" => Ty::Str,
            "String" => Ty::String,
            _ => return None,
        })
    }

    pub fn name(self) -> String {
        match self {
            Ty::I8 => "i8".to_string(),
            Ty::I16 => "i16".to_string(),
            Ty::I32 => "i32".to_string(),
            Ty::I64 => "i64".to_string(),
            Ty::U8 => "u8".to_string(),
            Ty::U16 => "u16".to_string(),
            Ty::U32 => "u32".to_string(),
            Ty::U64 => "u64".to_string(),
            Ty::F64 => "f64".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Unit => "()".to_string(),
            Ty::Never => "!".to_string(),
            Ty::Struct(n) => n.to_string(),
            Ty::Ref(t, m) => format!("&{}{}", if m { "mut " } else { "" }, t.name()),
            Ty::Ptr(t, m) => format!("*{} {}", if m { "mut" } else { "const" }, t.name()),
            Ty::Str => "str".to_string(),
            Ty::String => "String".to_string(),
            Ty::Result(t, Ty::NoneTag) => format!("Option<{}>", t.name()),
            Ty::NoneTag => "None".to_string(),
            Ty::Result(t, e) => format!("Result<{}, {}>", t.name(), e.name()),
            Ty::Array(t, n) => format!("[{}; {n}]", t.name()),
            Ty::Slice(t) => format!("[{}]", t.name()),
        }
    }

    pub fn is_int(self) -> bool {
        matches!(
            self,
            Ty::I8 | Ty::I16 | Ty::I32 | Ty::I64 | Ty::U8 | Ty::U16 | Ty::U32 | Ty::U64
        )
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Ty::I8 | Ty::I16 | Ty::I32 | Ty::I64)
    }

    pub fn is_numeric(self) -> bool {
        self.is_int() || self == Ty::F64
    }

    pub fn bits(self) -> u32 {
        match self {
            Ty::I8 | Ty::U8 => 8,
            Ty::I16 | Ty::U16 => 16,
            Ty::I32 | Ty::U32 => 32,
            Ty::I64 | Ty::U64 | Ty::F64 => 64,
            Ty::Bool => 1,
            Ty::Ref(..) | Ty::Ptr(..) => 64,
            Ty::Unit | Ty::Never | Ty::Struct(_) | Ty::Str | Ty::String | Ty::Result(..) | Ty::Array(..) | Ty::Slice(_) | Ty::NoneTag => 0,
        }
    }

    pub fn llty(self) -> String {
        match self {
            Ty::I8 | Ty::U8 => "i8".to_string(),
            Ty::I16 | Ty::U16 => "i16".to_string(),
            Ty::I32 | Ty::U32 => "i32".to_string(),
            Ty::I64 | Ty::U64 => "i64".to_string(),
            Ty::F64 => "double".to_string(),
            Ty::Bool => "i1".to_string(),
            Ty::Unit | Ty::Never => "void".to_string(),
            Ty::Struct(n) => format!("%{}", ll_quote(n)),
            Ty::Ref(Ty::Slice(_), _) => "{ ptr, i64 }".to_string(),
            Ty::Ref(..) | Ty::Ptr(..) => "ptr".to_string(),
            Ty::Array(t, n) => format!("[{n} x {}]", t.llty()),
            Ty::NoneTag => "{}".to_string(),
            Ty::Slice(_) => unreachable!("an unsized slice type has no value representation"),
            Ty::Str => "{ ptr, i64 }".to_string(),
            Ty::String => "{ ptr, i64, i64 }".to_string(),
            Ty::Result(t, e) => format!("{{ i1, {}, {} }}", t.llty(), e.llty()),
        }
    }

    /// 정수 리터럴 `n`이 이 타입에 들어가는가. (리터럴은 i64 범위까지만 지원한다.)
    pub fn fits(self, n: i128) -> bool {
        match self {
            Ty::I8 => (-128..=127).contains(&n),
            Ty::I16 => (-32768..=32767).contains(&n),
            Ty::I32 => (i32::MIN as i128..=i32::MAX as i128).contains(&n),
            Ty::I64 => (i64::MIN as i128..=i64::MAX as i128).contains(&n),
            Ty::U8 => (0..=255).contains(&n),
            Ty::U16 => (0..=65535).contains(&n),
            Ty::U32 => (0..=u32::MAX as i128).contains(&n),
            Ty::U64 => (0..=u64::MAX as i128).contains(&n),
            _ => false,
        }
    }
}

/// LLVM 식별자 이름. `::`나 `.`처럼 특수 문자가 있으면 따옴표로 감싼다.
pub fn ll_quote(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        name.to_string()
    } else {
        format!("\"{name}\"")
    }
}
