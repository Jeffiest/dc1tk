// 3.5.2.1 Union specifiers
// expect: 3
// The members share their slots. The VM's values keep their type, so reading a member other than
// the one last stored gives the stored value, not its bits (C leaves that to the implementation).
union value {
    int i;
    float f;
    char c[8];
};

int test(void)
{
    union value v;

    v.i = 1;
    v.f = 2.5;
    return (v.f == 2.5) + sizeof v / 4;
}
