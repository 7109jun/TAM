#!/usr/bin/env python3
"""문장 수준 차분 퍼저: 변수/배열/if/for/while/break/continue/함수 호출이 섞인 랜덤 프로그램을
tamc로 실행한 결과를 파이썬 참조 구현과 비교한다. (SSA 승격, 루프, 오버플로 trap 검증)
사용: tools/fuzz_prog.py [개수] [시드]"""
import random, subprocess, sys, os, tempfile

class Trap(Exception): pass
class Break(Exception): pass
class Continue(Exception): pass

def w32(v):
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v >= 1 << 31 else v

def chk(v):
    if v < -(1 << 31) or v > (1 << 31) - 1:
        raise Trap()
    return v

def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) else -q

class Gen:
    def __init__(self):
        self.nl = 0          # 루프 변수 번호
        self.nw = 0

    # ---- 식: (소스, 평가함수) ----
    def expr(self, d, vars_, loops):
        r = random.random()
        if d == 0 or r < 0.2:
            c = random.random()
            if loops and c < 0.2:
                n = random.choice(loops)
                return n, (lambda e, n=n: e[n])
            if c < 0.65:
                n = random.choice(vars_)
                return n, (lambda e, n=n: e[n])
            k = random.choice([0, 1, 2, 3, 5, 7, 100, 1000, 65536, 2147483647])
            return str(k), (lambda e, k=k: k)
        k = random.choice(["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>", "arr", "call", "neg", "if"])
        a, fa = self.expr(d - 1, vars_, loops)
        if k in ("+", "-", "*"):
            b, fb = self.expr(d - 1, vars_, loops)
            def f(e, k=k):
                x, y = fa(e), fb(e)
                return chk(x + y if k == "+" else x - y if k == "-" else x * y)
            return f"({a} {k} {b})", f
        if k in ("/", "%"):
            b, fb = self.expr(d - 1, vars_, loops)
            def f(e, k=k):
                x, y = fa(e), fb(e | 0) if False else fb(e)
                y |= 1
                if x == -(1 << 31) and y == -1: raise Trap()
                q = tdiv(x, y)
                return q if k == "/" else x - q * y
            return f"({a} {k} ({b} | 1))", f
        if k in ("&", "|", "^"):
            b, fb = self.expr(d - 1, vars_, loops)
            def f(e, k=k):
                x, y = fa(e), fb(e)
                return x & y if k == "&" else x | y if k == "|" else x ^ y
            return f"({a} {k} {b})", f
        if k in ("<<", ">>"):
            n = random.randint(0, 31)
            def f(e, k=k):
                x = fa(e)
                return w32(x << n) if k == "<<" else x >> n
            return f"({a} {k} {n})", f
        if k == "arr":
            def f(e):
                return e["a"][fa(e) & 7]
            return f"a[({a} & 7) as u64]", f
        if k == "call":
            b, fb = self.expr(d - 1, vars_, loops)
            def f(e):
                x, y = fa(e), fb(e)
                # fn mix(x: i32, y: i32) i32 { if x < y { y - x } else { wrapping_add(x, y) } }
                return chk(y - x) if x < y else w32(x + y)
            return f"mix({a}, {b})", f
        if k == "neg":
            return f"(0 - {a})", (lambda e: chk(0 - fa(e)))
        b, fb = self.expr(d - 1, vars_, loops)
        c, fc = self.expr(d - 1, vars_, loops)
        return (f"(if {a} < {b} {{ {c} }} else {{ {a} }})",
                lambda e: fc(e) if fa(e) < fb(e) else fa(e))

    def cond(self, d, vars_, loops):
        a, fa = self.expr(d, vars_, loops)
        b, fb = self.expr(d, vars_, loops)
        op, py = random.choice([("<", lambda x, y: x < y), ("==", lambda x, y: x == y), (">=", lambda x, y: x >= y), ("!=", lambda x, y: x != y)])
        return f"{a} {op} {b}", (lambda e: py(fa(e), fb(e)))

    # ---- 문장: (소스 줄들, 실행함수) ----
    def block(self, d, vars_, loops, in_loop, ind):
        out, fs = [], []
        for _ in range(random.randint(1, 4)):
            lines, f = self.stmt(d, vars_, loops, in_loop, ind)
            out += lines
            fs.append(f)
        def run(e):
            for f in fs:
                f(e)
        return out, run

    def stmt(self, d, vars_, loops, in_loop, ind):
        pad = "    " * ind
        r = random.random()
        if r < 0.25 or d == 0:
            v = random.choice(vars_)
            x, fx = self.expr(2, vars_, loops)
            def f(e): e[v] = fx(e)
            return [f"{pad}{v} = {x}"], f
        if r < 0.4:
            v = random.choice(vars_)
            op = random.choice(["+=", "-=", "*=", "&=", "|=", "^="])
            x, fx = self.expr(2, vars_, loops)
            def f(e, op=op, v=v):
                y = fx(e)
                a = e[v]
                e[v] = chk(a + y) if op == "+=" else chk(a - y) if op == "-=" else chk(a * y) if op == "*=" else a & y if op == "&=" else a | y if op == "|=" else a ^ y
            return [f"{pad}{v} {op} {x}"], f
        if r < 0.5:
            i, fi = self.expr(1, vars_, loops)
            x, fx = self.expr(2, vars_, loops)
            def f(e):
                idx = fi(e) & 7
                e["a"][idx] = fx(e)
            return [f"{pad}a[({i} & 7) as u64] = {x}"], f
        if r < 0.65:
            c, fc = self.cond(1, vars_, loops)
            t, ft = self.block(d - 1, vars_, loops, in_loop, ind + 1)
            if random.random() < 0.5:
                el, fe = self.block(d - 1, vars_, loops, in_loop, ind + 1)
                lines = [f"{pad}if {c} {{"] + t + [f"{pad}}} else {{"] + el + [f"{pad}}}"]
                def f(e):
                    (ft if fc(e) else fe)(e)
                return lines, f
            def f(e):
                if fc(e): ft(e)
            return [f"{pad}if {c} {{"] + t + [f"{pad}}}"], f
        if r < 0.8:
            self.nl += 1
            iv = f"i{self.nl}"
            k = random.randint(0, 5)
            body, fb = self.block(d - 1, vars_, loops + [iv], True, ind + 1)
            def f(e):
                for n in range(k):
                    e[iv] = n
                    try:
                        fb(e)
                    except Continue:
                        pass
                    except Break:
                        break
            return [f"{pad}for {iv} in 0..{k} {{"] + body + [f"{pad}}}"], f
        if r < 0.9:
            self.nw += 1
            wv = f"w{self.nw}"
            k = random.randint(0, 5)
            body, fb = self.block(d - 1, vars_, loops, True, ind + 1)
            def f(e):
                n = 0
                while n < k:
                    n += 1
                    try:
                        fb(e)
                    except Continue:
                        pass
                    except Break:
                        break
            return [f"{pad}mut {wv} = 0", f"{pad}while {wv} < {k} {{", f"{pad}    {wv} += 1"] + body + [f"{pad}}}"], f
        if in_loop:
            c, fc = self.cond(1, vars_, loops)
            brk = random.random() < 0.5
            def f(e):
                if fc(e):
                    raise Break() if brk else Continue()
            return [f"{pad}if {c} {{ {'break' if brk else 'continue'} }}"], f
        v = random.choice(vars_)
        x, fx = self.expr(1, vars_, loops)
        def f(e): e[v] = fx(e)
        return [f"{pad}{v} = {x}"], f

