/*
//#Config:relax
//#Arch:loongarch64
//#LinkArgs:-nostdlib -static --relax
//#Object:larch-call36-tail-relaxation-2.s
//#RunEnabled:false
//#ExpectSym:_start size=4

//#Config:no-relax
//#Arch:loongarch64
//#LinkArgs:-nostdlib -static --no-relax
//#Object:larch-call36-tail-relaxation-2.s
//#RunEnabled:false
//#ExpectSym:_start size=8
*/

.section .text, "ax", @progbits
.globl _start
.type _start, @function
_start:
    pcaddu18i $r12, %call36(callee)
    jirl      $r0, $r12, 0
.size _start, .-_start
