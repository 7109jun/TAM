//! `Tmake.toml`: 패키지 메타데이터, 의존성, 타겟/빌드 설정, 워크스페이스.
//!
//! 문법은 TOML 그대로다(독자 문법 없음). 파싱은 `toml` 크레이트가 하고, 여기서는 의미 검사를 한다.
//! 모든 오류는 `Tmake.toml`의 줄/열을 가리키는 진단으로 나온다.
//!
//! ```toml
//! [package]
//! name = "app"
//! version = "0.1.0"
//! tamc = ">=0.16"            # 필요한 컴파일러 버전
//!
//! [dependencies]
//! util = { path = "../util", version = "^0.2" }
//!
//! [build]
//! target = "host"
//!
//! [profile.release]
//! opt-level = 2
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::diag::{Diag, Span};

pub const MANIFEST: &str = "Tmake.toml";
pub const LOCKFILE: &str = "Tmake.lock";
pub const TAMC_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TARGETS: [&str; 5] = ["host", "windows", "uefi", "bios", "aarch64"];

/// 진단과 함께 그 진단이 가리키는 파일들(표시 이름, 소스)을 돌려주는 오류.
#[derive(Debug)]
pub struct MError {
    pub diag: Diag,
    pub files: Vec<(String, String)>,
}

impl MError {
    pub fn render(&self) -> String {
        self.diag.render_multi(&self.files)
    }
}

type R<T> = Result<T, MError>;

