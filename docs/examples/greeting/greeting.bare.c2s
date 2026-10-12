#pragma natives event

int g0;

PROGRAM(100, program_100);
PROGRAM(1000, program_1000);
MAIN(program_100);

void func_1(int a0)
{
    while (a0 > 0) {
        wait;
        a0 = a0 - 1;
    }
}

void func_2(int a0)
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
    _CMRS_SET_POS(10.0, 20.0, 30.0);
    _CMRS_SET_REF(0.0, 15.0, 0.0);
    _OBJS_SET_POS(0, 0.0, 0.0, 0.0);
    _OBJS_SET_ROT(0, 0.0, 3.1415927, 0.0);
    _OBJS_SET_POS(1, 40.0, 0.0, 0.0);
    _OBJS_SET_ROT(1, 0.0, 0.0, 0.0);
    _OBJS_TEX_ANIME(0, 0);
    _OBJS_TEX_ANIME(0, 1, "笑顔");
    _OBJS_TEX_ANIME(0, 1, "まばたき");
    _OBJS_SET_MOTION(1, "立ち", 4);
}

void program_1000(void)
{
    g0 = g0 + 1;
    _OBJS_SET_MOTION(0, "手を振る", 2 | 4);
    func_1(30);
    if (g0 == 1) {
        func_2(100);
    } else {
        func_2(110);
    }
}
