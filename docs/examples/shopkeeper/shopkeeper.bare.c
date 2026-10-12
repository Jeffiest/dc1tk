#pragma natives event

int g0;

PROGRAM(100, program_100);
PROGRAM(1000, program_1000);
MAIN(program_100);

void func_1(int a0)
{
    int v1;

    _MES_MAKE(0, a0);
    v1 = 0;
    while (!v1) {
        wait;
        _CHECK_MES(0, &v1);
    }
}

void program_100(void)
{
    _INITIALIZE();
    _EOH_SYNC_CHARA(0, 8);
    _OBJS_INIT(0);
    _OBJS_SYNC_OBJ(0, 0);
    _OBJS_SET_POS(0, 120.0, 0.0, -40.0);
    _OBJS_SET_ROT(0, 0.0, 3.1415927, 0.0);
    _OBJS_SET_MOTION(0, "立ち", 4);
}

void func_2(void)
{
    _OBJS_SET_MOTION(0, "お辞儀", 4 | 2);
    _OBJS_NEXT_MOTION(0, "立ち", 4);
    _ADD_ITEM(150);
    _SET_MES_ITEM(0, 0, 150);
    func_1(320);
    _SET_FLAG(1210, 1);
}

void program_1000(void)
{
    int v0;
    int v1;
    int v2;

    g0 = g0 + 1;
    _GET_FLAG(1205, &v0);
    if (!v0) {
        func_1(300);
    } else {
        _GET_CNT(40, &v1);
        if (v1 < 3) {
            _SET_CNT(40, v1 + 1);
            func_1(310);
        } else {
            _GET_FLAG(1210, &v2);
            if (!v2) {
                func_2();
            } else {
                func_1(330);
            }
        }
    }
    if (g0 >= 3) {
        func_1(340);
    }
}