// ------------------------------------------------------------------ 버전

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Version {
    /// `1.2.3` (뒤에 `-pre`/`+build`가 붙어도 무시한다).
    pub fn parse(s: &str) -> Option<Version> {
        let core = s.split(['-', '+']).next()?;
        let mut it = core.split('.');
        let (a, b, c) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() {
            return None;
        }
        let n = |x: &str| if x.is_empty() || (x.len() > 1 && x.starts_with('0')) { None } else { x.parse::<u64>().ok() };
        let z = |x: &str| if x == "0" { Some(0) } else { n(x) };
        Some(Version { major: z(a)?, minor: z(b)?, patch: z(c)? })
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
    Caret,
    Tilde,
}

#[derive(Clone, Debug)]
struct Cmp {
    op: Op,
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
}

/// 버전 요구: `^1.2`, `~1.2.3`, `>=1.0, <2`, `=1.2.3`, `1.2`(= `^1.2`), `*`.
#[derive(Clone, Debug)]
pub struct Req {
    cmps: Vec<Cmp>,
    text: String,
}

impl std::fmt::Display for Req {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl Req {
    pub fn parse(s: &str) -> Result<Req, String> {
        let text = s.trim().to_string();
        if text == "*" {
            return Ok(Req { cmps: Vec::new(), text });
        }
        let mut cmps = Vec::new();
        for part in text.split(',') {
            let p = part.trim();
            let (op, rest) = if let Some(r) = p.strip_prefix(">=") {
                (Op::Ge, r)
            } else if let Some(r) = p.strip_prefix("<=") {
                (Op::Le, r)
            } else if let Some(r) = p.strip_prefix('>') {
                (Op::Gt, r)
            } else if let Some(r) = p.strip_prefix('<') {
                (Op::Lt, r)
            } else if let Some(r) = p.strip_prefix('=') {
                (Op::Eq, r)
            } else if let Some(r) = p.strip_prefix('^') {
                (Op::Caret, r)
            } else if let Some(r) = p.strip_prefix('~') {
                (Op::Tilde, r)
            } else {
                (Op::Caret, p)
            };
            let rest = rest.trim();
            let mut nums = Vec::new();
            let mut wild = false;
            for seg in rest.split('.') {
                if seg == "*" || seg == "x" {
                    wild = true;
                    continue;
                }
                if wild {
                    return Err(format!("`{p}` is not a valid version requirement"));
                }
                match seg.parse::<u64>() {
                    Ok(n) if !(seg.len() > 1 && seg.starts_with('0')) => nums.push(n),
                    _ => return Err(format!("`{p}` is not a valid version requirement")),
                }
            }
            if nums.is_empty() || nums.len() > 3 {
                return Err(format!("`{p}` is not a valid version requirement"));
            }
            cmps.push(Cmp { op, major: nums[0], minor: nums.get(1).copied(), patch: nums.get(2).copied() });
        }
        Ok(Req { cmps, text })
    }

    pub fn matches(&self, v: &Version) -> bool {
        self.cmps.iter().all(|c| c.matches(v))
    }
}

impl Cmp {
    fn full(&self) -> Version {
        Version { major: self.major, minor: self.minor.unwrap_or(0), patch: self.patch.unwrap_or(0) }
    }

    fn matches(&self, v: &Version) -> bool {
        let lo = self.full();
        let ver = |a: u64, b: u64, c: u64| Version { major: a, minor: b, patch: c };
        match self.op {
            Op::Eq => {
                v.major == self.major
                    && self.minor.map_or(true, |m| v.minor == m)
                    && self.patch.map_or(true, |p| v.patch == p)
            }
            Op::Ge => *v >= lo,
            Op::Gt => match (self.minor, self.patch) {
                (_, Some(_)) => *v > lo,
                (Some(m), None) => *v >= ver(self.major, m + 1, 0),
                _ => *v >= ver(self.major + 1, 0, 0),
            },
            Op::Lt => *v < lo,
            Op::Le => match (self.minor, self.patch) {
                (_, Some(_)) => *v <= lo,
                (Some(m), None) => *v < ver(self.major, m + 1, 0),
                _ => *v < ver(self.major + 1, 0, 0),
            },
            Op::Caret => {
                let upper = if self.major > 0 {
                    ver(self.major + 1, 0, 0)
                } else if let Some(m) = self.minor {
                    if m > 0 {
                        ver(0, m + 1, 0)
                    } else if let Some(p) = self.patch {
                        ver(0, 0, p + 1)
                    } else {
                        ver(0, 1, 0)
                    }
                } else {
                    ver(1, 0, 0)
                };
                *v >= lo && *v < upper
            }
            Op::Tilde => {
                let upper = match self.minor {
                    Some(m) => ver(self.major, m + 1, 0),
                    None => ver(self.major + 1, 0, 0),
                };
                *v >= lo && *v < upper
            }
        }
    }
}

// ------------------------------------------------------------------ TOML 트리 (위치 포함)

#[derive(Debug)]
pub struct Src {
    pub path: String,
    pub text: String,
    line_starts: Vec<usize>,
}

impl Src {
    fn new(path: String, text: String) -> Src {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Src { path, text, line_starts }
    }

    fn span(&self, r: std::ops::Range<usize>) -> Span {
        let line = match self.line_starts.binary_search(&r.start) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let start = self.line_starts[line];
        let col = self.text[start..r.start.min(self.text.len())].chars().count() as u32 + 1;
        let end = r.end.min(self.text.len()).max(r.start);
        let len = if self.text[r.start.min(end)..end].contains('\n') {
            // 여러 줄 값은 첫 줄만 가리킨다.
            self.text[r.start..end].lines().next().map_or(1, |l| l.chars().count() as u32)
        } else {
            self.text[r.start.min(end)..end].chars().count() as u32
        };
        Span::new(line as u32 + 1, col, len.max(1))
    }
}

#[derive(Clone, Debug)]
pub enum Val {
    Str(String),
    Int(i64),
    Bool(bool),
    Arr(Vec<Node>),
    Tab(Table),
    Other(&'static str),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub v: Val,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct Table {
    pub entries: Vec<(String, Span, Node)>,
}

impl Val {
    fn kind(&self) -> &'static str {
        match self {
            Val::Str(_) => "a string",
            Val::Int(_) => "an integer",
            Val::Bool(_) => "a boolean",
            Val::Arr(_) => "an array",
            Val::Tab(_) => "a table",
            Val::Other(s) => s,
        }
    }
}

fn convert(src: &Src, v: &toml::de::DeValue<'_>, span: std::ops::Range<usize>) -> Node {
    use toml::de::DeValue as D;
    let val = match v {
        D::String(s) => Val::Str(s.to_string()),
        D::Integer(i) => match i64::from_str_radix(i.as_str().replace('_', "").trim_start_matches("0x").trim_start_matches("0o").trim_start_matches("0b"), i.radix()) {
            Ok(n) => Val::Int(n),
            Err(_) => Val::Other("an out-of-range integer"),
        },
        D::Float(_) => Val::Other("a float"),
        D::Boolean(b) => Val::Bool(*b),
        D::Datetime(_) => Val::Other("a date/time"),
        D::Array(a) => Val::Arr(a.iter().map(|n| convert(src, n.get_ref(), n.span())).collect()),
        D::Table(t) => Val::Tab(convert_table(src, t)),
    };
    Node { v: val, span: src.span(span) }
}

fn convert_table(src: &Src, t: &toml::de::DeTable<'_>) -> Table {
    let mut entries = Vec::new();
    for (k, v) in t.iter() {
        entries.push((k.get_ref().to_string(), src.span(k.span()), convert(src, v.get_ref(), v.span())));
    }
    Table { entries }
}

// ------------------------------------------------------------------ 오류 도우미

struct Ctx<'a> {
    src: &'a Src,
}

impl Ctx<'_> {
    fn err(&self, msg: impl Into<String>, span: Span, label: impl Into<String>) -> MError {
        MError { diag: Diag::new(msg, span, label), files: vec![(self.src.path.clone(), self.src.text.clone())] }
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

impl Table {
    fn get(&self, key: &str) -> Option<&(String, Span, Node)> {
        self.entries.iter().find(|e| e.0 == key)
    }

    /// 허용 목록에 없는 키는 오류(철자 제안 포함). 중복 키는 TOML 파서가 이미 거절한다.
    fn only(&self, cx: &Ctx, section: &str, allowed: &[&str]) -> R<()> {
        for (k, span, _) in &self.entries {
            if !allowed.contains(&k.as_str()) {
                let hint = allowed.iter().filter(|a| edit_distance(k, a) <= 2).min_by_key(|a| edit_distance(k, a));
                let label = match hint {
                    Some(h) => format!("did you mean `{h}`?"),
                    None => format!("allowed keys: {}", allowed.join(", ")),
                };
                return Err(cx.err(format!("unknown key `{k}` in {section}"), *span, label));
            }
        }
        Ok(())
    }

    fn string(&self, cx: &Ctx, key: &str) -> R<Option<(String, Span)>> {
        match self.get(key) {
            None => Ok(None),
            Some((_, _, n)) => match &n.v {
                Val::Str(s) => Ok(Some((s.clone(), n.span))),
                other => Err(cx.err(format!("`{key}` must be a string"), n.span, format!("found {}", other.kind()))),
            },
        }
    }

    fn table(&self, cx: &Ctx, key: &str) -> R<Option<(&Table, Span)>> {
        match self.get(key) {
            None => Ok(None),
            Some((_, _, n)) => match &n.v {
                Val::Tab(t) => Ok(Some((t, n.span))),
                other => Err(cx.err(format!("`{key}` must be a table"), n.span, format!("found {}", other.kind()))),
            },
        }
    }

    fn strings(&self, cx: &Ctx, key: &str) -> R<Option<Vec<(String, Span)>>> {
        match self.get(key) {
            None => Ok(None),
            Some((_, _, n)) => match &n.v {
                Val::Arr(items) => {
                    let mut out = Vec::new();
                    for it in items {
                        match &it.v {
                            Val::Str(s) => out.push((s.clone(), it.span)),
                            other => {
                                return Err(cx.err(format!("`{key}` must be an array of strings"), it.span, format!("found {}", other.kind())))
                            }
                        }
                    }
                    Ok(Some(out))
                }
                other => Err(cx.err(format!("`{key}` must be an array of strings"), n.span, format!("found {}", other.kind()))),
            },
        }
    }

    /// `{ workspace = true }` 이면 `true`.
    fn inherits(&self, cx: &Ctx, key: &str) -> R<Option<Span>> {
        if let Some((_, _, n)) = self.get(key) {
            if let Val::Tab(t) = &n.v {
                t.only(cx, &format!("`{key}`"), &["workspace"])?;
                return match t.get("workspace") {
                    Some((_, _, Node { v: Val::Bool(true), .. })) => Ok(Some(n.span)),
                    Some((_, _, m)) => Err(cx.err("`workspace` must be `true`", m.span, "write `{ workspace = true }`")),
                    None => Err(cx.err(format!("`{key}` is a table without `workspace = true`"), n.span, "write `{ workspace = true }` or a plain value")),
                };
            }
        }
        Ok(None)
    }
}

// ------------------------------------------------------------------ 매니페스트

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bin,
    Lib,
}

#[derive(Clone, Debug)]
pub struct DepSpec {
    pub name: String,
    pub path: PathBuf,
    pub version: Option<Req>,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct TargetCfg {
    pub entry: Option<PathBuf>,
    pub out_name: Option<String>,
    pub deps: Vec<DepSpec>,
}

#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub name_span: Span,
    pub version: Version,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub tamc: Option<Req>,
    pub kind: Kind,
    pub entry: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub opt_level: u8,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub members: Vec<PathBuf>,
    pub default_members: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Manifest {
    pub path: PathBuf,
    pub dir: PathBuf,
    pub package: Option<Package>,
    pub workspace: Option<Workspace>,
    pub default_target: Option<String>,
    pub out_dir: PathBuf,
    pub dev: Profile,
    pub release: Profile,
    pub deps: Vec<DepSpec>,
    pub targets: BTreeMap<String, TargetCfg>,
    pub src: Src,
}

#[derive(Default, Clone)]
struct WsPackage {
    version: Option<String>,
    authors: Option<Vec<String>>,
    description: Option<String>,
    license: Option<String>,
    repository: Option<String>,
    tamc: Option<String>,
}

struct WsInfo {
    pkg: WsPackage,
    /// 이름 -> (경로(작업 공간 루트 기준 절대), 버전 요구)
    deps: HashMap<String, (PathBuf, Option<String>)>,
}

fn valid_name(n: &str) -> bool {
    let mut cs = n.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_lowercase())
        && n.len() <= 64
        && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `use` 경로에서 쓰는 이름 (`my-lib` -> `my_lib`).
pub fn use_name(pkg_name: &str) -> String {
    pkg_name.replace('-', "_")
}

pub fn read_source(path: &Path) -> R<Src> {
    let text = std::fs::read_to_string(path).map_err(|e| MError {
        diag: Diag::new(format!("cannot read `{}`: {e}", path.display()), Span::default(), ""),
        files: Vec::new(),
    })?;
    Ok(Src::new(path.display().to_string(), text))
}

fn parse_toml(src: &Src) -> R<Table> {
    match toml::de::DeTable::parse(&src.text) {
        Ok(t) => Ok(convert_table(src, t.get_ref())),
        Err(e) => {
            let span = e.span().map(|r| src.span(r)).unwrap_or_default();
            Err(MError {
                diag: Diag::new(format!("invalid TOML: {}", e.message()), span, "syntax error here"),
                files: vec![(src.path.clone(), src.text.clone())],
            })
        }
    }
}

fn rel_path(cx: &Ctx, dir: &Path, s: &str, span: Span, what: &str) -> R<PathBuf> {
    let p = Path::new(s);
    if s.is_empty() {
        return Err(cx.err(format!("{what} must not be empty"), span, "give a path"));
    }
    if p.is_absolute() {
        return Err(cx.err(format!("{what} must be a relative path"), span, "absolute paths make the build machine-specific"));
    }
    Ok(dir.join(p))
}

/// 하위 경로 전용 (빌드 산출물/진입점이 패키지 밖으로 나가지 않게 한다).
fn inner_path(cx: &Ctx, dir: &Path, s: &str, span: Span, what: &str) -> R<PathBuf> {
    let p = rel_path(cx, dir, s, span, what)?;
    if Path::new(s).components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(cx.err(format!("{what} must stay inside the package"), span, "`..` is not allowed here"));
    }
    Ok(p)
}

fn parse_deps(cx: &Ctx, dir: &Path, t: &Table, ws: Option<&WsInfo>, section: &str) -> R<Vec<DepSpec>> {
    let mut out = Vec::new();
    for (name, nspan, node) in &t.entries {
        if !valid_name(name) {
            return Err(cx.err(
                format!("invalid dependency name `{name}`"),
                *nspan,
                "names are lowercase letters, digits, `_` and `-`, starting with a letter",
            ));
        }
        let Val::Tab(spec) = &node.v else {
            return Err(cx.err(
                format!("dependency `{name}` must be a table"),
                node.span,
                format!("write `{name} = {{ path = \"../{name}\" }}` (only path dependencies exist)"),
            ));
        };
        spec.only(cx, &format!("dependency `{name}`"), &["path", "version", "workspace"])?;
        let (path, version) = if let Some((_, _, w)) = spec.get("workspace") {
            if !matches!(w.v, Val::Bool(true)) {
                return Err(cx.err("`workspace` must be `true`", w.span, "write `workspace = true`"));
            }
            let Some(info) = ws.and_then(|w| w.deps.get(name)) else {
                return Err(cx.err(
                    format!("dependency `{name}` is not in `[workspace.dependencies]`"),
                    w.span,
                    "add it to the workspace root's Tmake.toml",
                ));
            };
            (info.0.clone(), info.1.clone())
        } else {
            let Some((p, pspan)) = spec.string(cx, "path")? else {
                return Err(cx.err(
                    format!("dependency `{name}` needs a `path`"),
                    node.span,
                    "only path dependencies are supported: `{ path = \"../name\" }`",
                ));
            };
            (rel_path(cx, dir, &p, pspan, "`path`")?, spec.string(cx, "version")?.map(|v| v.0))
        };
        let req = match version {
            None => None,
            Some(v) => {
                let span = spec.get("version").map(|e| e.2.span).unwrap_or(node.span);
                Some(Req::parse(&v).map_err(|m| cx.err(format!("bad version requirement in {section}"), span, m))?)
            }
        };
        out.push(DepSpec { name: name.clone(), path, version: req, span: *nspan });
    }
    Ok(out)
}

fn parse_profile(cx: &Ctx, t: &Table, name: &str, default: u8) -> R<Profile> {
    let mut p = Profile { opt_level: default };
    if let Some((_, _, n)) = t.get(name) {
        let Val::Tab(pt) = &n.v else {
            return Err(cx.err(format!("`profile.{name}` must be a table"), n.span, format!("found {}", n.v.kind())));
        };
        pt.only(cx, &format!("[profile.{name}]"), &["opt-level"])?;
        if let Some((_, _, o)) = pt.get("opt-level") {
            match &o.v {
                Val::Int(i) if (0..=2).contains(i) => p.opt_level = *i as u8,
                _ => {
                    return Err(cx.err(
                        "`opt-level` must be 0, 1 or 2",
                        o.span,
                        "0 = no optimization, 1 = register promotion, 2 = promotion + full backend optimization",
                    ))
                }
            }
        }
    }
    Ok(p)
}

fn expand_members(cx: &Ctx, dir: &Path, items: &[(String, Span)]) -> R<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    for (m, span) in items {
        let base = inner_path(cx, dir, m.trim_end_matches("/*"), *span, "workspace member")?;
        if m.ends_with("/*") || m == "*" {
            let mut found = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&base) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.join(MANIFEST).is_file() {
                        found.push(p);
                    }
                }
            }
            found.sort();
            out.extend(found);
        } else {
            if m.contains('*') {
                return Err(cx.err("only a trailing `/*` glob is supported in `members`", *span, "e.g. \"libs/*\""));
            }
            if !base.join(MANIFEST).is_file() {
                return Err(cx.err(format!("workspace member `{m}` has no {MANIFEST}"), *span, format!("expected `{}`", base.join(MANIFEST).display())));
            }
            out.push(base);
        }
    }
    Ok(out)
}

