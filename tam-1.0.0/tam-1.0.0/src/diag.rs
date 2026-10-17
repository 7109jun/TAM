//! 위치 정보와 사람이 읽을 수 있는 진단 메시지.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub file: u16,
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

impl Span {
    pub fn new(line: u32, col: u32, len: u32) -> Self {
        Span { file: 0, line, col, len }
    }

    /// `self`에서 `other`의 끝까지 덮는 span. 줄이 다르면 `self`의 첫 토큰만 가리킨다.
    pub fn to(self, other: Span) -> Span {
        if self.file == other.file && self.line == other.line && other.col + other.len >= self.col {
            Span { file: self.file, ..Span::new(self.line, self.col, other.col + other.len - self.col) }
        } else {
            self
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub msg: String,
    pub span: Span,
    pub label: String,
    /// 관련된 다른 위치들 (예: 값이 이동된 곳).
    pub secondary: Vec<(Span, String)>,
}

impl Diag {
    pub fn new(msg: impl Into<String>, span: Span, label: impl Into<String>) -> Self {
        Diag {
            msg: msg.into(),
            span,
            label: label.into(),
            secondary: Vec::new(),
        }
    }

    pub fn with_secondary(mut self, span: Span, label: impl Into<String>) -> Self {
        self.secondary.push((span, label.into()));
        self
    }

    /// 파일 하나짜리 렌더링 (테스트/단일 파일용).
    pub fn render(&self, file: &str, src: &str) -> String {
        self.render_multi(&[(file.to_string(), src.to_string())])
    }

    /// `files[span.file]`에서 이름과 소스를 찾아 렌더링한다.
    pub fn render_multi(&self, files: &[(String, String)]) -> String {
        // 위치가 없는 오류(파일을 못 읽음 등)는 메시지만 보여 준다.
        if self.span.line == 0 && self.secondary.is_empty() {
            let mut out = format!("error: {}\n", self.msg);
            if !self.label.is_empty() {
                out.push_str(&format!("note: {}\n", self.label));
            }
            return out;
        }
        // (위치, 설명, 밑줄 문자). 주 위치는 `^`, 보조 위치는 `-`.
        let mut labels: Vec<(Span, &str, char)> = vec![(self.span, self.label.as_str(), '^')];
        for (s, t) in &self.secondary {
            labels.push((*s, t.as_str(), '-'));
        }
        labels.sort_by_key(|(s, _, c)| (s.file, s.line, s.col, *c != '-'));

        let width = labels
            .iter()
            .map(|(s, _, _)| s.line.to_string().len())
            .max()
            .unwrap_or(1);
        let pad = " ".repeat(width);
        let col = self.span.col.max(1);
        let name_of = |f: u16| files.get(f as usize).map(|x| x.0.as_str()).unwrap_or("<unknown>");

        let mut out = String::new();
        out.push_str(&format!("error: {}\n", self.msg));
        out.push_str(&format!("{pad}--> {}:{}:{col}\n", name_of(self.span.file), self.span.line));
        out.push_str(&format!("{pad} |\n"));

        let mut prev: Option<(u16, u32)> = None;
        for (span, text, ch) in labels {
            let src = files.get(span.file as usize).map(|x| x.1.as_str()).unwrap_or("");
            let line_text = src.lines().nth((span.line as usize).saturating_sub(1)).unwrap_or("");
            if prev.map(|p| p.0) != Some(span.file) && prev.is_some() {
                out.push_str(&format!("{pad}::: {}:{}\n", name_of(span.file), span.line));
            }
            if prev != Some((span.file, span.line)) {
                if matches!(prev, Some((pf, pl)) if pf == span.file && span.line > pl + 1) {
                    out.push_str("...\n");
                }
                out.push_str(&format!("{:>width$} | {line_text}\n", span.line));
            }
            // 탭은 그대로 복사해서 밑줄 정렬을 유지한다.
            let indent: String = line_text
                .chars()
                .take(span.col.max(1) as usize - 1)
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            let marks = ch.to_string().repeat(span.len.max(1) as usize);
            if text.is_empty() {
                out.push_str(&format!("{pad} | {indent}{marks}\n"));
            } else {
                out.push_str(&format!("{pad} | {indent}{marks} {text}\n"));
            }
            prev = Some((span.file, span.line));
        }
        out
    }
}
