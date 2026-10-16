#!/usr/bin/env python3
"""힙/소유권 퍼저: String, 구조체 안의 String, Option<String>, Result<String, i32>, 조기 return/`?`,
break/continue가 섞인 소유권상 올바른 프로그램을 만들어 (1) 컴파일되는지, (2) valgrind 아래에서
누수·이중 해제 없이 끝나는지, (3) 파이썬 참조 구현과 같은 종료 코드를 내는지 확인한다.
사용: tools/fuzz_heap.py [개수] [시드]"""
import random, subprocess, sys, os, tempfile

class Ret(Exception):
    def __init__(self, v, err=False):
        self.v, self.err = v, err
class Brk(Exception): pass
class Cont(Exception): pass

PRELUDE = """struct W { s: String, n: i32 }
fn consume(s: String) i32 { s.len as i32 }
fn cw(w: W) i32 { w.s.len as i32 + w.n }
fn mkres(k: i32) Result<String, i32> {
    if k % 3 == 0 { return Err(k) }
    mut s = string_from("")
    mut i = 0
    while i < k { string_push(&mut s, 66); i += 1 }
    Ok(s)
}
"""

class Gen:
    def __init__(self, rnd):
        self.r = rnd
        self.n = 0
        self.size = 0

    def fresh(self, p):
        self.n += 1
        return f"{p}{self.n}"

    # 블록 하나: (코드 줄들, 연산 목록)을 만든다. env: 살아 있는 변수 {이름: ('s'|'w', mut?)}
    def block(self, depth, in_loop, in_res, outer):
        ops = []
        live = {}  # 이 블록이 소유한 변수
        for _ in range(self.r.randint(2, 7)):
            self.size += 1
            if self.size > 60:
                break
            ops.append(self.stmt(depth, in_loop, in_res, live, outer))
        return ops

    def stmt(self, depth, in_loop, in_res, live, outer):
        r = self.r
        strs = [k for k, v in live.items() if v == 's']
        ws = [k for k, v in live.items() if v == 'w']
        all_s = strs + [k for k, v in outer.items() if v == 's']
        choices = ['new', 'new', 'res', 'acc']
        if strs:
            choices += ['push', 'move', 'consume', 'wrap', 'opt', 'acc', 'push']
        if ws:
            choices += ['accw', 'consumew']
        if depth < 3:
            choices += ['if', 'while']
        if in_loop:
            choices += ['brk', 'cont']
        choices += ['ret']
        if in_res:
            choices += ['try', 'try']
        c = r.choice(choices)
        if c == 'new':
            nm = self.fresh('s'); live[nm] = 's'
            return ('new', nm, r.randint(0, 5))
        if c == 'push':
            return ('push', r.choice(strs))
        if c == 'acc':
            if not all_s:
                return ('accc', r.randint(0, 9))
            return ('acc', r.choice(all_s))
        if c == 'move':
            s = r.choice(strs); del live[s]
            nm = self.fresh('s'); live[nm] = 's'
            return ('move', nm, s)
        if c == 'consume':
            s = r.choice(strs); del live[s]
            return ('consume', s)
        if c == 'wrap':
            s = r.choice(strs); del live[s]
            nm = self.fresh('w'); live[nm] = 'w'
            return ('wrap', nm, s, r.randint(0, 9))
        if c == 'accw':
            return ('accw', r.choice(ws))
        if c == 'consumew':
            w = r.choice(ws); del live[w]
            return ('consumew', w)
        if c == 'opt':
            s = r.choice(strs); del live[s]
            return ('opt', self.fresh('o'), s, self.fresh('x'), r.randint(0, 1))
        if c == 'res':
            return ('res', self.fresh('r'), self.fresh('x'), r.randint(0, 7))
        if c == 'try':
            return ('try', self.fresh('x'), r.randint(0, 7))
        if c == 'ret':
            return ('ret', r.choice([0, 0, 1]))
        if c == 'brk':
            return ('brk', r.randint(0, 1))
        if c == 'cont':
            return ('cont', r.randint(0, 1))
        merged = dict(outer); merged.update(live)
        if c == 'if':
            cond = r.randint(0, 3)
            a = self.block(depth + 1, in_loop, in_res, merged)
            b = self.block(depth + 1, in_loop, in_res, merged)
            return ('if', cond, a, b)
        if c == 'while':
            n = r.randint(0, 4)
            body = self.block(depth + 1, True, in_res, merged)
            return ('while', self.fresh('i'), n, body)

