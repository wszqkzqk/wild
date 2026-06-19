/*
//#Config:relax
//#Arch:loongarch64
//#LinkArgs:-nostdlib -static --relax
//#Object:larch-call36-relaxation-2.s
//#RunEnabled:false
//#ExpectSym:_start size=16

//#Config:no-relax
//#Arch:loongarch64
//#LinkArgs:-nostdlib -static --no-relax
//#Object:larch-call36-relaxation-2.s
//#RunEnabled:false
//#ExpectSym:_start size=20
*/

.section .text, "ax", @progbits
.globl _start
.type _start, @function
_start:
    pcaddu18i $r12, %call36(callee)
    jirl      $r1, $r12, 0
    addi.d    $r4, $r0, 0
    addi.d    $r11, $r0, 93
    syscall   0
.size _start, .-_start
