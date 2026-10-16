"""Windows x64 PE(TAM이 만든 kernel32 전용 실행 파일)를 unicorn으로 실행해 종료 코드, 표준 출력, 힙 누수를 돌려준다.

사용법: pe_emu.py <exe> [stdin 텍스트]
kernel32의 Heap*, ExitProcess, GetStdHandle, ReadFile, WriteFile, CreateFileA, CloseHandle, GetLastError, GetTickCount64만
흉내 낸다(파일은 메모리 안의 가짜 파일 시스템). 진짜 Windows가 아니다.
"""
import sys, struct
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_64, UC_HOOK_CODE, UcError
from unicorn.x86_const import *

def run(path, stdin=b"", max_insn=200_000_000):
    pe = pefile.PE(path)
    base = pe.OPTIONAL_HEADER.ImageBase
    size = (pe.OPTIONAL_HEADER.SizeOfImage + 0xFFF) & ~0xFFF
    mu = Uc(UC_ARCH_X86, UC_MODE_64)
    mu.mem_map(base, size)
    mu.mem_write(base, pe.header[: pe.OPTIONAL_HEADER.SizeOfHeaders])
    for s in pe.sections:
        mu.mem_write(base + s.VirtualAddress, s.get_data())
    STUBS, HEAP, STACK = 0x7000_0000, 0x1_0000_0000, 0x6000_0000
    mu.mem_map(STUBS, 0x1000)
    mu.mem_map(HEAP, 0x1000_0000)
    mu.mem_map(STACK, 0x100000)
    stubs = {}
    for entry in pe.DIRECTORY_ENTRY_IMPORT:
        assert entry.dll.decode().lower() == "kernel32.dll"
        for i, imp in enumerate(entry.imports):
            addr = STUBS + 16 * len(stubs)
            stubs[addr] = imp.name.decode()
            mu.mem_write(imp.address - base + base, struct.pack("<Q", addr))
    state = {"brk": HEAP + 0x1000, "sizes": {}, "exit": None, "live": 0,
             "out": bytearray(), "stdin": bytearray(stdin), "fs": {}, "handles": {}, "next_h": 0x100, "err": 0}
    STDIN_H, STDOUT_H, INVALID = 0x10, 0x11, 0xFFFFFFFFFFFFFFFF
    def cstr(uc, addr):
        b = bytearray()
        while True:
            c = uc.mem_read(addr, 1)[0]
            if c == 0:
                return bytes(b)
            b.append(c)
            addr += 1
    def alloc(n):
        p = (state["brk"] + 15) & ~15
        state["brk"] = p + max(n, 1)
        state["sizes"][p] = n
        state["live"] += 1
        return p
    def hook(uc, addr, sz, _):
        if addr not in stubs:
            return
        name = stubs[addr]
        rcx, rdx, r8, r9 = (uc.reg_read(r) for r in (UC_X86_REG_RCX, UC_X86_REG_RDX, UC_X86_REG_R8, UC_X86_REG_R9))
        ret = 0
        if name == "ExitProcess":
            state["exit"] = rcx & 0xFFFFFFFF
            uc.emu_stop()
            return
        elif name == "GetProcessHeap":
            ret = 0xABCD0000
        elif name == "HeapAlloc":
            assert rcx == 0xABCD0000
            ret = alloc(r8)
        elif name == "HeapReAlloc":
            old = r8
            new = alloc(r9)
            state["live"] -= 1  # 옛 블록은 곧 해제된다
            n = min(state["sizes"].get(old, 0), r9)
            if old and n:
                uc.mem_write(new, bytes(uc.mem_read(old, n)))
            state["sizes"].pop(old, None)
            ret = new
        elif name == "HeapFree":
            assert r8 in state["sizes"], "free of unknown pointer %x" % r8
            state["sizes"].pop(r8)
            state["live"] -= 1
            ret = 1
        elif name == "GetStdHandle":
            ret = {0xFFFFFFF6 & 0xFFFFFFFF: STDIN_H, 0xFFFFFFF5 & 0xFFFFFFFF: STDOUT_H}.get(rcx & 0xFFFFFFFF, INVALID)
        elif name == "WriteFile":
            data = bytes(uc.mem_read(rdx, r8 & 0xFFFFFFFF))
            if rcx == STDOUT_H:
                state["out"] += data
            elif rcx in state["handles"] and state["handles"][rcx]["w"]:
                state["handles"][rcx]["data"] += data
            else:
                state["err"] = 6
                ret = 0
                uc.reg_write(UC_X86_REG_RAX, ret)
                return finish(uc)
            if r9:
                uc.mem_write(r9, struct.pack("<I", len(data)))
            ret = 1
        elif name == "ReadFile":
            n = r8 & 0xFFFFFFFF
            if rcx == STDIN_H:
                chunk = bytes(state["stdin"][:n]); del state["stdin"][:n]
            elif rcx in state["handles"] and not state["handles"][rcx]["w"]:
                h = state["handles"][rcx]
                chunk = h["data"][h["pos"]:h["pos"] + n]; h["pos"] += len(chunk)
            else:
                state["err"] = 6
                uc.reg_write(UC_X86_REG_RAX, 0)
                return finish(uc)
            uc.mem_write(rdx, chunk)
            if r9:
                uc.mem_write(r9, struct.pack("<I", len(chunk)))
            ret = 1
        elif name == "CreateFileA":
            rsp = uc.reg_read(UC_X86_REG_RSP)
            disp = struct.unpack("<Q", bytes(uc.mem_read(rsp + 0x28, 8)))[0] & 0xFFFFFFFF
            path = cstr(uc, rcx).decode()
            write = bool(rdx & 0x40000000)
            if disp == 3 and path not in state["fs"]:
                state["err"] = 2
                ret = INVALID
            else:
                if disp == 2:
                    state["fs"][path] = b""
                h = state["next_h"]; state["next_h"] += 8
                state["handles"][h] = {"path": path, "w": write, "pos": 0,
                                       "data": bytearray(b"") if write else state["fs"][path]}
                ret = h
        elif name == "CloseHandle":
            h = state["handles"].pop(rcx, None)
            if h is not None and h["w"]:
                state["fs"][h["path"]] = bytes(h["data"])
            ret = 1
        elif name == "GetLastError":
            ret = state["err"]
        elif name == "GetTickCount64":
            ret = 123456
        else:
            raise RuntimeError("unexpected import " + name)
        uc.reg_write(UC_X86_REG_RAX, ret)
        finish(uc)
    def finish(uc):
        rsp = uc.reg_read(UC_X86_REG_RSP)
        retaddr = struct.unpack("<Q", bytes(uc.mem_read(rsp, 8)))[0]
        uc.reg_write(UC_X86_REG_RSP, rsp + 8)
        uc.reg_write(UC_X86_REG_RIP, retaddr)
    mu.hook_add(UC_HOOK_CODE, hook, begin=STUBS, end=STUBS + 0x1000)
    rsp = STACK + 0x100000 - 0x1008  # 진입 시 rsp % 16 == 8
    mu.reg_write(UC_X86_REG_RSP, rsp)
    mu.mem_write(rsp, struct.pack("<Q", 0xDEAD0000))
    try:
        mu.emu_start(base + pe.OPTIONAL_HEADER.AddressOfEntryPoint, 0, count=max_insn)
    except UcError as e:
        return None, "emulation error: %s at rip=%x" % (e, mu.reg_read(UC_X86_REG_RIP)), state["live"], bytes(state["out"])
    return state["exit"], "", state["live"], bytes(state["out"])

if __name__ == "__main__":
    stdin = sys.argv[2].encode() if len(sys.argv) > 2 else b""
    code, err, live, out = run(sys.argv[1], stdin)
    print(f"exit={code} live_heap_blocks={live} stdout={out.hex()} {err}")
