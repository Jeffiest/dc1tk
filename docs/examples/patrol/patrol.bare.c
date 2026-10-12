#pragma natives monster

PROGRAM(100, program_100);
PROGRAM(200, program_200);
PROGRAM(500, program_500);
MAIN(program_100);

void func_1(void)
{
    int v0;

    v0 = 0;
    while (!v0) {
        wait;
        AT(180) _CHECK_MOS_END(&v0);
    }
}

void program_100(void)
{
    float v0;
    float v1;
    float v2;

    AT(70) _GET_POS(&v0, &v1, &v2);
    AT(70) _SET_PLACE_POS(v0, v1, v2);
    AT(110) _SET_DMG2("通常攻撃", 12.0, "攻撃", 0.4, 0.7, 1);
    AT(180) _SET_MOS("立ち", -1, 0);
}

void func_2(void)
{
    float v0;
    float v1;
    float v2;
    float v3;
    float v4;
    float v5;
    int v6;
    int v7;

    AT(70) _GET_PLACE_POS(&v0, &v1, &v2);
    AT(35) _GET_RNDF(200.0, &v3);
    AT(35) _GET_RNDF(200.0, &v4);
    AT(180) _SET_MOS("歩き", -1, 0);
    AT(70) _SET_NEXT_POS(v0 + v3 - 100.0, v1, v2 + v4 - 100.0, 1.5, 10.0);
    v6 = 180;
    v7 = 0;
    while (!v7 && v6 > 0) {
        wait;
        v6 = v6 - 1;
        AT(70) _CHK_MOVE_END(&v7);
        AT(70) _GET_TARGET_DIST(&v5);
        if (v5 < 150.0) {
            break;
        }
    }
    AT(70) _RESET_MOVE();
    AT(180) _SET_MOS("立ち", -1, 0);
}

void program_200(void)
{
    float v0;
    float v1;

    AT(70) _GET_TARGET_DIST(&v0);
    if (v0 < 40.0) {
        _RESET_MOTION();
        AT(180) _SET_MOS("攻撃", -1, 2);
        func_1();
        AT(180) _SET_MOS("立ち", -1, 0);
    } else if (v0 < 150.0) {
        AT(70) _GET_TARGET_ANGLE(&v1);
        AT(70) _SET_NEXT_ROT(v1, 0.1);
        wait;
    } else {
        func_2();
    }
}

void program_500(void)
{
    AT(70) _RESET_MOVE();
    AT(180) _SET_MOS("ダメージ", -1, 2);
    _BLOW_START(3.0, 0.2, 10);
    func_1();
    AT(180) _SET_MOS("立ち", -1, 0);
}