/// 이 폴더를 멤버로 가진 작업 공간 루트의 매니페스트 폴더를 위로 올라가며 찾는다.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(start).ok()?;
    let mut cur = start.parent();
    while let Some(d) = cur {
        let m = d.join(MANIFEST);
        if m.is_file() {
            if let Ok(src) = read_source(&m) {
                if let Ok(t) = parse_toml(&src) {
                    if let Some((_, _, Node { v: Val::Tab(ws), .. })) = t.get("workspace") {
                        let cx = Ctx { src: &src };
                        if let Ok(Some(items)) = ws.strings(&cx, "members") {
                            if let Ok(ms) = expand_members(&cx, d, &items) {
                                if ms.iter().any(|p| std::fs::canonicalize(p).ok().as_deref() == Some(start.as_path())) {
                                    return Some(d.to_path_buf());
                                }
                            }
                        }
                    }
                }
            }
        }
        cur = d.parent();
    }
    None
}

fn load_ws_info(root: &Path) -> R<WsInfo> {
    let src = read_source(&root.join(MANIFEST))?;
    let t = parse_toml(&src)?;
    let cx = Ctx { src: &src };
    let mut info = WsInfo { pkg: WsPackage::default(), deps: HashMap::new() };
    if let Some((ws, _)) = t.table(&cx, "workspace")? {
        if let Some((p, _)) = ws.table(&cx, "package")? {
            p.only(&cx, "[workspace.package]", &["version", "authors", "description", "license", "repository", "tamc"])?;
            info.pkg = WsPackage {
                version: p.string(&cx, "version")?.map(|x| x.0),
                authors: p.strings(&cx, "authors")?.map(|v| v.into_iter().map(|x| x.0).collect()),
                description: p.string(&cx, "description")?.map(|x| x.0),
                license: p.string(&cx, "license")?.map(|x| x.0),
                repository: p.string(&cx, "repository")?.map(|x| x.0),
                tamc: p.string(&cx, "tamc")?.map(|x| x.0),
            };
        }
        if let Some((d, _)) = ws.table(&cx, "dependencies")? {
            for dep in parse_deps(&cx, root, d, None, "[workspace.dependencies]")? {
                info.deps.insert(dep.name.clone(), (dep.path, dep.version.map(|r| r.text)));
            }
        }
    }
    Ok(info)
}

