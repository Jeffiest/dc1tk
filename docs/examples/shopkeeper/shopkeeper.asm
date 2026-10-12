.natives event
.header SB2 main f_0140 globals 1
.program 100 f_0140
.program 1000 f_04d0
.code

.function f_0000 local 2 arg 1 types 1
    native _MES_MAKE
    push 0
    load local 0
    ext 3
    addr local 1
    push 0
    store
    pop
L_0098:
    nop 0
    load local 1
    not
    jf L_0110
    wait
    native _CHECK_MES
    push 0
    addr local 1
    ext 3
    jmp L_0098
L_0110:
    nop 1
    push 0
    ret

.function f_0140 local 0 arg 0
    native _INITIALIZE
    ext 1
    native _EOH_SYNC_CHARA
    push 0
    push 8
    ext 3
    native _OBJS_INIT
    push 0
    ext 2
    native _OBJS_SYNC_OBJ
    push 0
    push 0
    ext 3
    native _OBJS_SET_POS
    push 0
    push 120.0
    push 0.0
    push 40.0
    neg
    ext 5
    native _OBJS_SET_ROT
    push 0
    push 0.0
    push 3.1415927
    push 0.0
    ext 5
    native _OBJS_SET_MOTION
    push 0
    push "立ち"
    push 4
    ext 4
    push 0
    ret
.string "立ち"
.padding 0000000000000000000000

.function f_0320 local 0 arg 0
    native _OBJS_SET_MOTION
    push 0
    push "お辞儀"
    push 4
    push 2
    or
    ext 4
    native _OBJS_NEXT_MOTION
    push 0
    push "立ち"
    push 4
    ext 4
    native _ADD_ITEM
    push 150
    ext 2
    native _SET_MES_ITEM
    push 0
    push 0
    push 150
    ext 4
    push 320
    call f_0000
    pop
    native _SET_FLAG
    push 1210
    push 1
    ext 3
    push 0
    ret
.string "お辞儀"
.string "立ち"
.padding 00000000

.function f_04d0 local 3 arg 0
    addr global 0
    load global 0
    push 1
    add
    store
    pop
    native _GET_FLAG
    push 1205
    addr local 0
    ext 3
    load local 0
    not
    jf L_05d4
    push 300
    call f_0000
    pop
    jmp L_0784
L_05d4:
    nop 0
    native _GET_CNT
    push 40
    addr local 1
    ext 3
    load local 1
    push 3
    cmp lt
    jf L_06b8
    native _SET_CNT
    push 40
    load local 1
    push 1
    add
    ext 3
    push 310
    call f_0000
    pop
    jmp L_0778
L_06b8:
    nop 2
    native _GET_FLAG
    push 1210
    addr local 2
    ext 3
    load local 2
    not
    jf L_073c
    call f_0320
    pop
    jmp L_076c
L_073c:
    nop 4
    push 330
    call f_0000
    pop
L_076c:
    nop 5
L_0778:
    nop 3
L_0784:
    nop 1
    load global 0
    push 3
    cmp ge
    jf L_07e4
    push 340
    call f_0000
    pop
L_07e4:
    nop 6
    push 0
    ret