# ---------- 코드 생성 ----------
def emit(ops, ind, in_res, out):
    p = '    ' * ind
    for op in ops:
        k = op[0]
        if k == 'new':
            out.append(f'{p}mut {op[1]} = string_from("{"a" * op[2]}")')
        elif k == 'push':
            out.append(f'{p}string_push(&mut {op[1]}, 67)')
        elif k == 'acc':
            out.append(f'{p}acc += {op[1]}.len as i32')
        elif k == 'accc':
            out.append(f'{p}acc += {op[1]}')
        elif k == 'move':
            out.append(f'{p}mut {op[1]} = {op[2]}')
        elif k == 'consume':
            out.append(f'{p}acc += consume({op[1]})')
        elif k == 'wrap':
            out.append(f'{p}let {op[1]} = W {{ s: {op[2]}, n: {op[3]} }}')
        elif k == 'accw':
            out.append(f'{p}acc += {op[1]}.s.len as i32 + {op[1]}.n')
        elif k == 'consumew':
            out.append(f'{p}acc += cw({op[1]})')
        elif k == 'opt':
            _, o, s, x, none = op
            if none:
                out.append(f'{p}let {o}: Option<String> = None')
            else:
                out.append(f'{p}let {o} = Some({s})')
            out.append(f'{p}match {o} {{')
            out.append(f'{p}    Some({x}) => {{ acc += {x}.len as i32 }}')
            out.append(f'{p}    None => {{ acc += 100 }}')
            out.append(f'{p}}}')
        elif k == 'res':
            _, rv, x, n = op
            out.append(f'{p}match mkres({n}) {{')
            out.append(f'{p}    Ok({x}) => {{ acc += {x}.len as i32 }}')
            out.append(f'{p}    Err({x}) => {{ acc += {x} * 10 }}')
            out.append(f'{p}}}')
        elif k == 'try':
            _, x, n = op
            out.append(f'{p}let {x} = mkres({n})?')
            out.append(f'{p}acc += {x}.len as i32')
        elif k == 'ret':
            if op[1]:
                out.append(f'{p}if acc > 3 {{ return {"Ok(acc)" if in_res else "acc"} }}')
            else:
                out.append(f'{p}if acc % 7 == 3 {{ return {"Ok(acc)" if in_res else "acc"} }}')
        elif k == 'brk':
            out.append(f'{p}if acc % 5 == {op[1]} {{ break }}')
        elif k == 'cont':
            out.append(f'{p}if acc % 4 == {op[1]} {{ continue }}')
        elif k == 'if':
            _, cond, a, b = op
            out.append(f'{p}if acc % 4 == {cond} {{')
            emit(a, ind + 1, in_res, out)
            out.append(f'{p}}} else {{')
            emit(b, ind + 1, in_res, out)
            out.append(f'{p}}}')
        elif k == 'while':
            _, iv, n, body = op
            out.append(f'{p}mut {iv} = 0')
            out.append(f'{p}while {iv} < {n} {{')
            out.append(f'{p}    {iv} += 1')
            emit(body, ind + 1, in_res, out)
            out.append(f'{p}}}')

# ---------- 참조 실행 ----------
def w32(v):
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v >= 1 << 31 else v

def run(ops, st, env):
    """st: {'acc': int}. env: 이 블록 변수들의 길이/값."""
    for op in ops:
        k = op[0]
        if k == 'new':
            env[op[1]] = op[2]
        elif k == 'push':
            env[op[1]] += 1
        elif k == 'acc':
            st['acc'] += lookup(env, op[1])
        elif k == 'accc':
            st['acc'] += op[1]
        elif k == 'move':
            env[op[1]] = env.pop(op[2])
        elif k == 'consume':
            st['acc'] += env.pop(op[1])
        elif k == 'wrap':
            env[op[1]] = (env.pop(op[2]), op[3])
        elif k == 'accw':
            a, n = env[op[1]]; st['acc'] += a + n
        elif k == 'consumew':
            a, n = env.pop(op[1]); st['acc'] += a + n
        elif k == 'opt':
            _, o, s, x, none = op
            if none:
                env.pop(s) if False else None
                # None 분기: s는 이동되지 않고 남는다(소스에서는 s를 쓰지 않으므로 블록 끝에서 drop)
                st['acc'] += 100
            else:
                st['acc'] += env.pop(s)
        elif k == 'res':
            n = op[3]
            st['acc'] += (n * 10) if n % 3 == 0 else n
        elif k == 'try':
            n = op[2]
            if n % 3 == 0:
                raise Ret(n, True)
            st['acc'] += n
        elif k == 'ret':
            if (op[1] and st['acc'] > 3) or (not op[1] and st['acc'] % 7 == 3):
                raise Ret(st['acc'])
        elif k == 'brk':
            if st['acc'] % 5 == op[1]:
                raise Brk()
        elif k == 'cont':
            if st['acc'] % 4 == op[1]:
                raise Cont()
        elif k == 'if':
            _, cond, a, b = op
            run(a if st['acc'] % 4 == cond else b, st, {'__up': env})
        elif k == 'while':
            _, iv, n, body = op
            i = 0
            while i < n:
                i += 1
                try:
                    run(body, st, {'__up': env})
                except Brk:
                    break
                except Cont:
                    continue
        # 오버플로 trap은 길이가 작아 일어나지 않는다.

