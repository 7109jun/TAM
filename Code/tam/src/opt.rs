//! IR 텍스트 수준의 `mem2reg`: 주소가 새지 않는 스칼라 지역 변수(`alloca`)를 SSA 값으로 바꾼다.
//!
//! 번들 도구가 `llc`뿐이라(IR 최적화 도구 `opt`는 설치 폴더에 없다) 모든 지역 변수가
//! 스택 `load`/`store`로 남는다. 이 패스는 표준 알고리즘(지배자 트리 + 지배 경계 + 이름 바꾸기)으로
//! 그것을 레지스터로 올린다. 의심스러운 모양이 나오면 아무것도 바꾸지 않고 원본을 돌려준다.

use std::collections::HashMap;

struct Block {
    label: String,
    lines: Vec<String>,
}

struct Cand {
    name: String,
    ty: String,
    /// 주소가 다른 곳에 쓰이면 승격하지 않는다.
    ok: bool,
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$' || c == '-'
}

/// 줄 안의 `%이름` 토큰들(`%` 포함).
fn tokens(line: &str) -> Vec<&str> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let mut j = i + 1;
            while j < b.len() && is_ident(b[j] as char) {
                j += 1;
            }
            if j > i + 1 {
                out.push(&line[i..j]);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn subst(line: &str, map: &HashMap<String, String>) -> String {
    if map.is_empty() || !line.contains('%') {
        return line.to_string();
    }
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let mut j = i + 1;
            while j < b.len() && is_ident(b[j] as char) {
                j += 1;
            }
            let tok = &line[i..j];
            match map.get(tok) {
                Some(v) => out.push_str(v),
                None => out.push_str(tok),
            }
            i = j;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// `  %x = alloca TYPE` -> (x, TYPE)
fn parse_alloca(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("  ")?;
    let (lhs, rhs) = rest.split_once(" = alloca ")?;
    if !lhs.starts_with('%') || rhs.contains(',') && !rhs.starts_with('{') {
        return None;
    }
    // 정렬/개수 인자가 붙은 모양(`, align`, `, i32 N`)은 다루지 않는다.
    let ty = rhs.trim();
    if ty.starts_with('[') || ty.is_empty() {
        return None;
    }
    if ty.starts_with('{') {
        // `{ ptr, i64 }` 뒤에 더 붙은 것이 없어야 한다.
        if !ty.ends_with('}') {
            return None;
        }
    }
    Some((lhs.to_string(), ty.to_string()))
}

/// `  %t = load TY, ptr %x` -> (t, TY, x)
fn parse_load(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix("  ")?;
    let (lhs, rhs) = rest.split_once(" = load ")?;
    let (ty, ptr) = rhs.rsplit_once(", ptr ")?;
    if !lhs.starts_with('%') || !ptr.starts_with('%') || ptr.contains(' ') || ptr.contains(',') {
        return None;
    }
    Some((lhs, ty, ptr))
}

/// `  store TY V, ptr %x` -> (TY, V, x)
fn parse_store(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix("  store ")?;
    let (head, ptr) = rest.rsplit_once(", ptr ")?;
    if !ptr.starts_with('%') || ptr.contains(' ') || ptr.contains(',') {
        return None;
    }
    // head = "TY V": 값은 마지막 공백 뒤의 토큰, 단 `{ ptr, i64 } %v`처럼 타입에 공백이 있다.
    let (ty, v) = head.rsplit_once(' ')?;
    Some((ty, v, ptr))
}

fn succs(last: &str) -> Option<Vec<String>> {
    let t = last.trim();
    if t.starts_with("ret") || t == "unreachable" {
        return Some(Vec::new());
    }
    if let Some(r) = t.strip_prefix("br label %") {
        return Some(vec![r.trim().to_string()]);
    }
    if let Some(r) = t.strip_prefix("br i1 ") {
        let (_, labels) = r.split_once(", label %")?;
        let (a, b) = labels.split_once(", label %")?;
        let mut v = vec![a.trim().to_string(), b.trim().to_string()];
        v.dedup();
        return Some(v);
    }
    None
}

use std::sync::atomic::{AtomicU8, Ordering};

static LEVEL: AtomicU8 = AtomicU8::new(2);

/// 최적화 수준: 0 = 없음, 1 = 레지스터 승격, 2 = 승격 + 백엔드(clang/llc) 최적화. 기본 2.
pub fn set_level(l: u8) {
    LEVEL.store(l.min(2), Ordering::Relaxed);
}

pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

pub fn promote(func: &str) -> String {
    if level() == 0 || std::env::var_os("TAMC_NO_OPT").is_some() {
        return func.to_string();
    }
    promote_inner(func).unwrap_or_else(|| func.to_string())
}

fn promote_inner(func: &str) -> Option<String> {
    let all: Vec<&str> = func.lines().collect();
    let hdr = all.iter().position(|l| l.starts_with("define "))?;
    let end = all.iter().rposition(|l| *l == "}")?;
    // 지원하지 않는 제어 흐름/명령이 있으면 포기한다.
    for l in &all[hdr + 1..end] {
        if l.contains(" switch ") || l.contains("indirectbr") || l.contains(" invoke ") || l.contains("landingpad") {
            return None;
        }
    }
    // ---- 블록 나누기 ----
    let mut blocks: Vec<Block> = Vec::new();
    for l in &all[hdr + 1..end] {
        if !l.starts_with(' ') && l.ends_with(':') {
            blocks.push(Block { label: l[..l.len() - 1].to_string(), lines: Vec::new() });
        } else {
            blocks.last_mut()?.lines.push(l.to_string());
        }
    }
    if blocks.is_empty() || blocks[0].label != "entry" {
        return None;
    }
    let n = blocks.len();
    let index: HashMap<String, usize> = blocks.iter().enumerate().map(|(i, b)| (b.label.clone(), i)).collect();
    if index.len() != n {
        return None;
    }
    // ---- CFG ----
    let mut succ: Vec<Vec<usize>> = Vec::with_capacity(n);
    for b in &blocks {
        let last = b.lines.last()?;
        let s = succs(last)?;
        let mut v = Vec::new();
        for name in s {
            v.push(*index.get(&name)?);
        }
        succ.push(v);
    }
    let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, ss) in succ.iter().enumerate() {
        for &s in ss {
            pred[s].push(i);
        }
    }
    // 도달 가능성 + 역후위 순서
    let mut visited = vec![false; n];
    let mut post: Vec<usize> = Vec::new();
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    visited[0] = true;
    while let Some(&mut (b, ref mut k)) = stack.last_mut() {
        if *k < succ[b].len() {
            let s = succ[b][*k];
            *k += 1;
            if !visited[s] {
                visited[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
            stack.pop();
        }
    }
    let rpo: Vec<usize> = post.iter().rev().copied().collect();
    let mut rpo_num = vec![usize::MAX; n];
    for (i, &b) in rpo.iter().enumerate() {
        rpo_num[b] = i;
    }

    // ---- 후보 alloca ----
    let mut cands: Vec<Cand> = Vec::new();
    let mut cidx: HashMap<String, usize> = HashMap::new();
    for l in &blocks[0].lines {
        if let Some((name, ty)) = parse_alloca(l) {
            cidx.insert(name.clone(), cands.len());
            cands.push(Cand { name, ty, ok: true });
        }
    }
    if cands.is_empty() {
        return None;
    }
    for (bi, b) in blocks.iter().enumerate() {
        for l in &b.lines {
            if bi == 0 && parse_alloca(l).is_some() {
                continue;
            }
            let toks = tokens(l);
            let mentioned: Vec<&&str> = toks.iter().filter(|t| cidx.contains_key(**t)).collect();
            if mentioned.is_empty() {
                continue;
            }
            let mut good = false;
            if let Some((t, ty, p)) = parse_load(l) {
                if let Some(&ci) = cidx.get(p) {
                    if cands[ci].ty == ty && mentioned.len() == 1 && !cidx.contains_key(t) {
                        good = true;
                    }
                }
            } else if let Some((ty, v, p)) = parse_store(l) {
                if let Some(&ci) = cidx.get(p) {
                    if cands[ci].ty == ty && mentioned.len() == 1 && v != p {
                        good = true;
                    }
                }
            }
            if !good {
                for t in mentioned {
                    cands[cidx[*t]].ok = false;
                }
            }
        }
    }
    let promo: Vec<usize> = (0..cands.len()).filter(|&i| cands[i].ok).collect();
    if promo.is_empty() {
        return None;
    }
    let mut pvar: HashMap<&str, usize> = HashMap::new(); // 이름 -> 승격 변수 번호
    for (k, &ci) in promo.iter().enumerate() {
        pvar.insert(cands[ci].name.as_str(), k);
    }
    let nv = promo.len();

    // ---- 지배자 (Cooper-Harvey-Kennedy) ----
    let mut idom = vec![usize::MAX; n];
    idom[0] = 0;
    let intersect = |idom: &Vec<usize>, mut a: usize, mut b: usize| -> usize {
        while a != b {
            while rpo_num[a] > rpo_num[b] {
                a = idom[a];
            }
            while rpo_num[b] > rpo_num[a] {
                b = idom[b];
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new = usize::MAX;
            for &p in &pred[b] {
                if idom[p] == usize::MAX {
                    continue;
                }
                new = if new == usize::MAX { p } else { intersect(&idom, p, new) };
            }
            if new != usize::MAX && idom[b] != new {
                idom[b] = new;
                changed = true;
            }
        }
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &b in rpo.iter().skip(1) {
        children[idom[b]].push(b);
    }
    // 지배 경계
    let mut df: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &b in &rpo {
        let live_preds: Vec<usize> = pred[b].iter().copied().filter(|&p| visited[p]).collect();
        if live_preds.len() >= 2 {
            for p in live_preds {
                let mut r = p;
                while r != idom[b] {
                    if !df[r].contains(&b) {
                        df[r].push(b);
                    }
                    r = idom[r];
                }
            }
        }
    }

    // ---- phi 위치 (준-가지치기: 블록을 넘어 읽히는 변수만) ----
    let mut def_blocks: Vec<Vec<usize>> = vec![Vec::new(); nv];
    let mut global = vec![false; nv];
    for &bi in &rpo {
        let mut stored = vec![false; nv];
        for l in &blocks[bi].lines {
            if let Some((_, _, p)) = parse_load(l) {
                if let Some(&k) = pvar.get(p) {
                    if !stored[k] {
                        global[k] = true;
                    }
                }
            } else if let Some((_, _, p)) = parse_store(l) {
                if let Some(&k) = pvar.get(p) {
                    stored[k] = true;
                    if !def_blocks[k].contains(&bi) {
                        def_blocks[k].push(bi);
                    }
                }
            }
        }
    }
    // phis[block] = (변수, phi 이름)
    let mut phis: Vec<Vec<(usize, String)>> = vec![Vec::new(); n];
    for k in 0..nv {
        if !global[k] {
            continue;
        }
        let mut has = vec![false; n];
        let mut work = def_blocks[k].clone();
        let mut queued: Vec<bool> = vec![false; n];
        for &b in &work {
            queued[b] = true;
        }
        while let Some(b) = work.pop() {
            for &d in &df[b] {
                if !has[d] {
                    has[d] = true;
                    let name = format!("{}.p{}", cands[promo[k]].name.trim_start_matches('%'), d);
                    phis[d].push((k, name));
                    if !queued[d] {
                        queued[d] = true;
                        work.push(d);
                    }
                }
            }
        }
    }

    // ---- 이름 바꾸기 ----
    let mut out_lines: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut incoming: Vec<HashMap<usize, Vec<(String, usize)>>> = vec![HashMap::new(); n]; // 블록 -> 변수 -> [(값, pred)]
    let mut map: HashMap<String, String> = HashMap::new();
    let mut stacks: Vec<Vec<String>> = vec![vec!["undef".to_string()]; nv];

    // 재귀 대신 명시적 스택: (블록, 단계, 이 블록에서 push한 변수들)
    enum Step {
        Enter(usize),
        Leave(Vec<usize>),
    }
    let mut work = vec![Step::Enter(0)];
    while let Some(step) = work.pop() {
        match step {
            Step::Leave(pushed) => {
                for k in pushed.into_iter().rev() {
                    stacks[k].pop();
                }
            }
            Step::Enter(b) => {
                let mut pushed: Vec<usize> = Vec::new();
                for (k, name) in &phis[b] {
                    stacks[*k].push(format!("%{}", &name));
                    pushed.push(*k);
                }
                let mut lines = Vec::new();
                for l in &blocks[b].lines {
                    if b == 0 {
                        if let Some((name, _)) = parse_alloca(l) {
                            if pvar.contains_key(name.as_str()) {
                                continue;
                            }
                        }
                    }
                    if let Some((t, _ty, p)) = parse_load(l) {
                        if let Some(&k) = pvar.get(p) {
                            map.insert(t.to_string(), stacks[k].last().unwrap().clone());
                            continue;
                        }
                    }
                    if let Some((_ty, v, p)) = parse_store(l) {
                        if let Some(&k) = pvar.get(p) {
                            let val = subst(v, &map);
                            stacks[k].push(val);
                            pushed.push(k);
                            continue;
                        }
                    }
                    lines.push(subst(l, &map));
                }
                out_lines[b] = lines;
                // 후속 블록의 phi에 이 블록 끝의 값을 알려 준다.
                for &s in &succ[b] {
                    for (k, _) in &phis[s] {
                        let v = stacks[*k].last().unwrap().clone();
                        incoming[s].entry(*k).or_default().push((v, b));
                    }
                }
                work.push(Step::Leave(pushed));
                for &c in children[b].iter().rev() {
                    work.push(Step::Enter(c));
                }
            }
        }
    }
    // 도달 불가능한 블록: 승격된 변수의 load는 undef, store는 버린다.
    for b in 0..n {
        if visited[b] {
            continue;
        }
        let mut lines = Vec::new();
        for l in &blocks[b].lines {
            if let Some((t, _, p)) = parse_load(l) {
                if pvar.contains_key(p) {
                    map.insert(t.to_string(), "undef".to_string());
                    continue;
                }
            }
            if let Some((_, _, p)) = parse_store(l) {
                if pvar.contains_key(p) {
                    continue;
                }
            }
            lines.push(subst(l, &map));
        }
        out_lines[b] = lines;
    }
    // 도달 불가능한 블록이 도달 가능한 블록 밖으로 값을 내보내는 일은 없지만, 그 블록의 명령이
    // 쓰는 load 결과(map)가 뒤늦게 정해졌을 수 있으니 한 번 더 치환한다.
    for b in 0..n {
        if !visited[b] {
            for l in out_lines[b].iter_mut() {
                *l = subst(l, &map);
            }
        }
    }
    // 도달 가능한 블록의 앞쪽 줄에 쓰인 값이 나중에 정해지는 경우는 없다(정의가 지배한다).

    // ---- 출력 ----
    let mut out = String::new();
    for l in &all[..=hdr] {
        out.push_str(l);
        out.push('\n');
    }
    for (b, blk) in blocks.iter().enumerate() {
        out.push_str(&blk.label);
        out.push_str(":\n");
        if visited[b] {
            for (k, name) in &phis[b] {
                let ty = &cands[promo[*k]].ty;
                let mut ents: Vec<String> = Vec::new();
                if let Some(v) = incoming[b].get(k) {
                    for (val, p) in v {
                        ents.push(format!("[ {}, %{} ]", subst(val, &map), blocks[*p].label));
                    }
                }
                for &p in &pred[b] {
                    if !visited[p] {
                        ents.push(format!("[ undef, %{} ]", blocks[p].label));
                    }
                }
                out.push_str(&format!("  %{} = phi {} {}\n", name, ty, ents.join(", ")));
            }
        }
        for l in &out_lines[b] {
            out.push_str(&subst(l, &map));
            out.push('\n');
        }
    }
    out.push_str("}\n");
    Some(out)
}
