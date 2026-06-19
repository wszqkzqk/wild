/*
//#Config:relax
//#Arch:loongarch64
//#LinkArgs:-nostdlib -static --relax
//#Object:larch-call36-run-relaxation-2.s
//#RunEnabled:true

//#Config:no-relax
//#Arch:loongarch64
//#LinkArgs:-nostdlib -static --no-relax
//#Object:larch-call36-run-relaxation-2.s
//#RunEnabled:true
*/

.section .text, "ax", @progbits
.globl _start
.type _start, @function
_start:
    pcaddu18i $r12, %call36(callee)
    jirl      $r1, $r12, 0
    addi.d    $r11, $r0, 93
    syscall   0
.size _start, .-_start
