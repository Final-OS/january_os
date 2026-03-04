[BITS 16]
[ORG 0x8000]

start:
    cli

    ; Set segments to 0
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; Load GDT
    lgdt [gdt_desc]

    ; Enable PE (Protected Mode Enable)
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    ; Jump to 32-bit code
    jmp 0x8:protected_mode

[BITS 32]
protected_mode:
    ; Set data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    ; Enable PAE (Physical Address Extension) - Bit 5 of CR4
    ; If BSP runs with 5-level paging, mirror CR4.LA57 on AP before enabling PG.
    mov eax, cr4
    or eax, 1 << 5
    mov dl, [0x9000 - 33]
    test dl, dl
    jz .no_la57
    or eax, 1 << 12
.no_la57:
    mov cr4, eax

    ; Load CR3 (Page Table)
    mov eax, [0x9000 - 24]
    mov cr3, eax

    ; Enable Long Mode (LME) - Bit 8 of EFER (MSR 0xC0000080)
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    ; Enable Paging (PG) - Bit 31 of CR0
    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax

    ; Jump to 64-bit code
    jmp 0x18:long_mode

[BITS 64]
long_mode:
    ; Set data segments to 0
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov rbx, 0x9000

    ; Load kernel GDT (from BSP) - 10 bytes at [rbx - 64]
    lea rcx, [rbx - 64]
    lgdt [rcx]

    ; Load kernel IDT (from BSP) - 10 bytes at [rbx - 48]
    lea rcx, [rbx - 48]
    lidt [rcx]

    ; Enable SSE (kernel code may use SSE instructions)
    mov rcx, cr0
    and cx, 0xFFFB      ; Clear CR0.EM (bit 2)
    or cx, 0x2           ; Set CR0.MP (bit 1)
    mov cr0, rcx

    mov rcx, cr4
    or cx, 3 << 9        ; Set CR4.OSFXSR (bit 9) and CR4.OSXMMEXCPT (bit 10)
    mov cr4, rcx

    ; Load arguments
    mov rdi, [rbx - 32]  ; ARG (direct_map_base)
    mov rsp, [rbx - 16]  ; RSP (stack top)

    ; Load entry point
    mov rax, [rbx - 8]   ; ENTRY (ap_entry)

    ; Jump to kernel
    jmp rax

    ; Should not reach here
spin:
    hlt
    jmp spin

; Trampoline GDT (only used for 16->32->64 transition)
align 4
gdt_start:
    dq 0x0000000000000000 ; Null
    dq 0x00cf9a000000ffff ; Code 32
    dq 0x00cf92000000ffff ; Data 32
    dq 0x0020980000000000 ; Code 64 (Long Mode)
gdt_end:

gdt_desc:
    dw gdt_end - gdt_start - 1
    dd gdt_start
