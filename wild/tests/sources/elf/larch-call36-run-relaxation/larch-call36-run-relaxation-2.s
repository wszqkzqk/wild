.section .text, "ax", @progbits
.globl callee
.type callee, @function
callee:
    addi.d $r4, $r0, 42
    jirl   $r0, $r1, 0
.size callee, .-callee