def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    random.seed(seed)
    tamc = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "tamc")
    d = tempfile.mkdtemp(prefix="tamprog")
    bad = 0
    traps = 0
    for t in range(n):
        g = Gen()
        vars_ = ["x", "y", "z"]
        init = {v: random.choice([0, 1, -1, 7, 1000, 2147483647, -2147483648]) for v in vars_}
        e = dict(init)
        e["a"] = [random.randint(-5, 5) for _ in range(8)]
        a0 = list(e["a"])
        lines, f = g.block(3, vars_, [], False, 1)
        try:
            f(e)
            fin = w32(w32(w32(e["x"] + e["y"]) + e["z"]) + sum(e["a"]))
            want = fin & 255
        except Trap:
            want = 132; traps += 1
        lit = lambda v: f"(0 - {-v})" if v < 0 and v != -2147483648 else ("(0 - 2147483647 - 1)" if v == -2147483648 else str(v))
        src = ("fn mix(x: i32, y: i32) i32 { if x < y { y - x } else { wrapping_add(x, y) } }\n"
               "fn main() i32 {\n"
               + "".join(f"    mut {v}: i32 = {lit(init[v])}\n" for v in vars_)
               + f"    mut a: [i32; 8] = [{', '.join(lit(v) for v in a0)}]\n"
               + "\n".join(lines) + "\n"
               + "    let s = wrapping_add(wrapping_add(wrapping_add(x, y), z), wrapping_add(wrapping_add(wrapping_add(a[0], a[1]), wrapping_add(a[2], a[3])), wrapping_add(wrapping_add(a[4], a[5]), wrapping_add(a[6], a[7]))))\n"
               + "    (s & 255) as i32\n}\n")
        p = os.path.join(d, f"p{t}.tam")
        open(p, "w").write(src)
        r = subprocess.run([tamc, "run", p], capture_output=True, text=True, timeout=60)
        if r.returncode != want:
            bad += 1
            print(f"MISMATCH #{t}: want {want} got {r.returncode}\n{r.stderr[:400]}\n{src}")
            if bad >= 3: break
    print(f"{n} programs, {bad} mismatches, {traps} expected traps (seed {seed})")
    sys.exit(1 if bad else 0)

main()