impl Manifest {
    pub fn load(path: &Path) -> R<Manifest> {
        let src = read_source(path)?;
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
        let t = parse_toml(&src)?;
        let cx = Ctx { src: &src };
        t.only(&cx, "the manifest", &["package", "build", "profile", "dependencies", "target", "workspace"])?;

        // 작업 공간 상속 정보 (이 매니페스트 자신이 루트이거나, 위쪽에 루트가 있을 때)
        let is_root = t.get("workspace").is_some();
        let ws_info = if is_root {
            Some(load_ws_info(&dir)?)
        } else {
            match find_workspace_root(&dir) {
                Some(r) => Some(load_ws_info(&r)?),
                None => None,
            }
        };

        // [workspace]
        let mut workspace = None;
        if let Some((ws, span)) = t.table(&cx, "workspace")? {
            ws.only(&cx, "[workspace]", &["members", "exclude", "default-members", "package", "dependencies"])?;
            let members = match ws.strings(&cx, "members")? {
                Some(items) => expand_members(&cx, &dir, &items)?,
                None => Vec::new(),
            };
            let exclude: Vec<PathBuf> = match ws.strings(&cx, "exclude")? {
                Some(items) => items.iter().filter_map(|(s, sp)| inner_path(&cx, &dir, s, *sp, "`exclude`").ok()).collect(),
                None => Vec::new(),
            };
            let canon = |p: &PathBuf| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            let members: Vec<PathBuf> = members.into_iter().filter(|m| !exclude.iter().any(|e| canon(e) == canon(m))).collect();
            let default_members = match ws.strings(&cx, "default-members")? {
                Some(items) => {
                    let mut v = Vec::new();
                    for (s, sp) in &items {
                        let p = inner_path(&cx, &dir, s, *sp, "`default-members` entry")?;
                        if !members.iter().any(|m| canon(m) == canon(&p)) {
                            return Err(cx.err(format!("`{s}` is not a workspace member"), *sp, "list it in `members` first"));
                        }
                        v.push(p);
                    }
                    v
                }
                None => members.clone(),
            };
            if members.is_empty() && t.get("package").is_none() {
                return Err(cx.err("the workspace has no members", span, "add `members = [\"app\", \"libs/*\"]`"));
            }
            workspace = Some(Workspace { members, default_members });
        }

        // [package]
        let mut package = None;
        if let Some((p, pspan)) = t.table(&cx, "package")? {
            p.only(&cx, "[package]", &["name", "version", "authors", "description", "license", "repository", "tamc", "kind", "entry"])?;
            let Some((name, name_span)) = p.string(&cx, "name")? else {
                return Err(cx.err("`[package]` needs a `name`", pspan, "add `name = \"my-app\"`"));
            };
            if !valid_name(&name) {
                return Err(cx.err(
                    format!("invalid package name `{name}`"),
                    name_span,
                    "use lowercase letters, digits, `_` and `-`, starting with a letter (at most 64 characters)",
                ));
            }
            if name == "tam" {
                return Err(cx.err("`tam` is reserved for the standard library", name_span, "pick another name"));
            }
            let inherited = |key: &str| -> R<bool> {
                match p.inherits(&cx, key)? {
                    Some(sp) => {
                        if ws_info.is_none() {
                            return Err(cx.err(format!("`{key}.workspace = true` but there is no workspace"), sp, "this package is not inside a workspace"));
                        }
                        Ok(true)
                    }
                    None => Ok(false),
                }
            };
            let wsp = ws_info.as_ref().map(|w| w.pkg.clone()).unwrap_or_default();
            let need = |what: &str, v: Option<String>, span: Span| -> R<String> {
                v.ok_or_else(|| cx.err(format!("the workspace has no `{what}` to inherit"), span, format!("set `{what}` in `[workspace.package]`")))
            };
            let version_text = if inherited("version")? {
                let sp = p.get("version").unwrap().2.span;
                need("version", wsp.version.clone(), sp)?
            } else {
                match p.string(&cx, "version")? {
                    Some((v, _)) => v,
                    None => return Err(cx.err("`[package]` needs a `version`", pspan, "add `version = \"0.1.0\"`")),
                }
            };
            let vspan = p.get("version").map(|e| e.2.span).unwrap_or(pspan);
            let Some(version) = Version::parse(&version_text) else {
                return Err(cx.err(format!("`{version_text}` is not a valid version"), vspan, "use `MAJOR.MINOR.PATCH`, e.g. \"0.1.0\""));
            };
            let authors = if inherited("authors")? {
                need("authors", wsp.authors.as_ref().map(|a| a.join("\u{1}")), pspan)?.split('\u{1}').map(String::from).collect()
            } else {
                p.strings(&cx, "authors")?.map(|v| v.into_iter().map(|x| x.0).collect()).unwrap_or_default()
            };
            let text_field = |key: &str, ws_val: Option<String>| -> R<Option<String>> {
                if inherited(key)? {
                    let sp = p.get(key).unwrap().2.span;
                    Ok(Some(need(key, ws_val, sp)?))
                } else {
                    Ok(p.string(&cx, key)?.map(|x| x.0))
                }
            };
            let description = text_field("description", wsp.description.clone())?;
            let license = text_field("license", wsp.license.clone())?;
            let repository = text_field("repository", wsp.repository.clone())?;
            let tamc_text = text_field("tamc", wsp.tamc.clone())?;
            let tamc = match tamc_text {
                None => None,
                Some(s) => {
                    let sp = p.get("tamc").map(|e| e.2.span).unwrap_or(pspan);
                    Some(Req::parse(&s).map_err(|m| cx.err("bad `tamc` version requirement", sp, m))?)
                }
            };
            let explicit_kind = match p.string(&cx, "kind")? {
                None => None,
                Some((k, sp)) => match k.as_str() {
                    "bin" => Some(Kind::Bin),
                    "lib" => Some(Kind::Lib),
                    _ => return Err(cx.err(format!("unknown package kind `{k}`"), sp, "use \"bin\" or \"lib\"")),
                },
            };
            let explicit_entry = match p.string(&cx, "entry")? {
                Some((e, sp)) => Some(inner_path(&cx, &dir, &e, sp, "`entry`")?),
                None => None,
            };
            let main = dir.join("src/main.tam");
            let lib = dir.join("src/lib.tam");
            let kind = explicit_kind.unwrap_or(if explicit_entry.is_some() {
                Kind::Bin
            } else if main.is_file() {
                Kind::Bin
            } else if lib.is_file() {
                Kind::Lib
            } else {
                return Err(cx.err(
                    format!("package `{name}` has no `src/main.tam` or `src/lib.tam`"),
                    name_span,
                    "create one, or set `entry = \"path/to/file.tam\"` and `kind`",
                ));
            });
            let entry = explicit_entry.unwrap_or(if kind == Kind::Bin { main } else { lib });
            package = Some(Package {
                name,
                name_span,
                version,
                authors,
                description,
                license,
                repository,
                tamc,
                kind,
                entry,
            });
        }
        if package.is_none() && workspace.is_none() {
            return Err(cx.err("the manifest needs a `[package]` or a `[workspace]`", Span::new(1, 1, 1), "add `[package]` with `name` and `version`"));
        }

        // [build]
        let mut default_target = None;
        let mut out_dir = dir.join("build");
        if let Some((b, _)) = t.table(&cx, "build")? {
            b.only(&cx, "[build]", &["target", "out-dir"])?;
            if let Some((tg, sp)) = b.string(&cx, "target")? {
                if !TARGETS.contains(&tg.as_str()) {
                    return Err(cx.err(format!("unknown target `{tg}`"), sp, format!("targets: {}", TARGETS.join(", "))));
                }
                default_target = Some(tg);
            }
            if let Some((o, sp)) = b.string(&cx, "out-dir")? {
                out_dir = inner_path(&cx, &dir, &o, sp, "`out-dir`")?;
            }
        }

        // [profile.*]
        let (dev, release) = match t.table(&cx, "profile")? {
            Some((pt, _)) => {
                pt.only(&cx, "[profile]", &["dev", "release"])?;
                (parse_profile(&cx, pt, "dev", 0)?, parse_profile(&cx, pt, "release", 2)?)
            }
            None => (Profile { opt_level: 0 }, Profile { opt_level: 2 }),
        };

        // [dependencies]
        let deps = match t.table(&cx, "dependencies")? {
            Some((d, _)) => parse_deps(&cx, &dir, d, ws_info.as_ref(), "[dependencies]")?,
            None => Vec::new(),
        };

        // [target.*]
        let mut targets = BTreeMap::new();
        if let Some((tt, _)) = t.table(&cx, "target")? {
            for (tname, tspan, tn) in &tt.entries {
                if !TARGETS.contains(&tname.as_str()) {
                    return Err(cx.err(format!("unknown target `{tname}`"), *tspan, format!("targets: {}", TARGETS.join(", "))));
                }
                let Val::Tab(ct) = &tn.v else {
                    return Err(cx.err(format!("`target.{tname}` must be a table"), tn.span, format!("found {}", tn.v.kind())));
                };
                ct.only(&cx, &format!("[target.{tname}]"), &["entry", "out-name", "dependencies"])?;
                let mut cfg = TargetCfg::default();
                if let Some((e, sp)) = ct.string(&cx, "entry")? {
                    cfg.entry = Some(inner_path(&cx, &dir, &e, sp, "`entry`")?);
                }
                if let Some((o, sp)) = ct.string(&cx, "out-name")? {
                    if o.is_empty() || o.contains(['/', '\\']) || o.starts_with('.') {
                        return Err(cx.err("`out-name` must be a plain file name", sp, "no path separators"));
                    }
                    cfg.out_name = Some(o);
                }
                if let Some((d, _)) = ct.table(&cx, "dependencies")? {
                    cfg.deps = parse_deps(&cx, &dir, d, ws_info.as_ref(), &format!("[target.{tname}.dependencies]"))?;
                }
                targets.insert(tname.clone(), cfg);
            }
        }

        Ok(Manifest { path: path.to_path_buf(), dir, package, workspace, default_target, out_dir, dev, release, deps, targets, src })
    }

