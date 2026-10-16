#!/usr/bin/env python3
"""컴파일러 견고성 퍼저: 예제 소스를 무작위로 망가뜨려도 tamc가 패닉/크래시 없이 깔끔한 오류(종료 코드 1)로
끝나는지 본다. 사용: tools/fuzz_crash.py [개수] [시드]"""
import random, subprocess, sys, os, glob, tempfile

def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    random.seed(int(sys.argv[2]) if len(sys.argv) > 2 else 1)
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
    tamc = os.path.join(root, "target", "debug", "tamc")
    srcs = [open(p).read() for p in glob.glob(os.path.join(root, "examples", "*.tam"))]
    srcs += [open(p).read() for p in glob.glob(os.path.join(root, "examples", "firmware", "*.tam"))]
    d = tempfile.mkdtemp(prefix="tamcrash")
    toks = ["{", "}", "(", ")", "[", "]", ";", ",", "=", "==", "+=", "..", "..=", "&", "&mut", "*", "fn", "let", "mut",
            "if", "else", "while", "for", "in", "break", "continue", "return", "match", "Some", "None", "Ok", "Err", "?",
            "'a'", "1e3", "0x", "0b2", "\"", "'", "<", ">", ">>=", "<<=", "i32", "u8", "String", "str", "0", "-1", "\n", "unsafe", "impl", "struct", "use"]
    bad = 0
    valid = 0
    for t in range(n):
        s = random.choice(srcs)
        for _ in range(random.randint(1, 4)):
            r = random.random()
            i = random.randint(0, max(0, len(s) - 1))
            if r < 0.35:
                j = min(len(s), i + random.randint(1, 8))
                s = s[:i] + s[j:]
            elif r < 0.7:
                s = s[:i] + " " + random.choice(toks) + " " + s[i:]
            elif r < 0.85:
                s = s[:i] + random.choice(toks) + s[i + 1:]
            else:
                lines = s.split("\n")
                if len(lines) > 1:
                    k = random.randrange(len(lines)); lines.insert(random.randrange(len(lines)), lines[k]); s = "\n".join(lines)
        p = os.path.join(d, f"c{t}.tam")
        open(p, "w").write(s)
        try:
            r = subprocess.run([tamc, "check", p], capture_output=True, text=True, timeout=20)
        except subprocess.TimeoutExpired:
            bad += 1; print(f"TIMEOUT #{t}\n{s}\n"); continue
        if r.returncode not in (0, 1) or "panicked" in r.stderr:
            bad += 1
            print(f"CRASH #{t}: rc={r.returncode}\n{r.stderr[:400]}\n--- source ---\n{s}\n")
            if bad >= 3: break
            continue
        if r.returncode == 0:
            # 검사를 통과한 변형은 IR까지 만들어 LLVM 검증기로 확인한다 (코드 생성/최적화 패스 버그 탐지).
            e = subprocess.run([tamc, "emit-llvm", p], capture_output=True, text=True, timeout=20)
            v = subprocess.run(["opt", "-passes=verify", "-disable-output", p[:-4] + ".ll"], capture_output=True, text=True)
            if e.returncode != 0 or v.returncode != 0:
                bad += 1
                print(f"BAD IR #{t}: emit rc={e.returncode}\n{e.stderr[:300]}\n{v.stderr[:600]}\n--- source ---\n{s}\n")
                if bad >= 3: break
            else:
                valid += 1
    print(f"{n} mutants, {bad} crashes, {valid} compiled+verified")
    sys.exit(1 if bad else 0)

main()
