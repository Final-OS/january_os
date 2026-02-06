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
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax
    
    ; Load CR3 (Page Table)
    ; Assuming CR3 < 4GB
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
    
    ; Load Argument (direct_map_base)
    mov rdi, [0x9000 - 32]

    ; Load Stack Pointer
    mov rsp, [0x9000 - 16]
    
    ; Load Entry Point
    mov rax, [0x9000 - 8]
    
    ; Jump to kernel
    call rax
    
    ; Should not reach here
spin:
    hlt
    jmp spin

; GDT for 32-bit transition
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
