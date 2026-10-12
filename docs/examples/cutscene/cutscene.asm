.natives event
.header SB2 main f_0280 globals 0
.program 100 f_0280
.code

.function f_0000 local 2 arg 1 types 1
    addr local 1
    push 1
    store
    pop
L_0068:
    nop 0
    load local 1
    jf L_00d4
    wait
    native _OBJS_CHECK
    load local 0
    addr local 1
    ext 3
    jmp L_0068
L_00d4:
    nop 1
    push 0
    ret

.function f_0104 local 4 arg 3 types 1 1 1
    native _SET_MES_AUTOSET
    push 0
    load local 0
    load local 1
    ext 4
    native _MES_MAKE
    push 0
    load local 2
    ext 3
    addr local 3
    push 0
    store
    pop
L_01d8:
    nop 0
    load local 3
    not
    jf L_0250
    wait
    native _CHECK_MES
    push 0
    addr local 3
    ext 3
    jmp L_01d8
L_0250:
    nop 1
    push 0
    ret

.function f_0280 local 1 arg 0
    native _INITIALIZE
    ext 1
    native _EOH_SYNC_CHARA
    push 0
    push 0
    ext 3
    native _OBJS_INIT
    push 0
    ext 2
    native _OBJS_SYNC_OBJ
    push 0
    push 0
    ext 3
    native _EOH_SYNC_CHARA
    push 1
    push 8
    ext 3
    native _OBJS_INIT
    push 1
    ext 2
    native _OBJS_SYNC_OBJ
    push 1
    push 1
    ext 3
    native _OBJS_SET_POS
    push 0
    push 0.0
    push 0.0
    push 0.0
    ext 5
    native _OBJS_SET_ROT
    push 0
    push 0.0
    push 0.0
    push 0.0
    ext 5
    native _OBJS_SET_POS
    push 1
    push 0.0
    push 0.0
    push 240.0
    ext 5
    native _OBJS_SET_ROT
    push 1
    push 0.0
    push 3.1415927
    push 0.0
    ext 5
    native _OBJS_SET_MOTION
    push 0
    push "立ち"
    push 4
    ext 4
    native _OBJS_SET_MOTION
    push 1
    push "歩き"
    push 4
    ext 4
    native _CMRS_SET_POS
    push 80.0
    push 45.0
    push 60.0
    neg
    ext 4
    native _CMRS_SET_REF
    push 0.0
    push 20.0
    push 80.0
    ext 4
    native _SET_FADE_IN
    push 30
    ext 2
    native _OBJS_MOVE
    push 1
    push 0.0
    push 0.0
    push 50.0
    push 120
    push 1
    ext 7
    native _CMRS_MOVE
    push 50.0
    push 35.0
    push 30.0
    neg
    push 0.0
    push 20.0
    push 25.0
    push 120
    ext 8
    push 1
    call f_0000
    pop
    native _OBJS_SET_MOTION
    push 1
    push "立ち"
    push 4
    ext 4
    push 8
    push 0
    push 200
    call f_0104
    pop
    native _OBJS_SET_MOTION
    push 0
    push "うなずく"
    push 4
    push 2
    or
    ext 4
    native _OBJS_NEXT_MOTION
    push 0
    push "立ち"
    push 4
    ext 4
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
    push 0
    push 8
    push 210
    call f_0104
    pop
    push 8
    push 0
    push 220
    call f_0104
    pop
    native _SET_FADE_OUT
    push 30
    ext 2
    addr local 0
    push 30
    store
    pop
L_0990:
    nop 0
    load local 0
    push 0
    cmp gt
    jf L_0a2c
    wait
    addr local 0
    load local 0
    push 1
    sub
    store
    pop
    jmp L_0990
L_0a2c:
    nop 1
    native _SET_FLAG
    push 1200
    push 1
    ext 3
    push 0
    ret
.string "立ち"
.string "歩き"
.string "立ち"
.string "うなずく"
.string "立ち"
.string "笑顔"
.string "まばたき"
.padding 0000000000
