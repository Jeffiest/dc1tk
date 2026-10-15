// 3.1.1 Keywords
// expect: 20
// Every keyword but struct, union and double (see their own programs) in its place.
int test(void)
{
    auto int a = 1;
    register int b = 2;
    static int c = 3;
    const int d = 4;
    volatile int e = 5;
    signed short f = 6;
    unsigned long g = 0;
    char h = 0;
    float x = 0.0f;
    enum colour { RED, GREEN } k = GREEN;
    int i;
    typedef int number;
    number n = 0;

    for (i = 0; i < 3; i++) {
        if (i == 1) {
            continue;
        } else {
            n += i;
        }
    }
    do {
        g++;
    } while (g < 2);
    while (1) {
        break;
    }
    switch (k) {
    case GREEN:
        h = 1;
        break;
    default:
        h = 2;
    }
    goto done;
done:
    x = (float)sizeof(int);
    return a + b + c + d + e + f + (int)g + h + (int)x - n - 6;
}
