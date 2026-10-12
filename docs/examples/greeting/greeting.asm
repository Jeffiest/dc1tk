.natives event
.header SB2 main f_0244 globals 1
.program 100 f_0244
.program 1000 f_0554
.code

.function f_0000 local 1 arg 1 types 1
L_0038:
    nop 0
    load local 0
    push 0
    cmp gt
    jf L_00d4
    wait
    addr local 0
    load local 0
    push 1
    sub
    store
    pop
    jmp L_0038
L_00d4:
    nop 1
    push 0
    ret

.function f_0104 local 2 arg 1 types 1
    native _MES_MAKE
    push 0
    load local 0
    ext 3
    addr local 1
    push 0
    store
    pop
L_019c:
    nop 0
    load local 1
    not
    jf L_0214
    wait
    native _CHECK_MES
    push 0
    addr local 1
    ext 3
    jmp L_019c
L_0214:
    nop 1
    push 0
    ret

.function f_0244 local 0 arg 0
    native _INITIALIZE
    ext 1
    native _CMRS_SET_POS
    push 10.0
    push 20.0
    push 30.0
    ext 4
    native _CMRS_SET_REF
    push 0.0
    push 15.0
    push 0.0
    ext 4
    native _OBJS_SET_POS
    push 0
    push 0.0
    push 0.0
    push 0.0
    ext 5
    native _OBJS_SET_ROT
    push 0
    push 0.0
    push 3.1415927
    push 0.0
    ext 5
    native _OBJS_SET_POS
    push 1
    push 40.0
    push 0.0
    push 0.0
    ext 5
    native _OBJS_SET_ROT
    push 1
    push 0.0
    push 0.0
    push 0.0
    ext 5
    native _OBJS_TEX_ANIME
    push 0
    push 0
    ext 3
    native _OBJS_TEX_ANIME
    push 0
    push 1
    push "笑顔"
    ext 4
    native _OBJS_TEX_ANIME
    push 0
    push 1
    push "まばたき"
    ext 4
    native _OBJS_SET_MOTION
    push 1
    push "立ち"
    push 4
    ext 4
    push 0
    ret
.string "笑顔"
.string "まばたき"
.string "立ち"
.padding 00000000000000000000000000

.function f_0554 local 0 arg 0
    addr global 0
    load global 0
    push 1
    add
    store
    pop
    native _OBJS_SET_MOTION
    push 0
    push "手を振る"
    push 2
    push 4
    or
    ext 4
    push 30
    call f_0000
    pop
    load global 0
    push 1
    cmp eq
    jf L_06ac
    push 100
    call f_0104
    pop
    jmp L_06dc
L_06ac:
    nop 0
    push 110
    call f_0104
    pop
L_06dc:
    nop 1
    push 0
    ret
.string "手を振る"
.padding 82bd82ab0097a7
