.natives monster
.header SB2 main f_011c globals 0
.program 100 f_011c
.program 200 f_081c
.program 500 f_0b04
.code

.function f_0000 local 1 arg 0
    addr local 0
    push 0
    store
    pop
L_0068:
    nop 0
    load local 0
    not
    jf L_00ec
    wait
    native _RESET_MOTION
    push 3
    add
    addr local 0
    ext 2
    jmp L_0068
L_00ec:
    nop 1
    push 0
    ret

.function f_011c local 3 arg 0
    native _GET_DIST
    push 5
    add
    addr local_f 0
    addr local_f 1
    addr local_f 2
    ext 4
    native _GET_DIST
    push 3
    add
    load local_f 0
    load local_f 1
    load local_f 2
    ext 4
    native _SET_OBJ
    push 3
    add
    push "通常攻撃"
    push 12.0
    push "攻撃"
    push 0.4
    push 0.7
    push 1
    ext 7
    native _RESET_MOTION
    push 2
    add
    push "立ち"
    push 1
    neg
    push 0
    ext 4
    push 0
    ret
.string "通常攻撃"
.string "攻撃"
.string "立ち"
.padding 00000000000000000000000000

.function f_0318 local 8 arg 0
    native _GET_DIST
    push 2
    add
    addr local_f 0
    addr local_f 1
    addr local_f 2
    ext 4
    native _GET_RND
    push 1
    add
    push 200.0
    addr local_f 3
    ext 3
    native _GET_RND
    push 1
    add
    push 200.0
    addr local_f 4
    ext 3
    native _RESET_MOTION
    push 2
    add
    push "歩き"
    push 1
    neg
    push 0
    ext 4
    native _GET_DIST
    push 10
    add
    load local_f 0
    load local_f 3
    add
    push 100.0
    sub
    load local_f 1
    load local_f 2
    load local_f 4
    add
    push 100.0
    sub
    push 1.5
    push 10.0
    ext 6
    addr local 6
    push 180
    store
    pop
    addr local 7
    push 0
    store
    pop
L_05c0:
    nop 0
    load local 7
    not
    jf_push L_0614
    load local 6
    push 0
    cmp gt
L_0614:
    nop 2
    jf L_074c
    wait
    addr local 6
    load local 6
    push 1
    sub
    store
    pop
    native _GET_DIST
    push 11
    add
    addr local 7
    ext 2
    native _GET_DIST
    push 14
    add
    addr local_f 5
    ext 2
    load local_f 5
    push 150.0
    cmp lt
    jf L_0734
    jmp L_074c
L_0734:
    nop 3
    jmp L_05c0
L_074c:
    nop 1
    native _GET_DIST
    push 12
    add
    ext 1
    native _RESET_MOTION
    push 2
    add
    push "立ち"
    push 1
    neg
    push 0
    ext 4
    push 0
    ret
.string "歩き"
.string "立ち"
.padding 558c820097a7

.function f_081c local 2 arg 0
    native _GET_DIST
    push 14
    add
    addr local_f 0
    ext 2
    load local_f 0
    push 40.0
    cmp lt
    jf L_09bc
    native _RESET_MOTION
    ext 1
    native _RESET_MOTION
    push 2
    add
    push "攻撃"
    push 1
    neg
    push 2
    ext 4
    call f_0000
    pop
    native _RESET_MOTION
    push 2
    add
    push "立ち"
    push 1
    neg
    push 0
    ext 4
    jmp L_0ac4
L_09bc:
    nop 0
    load local_f 0
    push 150.0
    cmp lt
    jf L_0a94
    native _GET_DIST
    push 15
    add
    addr local_f 1
    ext 2
    native _GET_DIST
    push 9
    add
    load local_f 1
    push 0.1
    ext 3
    wait
    jmp L_0ab8
L_0a94:
    nop 2
    call f_0318
    pop
L_0ab8:
    nop 3
L_0ac4:
    nop 1
    push 0
    ret
.string "攻撃"
.string "立ち"
.padding 558c820097a7

.function f_0b04 local 0 arg 0
    native _GET_DIST
    push 12
    add
    ext 1
    native _RESET_MOTION
    push 2
    add
    push "ダメージ"
    push 1
    neg
    push 2
    ext 4
    native _BLOW_START
    push 3.0
    push 0.2
    push 10
    ext 4
    call f_0000
    pop
    native _RESET_MOTION
    push 2
    add
    push "立ち"
    push 1
    neg
    push 0
    ext 4
    push 0
    ret
.string "ダメージ"
.string "立ち"
.padding 97a7