    pub fn package_name(&self) -> Option<&str> {
        self.package.as_ref().map(|p| p.name.as_str())
    }
}

// ------------------------------------------------------------------ 의존성 해석

#[derive(Debug)]
pub struct Pkg {
    pub name: String,
    pub version: Version,
    pub dir: PathBuf,
    pub manifest_path: PathBuf,
    pub entry: PathBuf,
    pub src_root: PathBuf,
    pub kind: Kind,
    /// (`use`에서 쓰는 이름, 패키지 번호)
    pub deps: Vec<(String, usize)>,
    pub description: Option<String>,
    pub out_name: Option<String>,
}

#[derive(Debug)]
pub struct Graph {
    /// 0번이 루트 패키지
    pub pkgs: Vec<Pkg>,
    /// 읽은 모든 매니페스트의 (경로, 내용) — 캐시 지문과 진단에 쓴다.
    pub manifests: Vec<(String, String)>,
}

fn check_tamc(cx: &Ctx, p: &Package) -> R<()> {
    if let Some(req) = &p.tamc {
        let me = Version::parse(TAMC_VERSION).unwrap();
        if !req.matches(&me) {
            return Err(cx.err(
                format!("package `{}` needs tamc {req}, but this is tamc {me}", p.name),
                p.name_span,
                "upgrade tamc, or relax the `tamc` requirement",
            ));
        }
    }
    Ok(())
}