def lookup(env, name):
    while env is not None:
        if name in env:
            return env[name]
        env = env.get('__up')
    raise KeyError(name)

def main():
    win = "--win" in sys.argv   # Windows 대상으로 빌드해 에뮬레이터에서 실행(누수는 살아 있는 힙 블록 수로 본다)
    argv = [a for a in sys.argv if a != "--win"]
    count = int(argv[1]) if len(argv) > 1 else 200
    seed = int(argv[2]) if len(argv) > 2 else 1
    tamc = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "tamc")
    have_vg = subprocess.run(["valgrind", "--version"], capture_output=True).returncode == 0
    bad = 0
    d = tempfile.mkdtemp(prefix="fuzzheap")
    for i in range(count):
        rnd = random.Random(seed * 100003 + i)
        g = Gen(rnd)
        fa = g.block(0, False, False, {})
        g.size = 0
        fb = g.block(0, False, True, {})
        lines = [PRELUDE.rstrip('\n'), 'fn f() i32 {', '    mut acc = 0']
        emit(fa, 1, False, lines)
        lines += ['    acc', '}', 'fn g() Result<i32, i32> {', '    mut acc = 0']
        emit(fb, 1, True, lines)
        lines += ['    Ok(acc)', '}', 'fn main() i32 {',
                  '    let a = f()',
                  '    let b = match g() { Ok(v) => v, Err(e) => 1000 + e }',
                  '    (a * 3 + b) % 251', '}']
        src = '\n'.join(lines) + '\n'
        # 기대값
        st = {'acc': 0}
        try:
            run(fa, st, {})
            a = st['acc']
        except Ret as r:
            a = r.v
        st = {'acc': 0}
        try:
            run(fb, st, {})
            b = st['acc']
        except Ret as r:
            b = 1000 + r.v if r.err else r.v
        want = (a * 3 + b) % 251
        p = os.path.join(d, "p.tam"); exe = os.path.join(d, "p")
        open(p, "w").write(src)
        c = subprocess.run([tamc, "build", p, "-o", exe] + (["--target", "windows"] if win else []), capture_output=True, text=True)
        if c.returncode != 0:
            print(f"--- #{i}: COMPILE ERROR\n{c.stderr}\n{src}")
            bad += 1
            continue
        if win:
            emu = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pe_emu.py")
            r = subprocess.run(["python3", "-I", emu, exe], capture_output=True, text=True, timeout=300)
            words = dict(w.split("=", 1) for w in r.stdout.split() if "=" in w)
            code, live = int(words.get("exit", -1)), int(words.get("live_heap_blocks", -1))
            if code != want or live != 0:
                print(f"--- #{i}: windows exit {code}, expected {want}, live heap blocks {live}\n{r.stdout[:300]}\n{src}")
                bad += 1
                if bad >= 3:
                    break
            continue
        cmd = ["valgrind", "--leak-check=full", "--errors-for-leak-kinds=all", "--error-exitcode=99", "-q", exe] if have_vg else [exe]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        if r.returncode != want:
            print(f"--- #{i}: exit {r.returncode}, expected {want}\n{r.stderr[:600]}\n{src}")
            bad += 1
            if bad >= 3:
                break
    print(f"{count} programs, {bad} problems (seed {seed})")
    sys.exit(1 if bad else 0)

main()
