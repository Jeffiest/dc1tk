#pragma natives event

PROGRAM(100, program_100);
MAIN(program_100);

void func_1(int a0)
{
    int v1;

    v1 = 1;
    while (v1) {
        wait;
        _OBJS_CHECK(a0, &v1);
    }
}

void func_2(int a0, int a1, int a2)
{
    int v3;

    _SET_MES_AUTOSET(0, a0, a1);
    _MES_MAKE(0, a2);
    v3 = 0;
    while (!v3) {
        wait;
        _CHECK_MES(0, &v3);
    }
}

void program_100(void)
{
    int v0;

    _INITIALIZE();
    _EOH_SYNC_CHARA(0, 0);
    _OBJS_INIT(0);
    _OBJS_SYNC_OBJ(0, 0);
    _EOH_SYNC_CHARA(1, 8);
    _OBJS_INIT(1);
    _OBJS_SYNC_OBJ(1, 1);
    _OBJS_SET_POS(0, 0.0, 0.0, 0.0);
    _OBJS_SET_ROT(0, 0.0, 0.0, 0.0);
    _OBJS_SET_POS(1, 0.0, 0.0, 240.0);
    _OBJS_SET_ROT(1, 0.0, 3.1415927, 0.0);
    _OBJS_SET_MOTION(0, "立ち", 4);
    _OBJS_SET_MOTION(1, "歩き", 4);
    _CMRS_SET_POS(80.0, 45.0, -60.0);
    _CMRS_SET_REF(0.0, 20.0, 80.0);
    _SET_FADE_IN(30);
    _OBJS_MOVE(1, 0.0, 0.0, 50.0, 120, 1);
    _CMRS_MOVE(50.0, 35.0, -30.0, 0.0, 20.0, 25.0, 120);
    func_1(1);
    _OBJS_SET_MOTION(1, "立ち", 4);
    func_2(8, 0, 200);
    _OBJS_SET_MOTION(0, "うなずく", 4 | 2);
    _OBJS_NEXT_MOTION(0, "立ち", 4);
    _OBJS_TEX_ANIME(0, 0);
    _OBJS_TEX_ANIME(0, 1, "笑顔");
    _OBJS_TEX_ANIME(0, 1, "まばたき");
    func_2(0, 8, 210);
    func_2(8, 0, 220);
    _SET_FADE_OUT(30);
    v0 = 30;
    while (v0 > 0) {
        wait;
        v0 = v0 - 1;
    }
    _SET_FLAG(1200, 1);
}
