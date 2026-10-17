; TAM BIOS 부트 섹터 — `nasm -f bin boot/bios.asm -o src/boot_bios.bin`
;
; 하는 일: 디스크 LBA 1부터 페이로드를 0x10000에 읽어 오고, A20을 켜고, 첫 1GiB를 항등 매핑하는
; 페이지 테이블을 만들어 롱 모드로 들어간 뒤 페이로드의 64비트 진입점을 부른다.
; tamc가 채우는 값: 오프셋 3의 페이로드 섹터 수(u16), 오프셋 5의 진입점 주소(u32).
BITS 16
ORG 0x7C00

start:
    jmp short entry
    nop
sectors:    dw 0
entry_addr: dd 0

entry:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    mov [boot_drive], dl
    sti

    ; 페이로드 읽기: 한 번에 64섹터(32KiB)씩, 목적지는 0x1000:0000부터 이어 붙인다.
    mov word [dap_lba], 1
    mov word [dap_seg], 0x1000
    mov cx, [sectors]
.next:
    test cx, cx
    jz .loaded
    mov ax, 64
    cmp cx, ax
    jae .count_ok
    mov ax, cx
.count_ok:
    mov [dap_count], ax
    mov dl, [boot_drive]
    mov si, dap
    mov ah, 0x42
    int 0x13
    jc disk_error
    mov ax, [dap_count]
    sub cx, ax
    add [dap_lba], ax
    shl ax, 5                  ; 섹터 수 * 512 / 16 = 문단 수
    add [dap_seg], ax
    jmp .next
.loaded:

    ; A20 (fast gate)
    in al, 0x92
    or al, 2
    out 0x92, al

    ; 페이지 테이블 0x1000(PML4) 0x2000(PDPT) 0x3000(PD)를 0으로 지운다.
    xor ax, ax
    mov es, ax
    mov di, 0x1000
    mov cx, 0xC00
    cld
    xor eax, eax
    rep stosd
    mov dword [0x1000], 0x2003
    mov dword [0x2000], 0x3003
    mov di, 0x3000
    mov eax, 0x83              ; present | writable | 2MiB page
    mov cx, 512
.pd:
    mov [di], eax
    mov dword [di + 4], 0
    add eax, 0x200000
    add di, 8
    loop .pd

    cli
    lgdt [gdt_desc]
    mov eax, cr4
    or eax, 0x620              ; PAE | OSFXSR | OSXMMEXCPT
    mov cr4, eax
    mov eax, 0x1000
    mov cr3, eax
    mov ecx, 0xC0000080        ; EFER
    rdmsr
    or eax, 0x100              ; LME
    wrmsr
    mov eax, cr0
    and eax, 0xFFFFFFFB        ; EM 끄기 (SSE 사용)
    or eax, 0x80000023         ; PG | NE | MP | PE
    mov cr0, eax
    jmp 0x08:long_mode

disk_error:
    mov ax, 0x0E45             ; 'E'
    xor bx, bx
    int 0x10
.hang:
    hlt
    jmp .hang

BITS 64
long_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax
    mov rsp, 0x9F000
    mov eax, [entry_addr]
    call rax
.halt:
    hlt
    jmp .halt

align 8
gdt:
    dq 0
    dq 0x00AF9A000000FFFF      ; 0x08: 64비트 코드
    dq 0x00CF92000000FFFF      ; 0x10: 데이터
gdt_end:
gdt_desc:
    dw gdt_end - gdt - 1
    dd gdt

boot_drive: db 0
dap:
    db 0x10, 0
dap_count: dw 0
dap_off:   dw 0
dap_seg:   dw 0
dap_lba:   dq 0

times 510 - ($ - $$) db 0
dw 0xAA55
