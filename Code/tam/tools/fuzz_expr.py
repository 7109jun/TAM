#!/usr/bin/env python3
"""정수 식 차분 퍼저: 랜덤 식을 만들어 tamc로 컴파일/실행한 결과를 파이썬 참조 구현과 비교한다.
사용: tools/fuzz_expr.py [개수] [시드]   (tamc는 target/debug/tamc)"""
import random, subprocess, sys, os, tempfile

TYPES = {"i8": (8, True), "i16": (16, True), "i32": (32, True), "i64": (64, True),
         "u8": (8, False), "u16": (16, False), "u32": (32, False), "u64": (64, False)}

class Trap(Exception):
    pass

def rng_of(t):
    b, s = TYPES[t]
    return (-(1 << (b - 1)), (1 << (b - 1)) - 1) if s else (0, (1 << b) - 1)

def wrap(t, v):
    b, s = TYPES[t]
    v &= (1 << b) - 1
    if s and v >= 1 << (b - 1):
        v -= 1 << b
    return v

def chk(t, v):
    lo, hi = rng_of(t)
    if v < lo or v > hi:
        raise Trap()
    return v

def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) else -q

def gen(t, depth, env):
    """(소스, 평가 함수) 를 돌려준다. 평가 함수는 env를 받아 값 또는 Trap."""
    lo, hi = rng_of(t)
    if depth == 0 or random.random() < 0.15:
        c = random.random()
        vs = [v for v, vt in env if vt == t]
        if vs and c < 0.5:
            v = random.choice(vs)
            return v, (lambda e, v=v: e[v])
        if c < 0.75:
            n = random.choice([0, 1, 2, 3, 7, 100, hi, lo if lo < 0 else 5, hi - 1])
        else:
            n = random.randint(max(lo, -300), min(hi, 300))
        if n < 0:
            # 리터럴은 음수 단항으로 쓴다 (MIN은 `-MAX - 1`)
            if n == lo:
                s = f"(-{hi} - 1)"
            else:
                s = f"(-{-n})"
        else:
            s = str(n)
        return s, (lambda e, n=n: n)
    k = random.choice(["add", "sub", "mul", "div", "rem", "and", "or", "xor", "shl", "shr",
                       "wadd", "wsub", "wmul", "neg", "bnot", "if", "cast"])
    if k in ("neg",) and not TYPES[t][1]:
        k = "add"
    if k in ("add", "sub", "mul", "div", "rem", "and", "or", "xor", "wadd", "wsub", "wmul"):
        a, fa = gen(t, depth - 1, env)
        b, fb = gen(t, depth - 1, env)
        sym = {"add": "+", "sub": "-", "mul": "*", "div": "/", "rem": "%", "and": "&", "or": "|", "xor": "^"}
        if k.startswith("w"):
            fn = {"wadd": "wrapping_add", "wsub": "wrapping_sub", "wmul": "wrapping_mul"}[k]
            op = {"wadd": lambda x, y: x + y, "wsub": lambda x, y: x - y, "wmul": lambda x, y: x * y}[k]
            return f"{fn}({a}, {b})", (lambda e: wrap(t, op(fa(e), fb(e))))
        def f(e, k=k):
            x, y = fa(e), fb(e)
            if k == "add": return chk(t, x + y)
            if k == "sub": return chk(t, x - y)
            if k == "mul": return chk(t, x * y)
            if k in ("div", "rem"):
                if y == 0: raise Trap()
                if TYPES[t][1] and x == lo and y == -1: raise Trap()
                q = tdiv(x, y)
                return q if k == "div" else x - q * y
            if k == "and": return x & y
            if k == "or": return x | y
            if k == "xor": return x ^ y
        return f"({a} {sym[k]} {b})", f
    if k in ("shl", "shr"):
        a, fa = gen(t, depth - 1, env)
        n = random.randint(0, TYPES[t][0] + 3)
        bits = TYPES[t][0]
        def f(e, k=k):
            x = fa(e)
            sh = n % bits
            if k == "shl": return wrap(t, x << sh)
            return x >> sh  # 산술/논리 모두 파이썬 >>와 같다 (부호 있는 값은 음수 그대로)
        return f"({a} {'<<' if k == 'shl' else '>>'} {n})", f
    if k == "neg":
        a, fa = gen(t, depth - 1, env)
        return f"(-{a})", (lambda e: chk(t, -fa(e)))
    if k == "bnot":
        a, fa = gen(t, depth - 1, env)
        return f"(~{a})", (lambda e: wrap(t, ~fa(e)))
    if k == "if":
        ct = random.choice(list(TYPES))
        a, fa = gen(ct, depth - 1, env)
        b, fb = gen(ct, depth - 1, env)
        cmp_, py = random.choice([("<", lambda x, y: x < y), ("==", lambda x, y: x == y),
                                  (">=", lambda x, y: x >= y), ("!=", lambda x, y: x != y)])
        x, fx = gen(t, depth - 1, env)
        y, fy = gen(t, depth - 1, env)
        a, b = f"(z_{ct} + {a})", f"(z_{ct} + {b})"  # 리터럴끼리의 비교가 i32로 굳지 않게 한다
        return (f"(if {a} {cmp_} {b} {{ {x} }} else {{ {y} }})",
                lambda e: fx(e) if py(fa(e), fb(e)) else fy(e))
    # cast: 다른 정수형에서 `as`
    st = random.choice(list(TYPES))
    a, fa = gen(st, depth - 1, env)
    return f"((z_{st} + {a}) as {t})", (lambda e: wrap(t, fa(e)))

def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    random.seed(seed)
    here = os.path.dirname(os.path.abspath(__file__))
    tamc = os.path.join(here, "..", "target", "debug", "tamc")
    d = tempfile.mkdtemp(prefix="tamfuzz")
    bad = 0
    for i in range(n):
        t = random.choice(list(TYPES))
        nv = random.randint(0, 3)
        env, decls, vals = [], [f"    let z_{ty}: {ty} = 0" for ty in TYPES], {}
        for j in range(nv):
            vt = random.choice(list(TYPES))
            lo, hi = rng_of(vt)
            v = random.choice([lo, hi, 0, 1, random.randint(max(lo, -200), min(hi, 200))])
            name = f"v{j}"
            vals[name] = v
            env.append((name, vt))
            lit = f"(-{-v})" if v < 0 and v != lo else (f"(-{hi} - 1)" if v == lo and lo < 0 else str(v))
            decls.append(f"    let {name}: {vt} = {lit}")
        src_e, f = gen(t, 4, env)
        try:
            want = ("ok", f(vals))
        except Trap:
            want = ("trap",)
        # 결과를 8비트 종료 코드로 줄인다: (값 mod 256) 대신 해시처럼 접어서 비교한다.
        body = "\n".join(decls)
        prog = f"fn main() i32 {{\n{body}\n    let r: {t} = {src_e}\n    ((r as i64) & 255) as i32\n}}\n"
        p = os.path.join(d, f"f{i}.tam")
        open(p, "w").write(prog)
        r = subprocess.run([tamc, "run", p], capture_output=True, text=True)
        if want[0] == "trap":
            ok = r.returncode == 132
        else:
            ok = r.returncode == (want[1] & 255)
        if not ok:
            bad += 1
            print(f"MISMATCH #{i} want={want} got rc={r.returncode}\n{r.stderr[:300]}\n{prog}")
            if bad >= 5:
                break
    print(f"{n} cases, {bad} mismatches (seed {seed})")
    sys.exit(1 if bad else 0)

main()
