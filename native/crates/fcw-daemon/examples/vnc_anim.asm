; Загрузочный сектор с анимацией на весь экран: «гость» для проверки потока
; экрана на настоящем QEMU и его VNC без образа ОС (см. screen_bench --vnc).
;
;   nasm -f bin vnc_anim.asm -o anim.img
;   qemu-system-x86_64 -machine pc -accel tcg -m 64 -nodefaults -vga std \
;       -drive file=anim.img,format=raw,if=ide -display none -vnc unix:/tmp/anim-vnc.sock
;
; Переходит в защищённый режим, находит видеокарту Bochs/QEMU (1234:1111),
; включает режим W x H x 32 через порты VBE DISPI и в цикле заливает кадровый
; буфер плавно движущимся цветным узором (в каждом тайле много цветов, как у
; видео). Размер задаётся при сборке: nasm -DW=1280 -DH=720 …

%ifndef W
%define W 1024
%endif
%ifndef H
%define H 768
%endif

bits 16
org 0x7c00

start:
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7c00
    lgdt [gdtr]
    mov eax, cr0
    or al, 1
    mov cr0, eax
    jmp 0x08:pm

bits 32
pm:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x7c00

    ; видеокарта на шине 0: читаем идентификатор каждого устройства
    xor ebx, ebx
.scan:
    mov eax, ebx
    shl eax, 11
    or eax, 0x80000000
    mov dx, 0xcf8
    out dx, eax
    mov dx, 0xcfc
    in eax, dx
    cmp eax, 0x11111234
    je .found
    inc ebx
    cmp ebx, 32
    jb .scan
.halt:
    hlt
    jmp .halt
.found:
    mov eax, ebx
    shl eax, 11
    or eax, 0x80000010          ; BAR0: кадровый буфер
    mov dx, 0xcf8
    out dx, eax
    mov dx, 0xcfc
    in eax, dx
    and eax, 0xfffffff0
    mov ebp, eax

%macro dispi 2
    mov dx, 0x1ce
    mov ax, %1
    out dx, ax
    inc dx
    mov ax, %2
    out dx, ax
%endmacro
    dispi 4, 0                  ; выключить
    dispi 1, W
    dispi 2, H
    dispi 3, 32
    dispi 4, 0x41               ; включить, линейный буфер

    xor esi, esi                ; номер кадра
.frame:
    mov edi, ebp
    xor edx, edx                ; y
.row:
    xor ecx, ecx                ; x
.px:
    lea eax, [ecx + edx]
    shr eax, 1
    add eax, esi
    shl eax, 16                 ; R = (x + y) / 2 + t
    lea ebx, [edx + esi * 2]
    mov ah, bl                  ; G = y + 2t
    lea ebx, [ecx + esi * 4]
    mov al, bl                  ; B = x + 4t
    stosd
    inc ecx
    cmp ecx, W
    jb .px
    inc edx
    cmp edx, H
    jb .row
    inc esi
    jmp .frame

align 8
gdt:
    dq 0
    dq 0x00cf9a000000ffff       ; код, 4 ГиБ
    dq 0x00cf92000000ffff       ; данные, 4 ГиБ
gdtr:
    dw gdtr - gdt - 1
    dd gdt

times 510 - ($ - $$) db 0
dw 0xaa55