/// 루트 매니페스트에서 시작해 `target`에 맞는 의존성 그래프를 만든다.
pub fn resolve(root: &Manifest, target: &str) -> R<Graph> {
    let Some(rootpkg) = &root.package else {
        return Err(MError {
            diag: Diag::new("this manifest is a virtual workspace, not a package", Span::new(1, 1, 1), "select a member with `-p <name>`"),
            files: vec![(root.src.path.clone(), root.src.text.clone())],
        });
    };
    let mut g = Graph { pkgs: Vec::new(), manifests: Vec::new() };
    let mut by_dir: HashMap<PathBuf, usize> = HashMap::new();
    let mut by_name: HashMap<String, PathBuf> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();

    fn visit(
        g: &mut Graph,
        by_dir: &mut HashMap<PathBuf, usize>,
        by_name: &mut HashMap<String, PathBuf>,
        stack: &mut Vec<String>,
        m: &Manifest,
        target: &str,
        is_root: bool,
        reqs: Option<(&Req, &str, Span, &Manifest)>,
    ) -> R<usize> {
        let p = m.package.as_ref().expect("package manifest");
        let cx = Ctx { src: &m.src };
        let canon = std::fs::canonicalize(&m.dir).unwrap_or_else(|_| m.dir.clone());
        if stack.contains(&p.name) {
            let mut cyc = stack.clone();
            cyc.push(p.name.clone());
            return Err(cx.err(format!("dependency cycle: {}", cyc.join(" -> ")), p.name_span, "packages may not depend on each other in a loop"));
        }
        if let Some(&i) = by_dir.get(&canon) {
            return Ok(i);
        }
        if let Some((req, depname, span, from)) = reqs {
            if !req.matches(&p.version) {
                let fcx = Ctx { src: &from.src };
                return Err(fcx.err(
                    format!("dependency `{depname}` is version {}, which does not satisfy `{req}`", p.version),
                    span,
                    "update the `version` requirement or the dependency",
                ));
            }
        }
        if let Some(other) = by_name.get(&p.name) {
            if *other != canon {
                return Err(cx.err(
                    format!("two different packages are named `{}`", p.name),
                    p.name_span,
                    format!("also at `{}`", other.display()),
                ));
            }
        }
        if !is_root && p.kind != Kind::Lib {
            return Err(cx.err(
                format!("package `{}` is a `bin` and cannot be used as a dependency", p.name),
                p.name_span,
                "dependencies must be libraries (`kind = \"lib\"`, entry `src/lib.tam`)",
            ));
        }
        check_tamc(&cx, p)?;
        g.manifests.push((m.src.path.clone(), m.src.text.clone()));
        let tcfg = m.targets.get(target);
        let entry = tcfg.and_then(|c| c.entry.clone()).filter(|_| is_root).unwrap_or_else(|| p.entry.clone());
        if !entry.is_file() {
            return Err(cx.err(
                format!("entry file `{}` of package `{}` does not exist", entry.display(), p.name),
                p.name_span,
                "create it or fix `entry`",
            ));
        }
        let idx = g.pkgs.len();
        let src_root = normalize(entry.parent().unwrap_or(Path::new(".")));
        g.pkgs.push(Pkg {
            name: p.name.clone(),
            version: p.version,
            dir: normalize(&m.dir),
            manifest_path: normalize(&m.path),
            entry: normalize(&entry),
            src_root,
            kind: p.kind,
            deps: Vec::new(),
            description: p.description.clone(),
            out_name: tcfg.and_then(|c| c.out_name.clone()),
        });
        by_dir.insert(canon.clone(), idx);
        by_name.insert(p.name.clone(), canon);
        stack.push(p.name.clone());
        let mut specs: Vec<&DepSpec> = m.deps.iter().collect();
        if let Some(c) = tcfg {
            specs.extend(c.deps.iter());
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut deps = Vec::new();
        for d in specs {
            if !seen.insert(d.name.as_str()) {
                return Err(cx.err(format!("dependency `{}` is listed twice", d.name), d.span, "remove one of them"));
            }
            let dm_path = d.path.join(MANIFEST);
            let dm = Manifest::load(&dm_path).map_err(|e| {
                if e.files.is_empty() {
                    cx.err(format!("cannot load dependency `{}`", d.name), d.span, format!("`{}` was not found", dm_path.display()))
                } else {
                    e
                }
            })?;
            let Some(dp) = &dm.package else {
                return Err(cx.err(format!("`{}` is a workspace, not a package", d.path.display()), d.span, "point `path` at a package folder"));
            };
            if dp.name != d.name {
                return Err(cx.err(
                    format!("dependency `{}` points at package `{}`", d.name, dp.name),
                    d.span,
                    "the dependency key must equal the package name",
                ));
            }
            let req_ctx = d.version.as_ref().map(|r| (r, d.name.as_str(), d.span, m));
            let di = visit(g, by_dir, by_name, stack, &dm, target, false, req_ctx)?;
            deps.push((use_name(&d.name), di));
        }
        stack.pop();
        g.pkgs[idx].deps = deps;
        Ok(idx)
    }

    let _ = rootpkg;
    visit(&mut g, &mut by_dir, &mut by_name, &mut stack, root, target, true, None)?;
    Ok(g)
}

impl Graph {
    /// 의존하는 쪽이 뒤에 오는 순서의 패키지 번호들.
    pub fn topo(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut seen = vec![false; self.pkgs.len()];
        fn go(g: &Graph, i: usize, seen: &mut Vec<bool>, out: &mut Vec<usize>) {
            if seen[i] {
                return;
            }
            seen[i] = true;
            for (_, d) in &g.pkgs[i].deps {
                go(g, *d, seen, out);
            }
            out.push(i);
        }
        go(self, 0, &mut seen, &mut out);
        out
    }

    /// 이 그래프의 패키지들을 잠금 항목에 합친다.
    pub fn collect_lock(&self, base: &Path, into: &mut BTreeMap<String, LockEntry>) {
        for p in &self.pkgs {
            let e = into.entry(p.name.clone()).or_insert_with(|| LockEntry {
                version: p.version.to_string(),
                path: relative_to(&p.dir, base),
                deps: BTreeSet::new(),
            });
            for (_, d) in &p.deps {
                e.deps.insert(self.pkgs[*d].name.clone());
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    pub version: String,
    pub path: String,
    pub deps: BTreeSet<String>,
}

/// `Tmake.lock` 내용: TOML, 이름순으로 고정되어 같은 입력이면 항상 같은 바이트다.
pub fn lock_text(entries: &BTreeMap<String, LockEntry>) -> String {
    let mut s = String::from("# This file is generated by tamc. Do not edit it by hand.\nversion = 1\n");
    for (name, e) in entries {
        s.push_str("\n[[package]]\n");
        s.push_str(&format!("name = {name:?}\nversion = {:?}\npath = {:?}\n", e.version, e.path));
        if !e.deps.is_empty() {
            s.push_str(&format!("dependencies = [{}]\n", e.deps.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>().join(", ")));
        }
    }
    s
}

/// `.`/`..`를 문자열 수준에서 정리한다 (파일 시스템을 보지 않는다. 심볼릭 링크는 그대로 둔다).
pub fn normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// `p`를 `base` 기준 상대 경로 문자열(`/` 구분)로 바꾼다.
pub fn relative_to(p: &Path, base: &Path) -> String {
    let (Ok(p), Ok(b)) = (std::fs::canonicalize(p), std::fs::canonicalize(base)) else {
        return p.display().to_string();
    };
    let pc: Vec<_> = p.components().collect();
    let bc: Vec<_> = b.components().collect();
    let mut i = 0;
    while i < pc.len() && i < bc.len() && pc[i] == bc[i] {
        i += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in i..bc.len() {
        parts.push("..".into());
    }
    for c in &pc[i..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

// ------------------------------------------------------------------ 새 프로젝트

pub fn check_new_name(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!(
            "invalid package name `{name}`: use lowercase letters, digits, `_` and `-`, starting with a letter"
        ));
    }
    if name == "tam" {
        return Err("`tam` is reserved for the standard library".into());
    }
    Ok(())
}

pub fn manifest_template(name: &str, lib: bool) -> String {
    let mut s = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ntamc = \">={}\"\n", {
        let v = Version::parse(TAMC_VERSION).unwrap();
        format!("{}.{}", v.major, v.minor)
    });
    if lib {
        s.push_str("kind = \"lib\"\n");
    }
    s.push_str("\n[dependencies]\n");
    s
}
