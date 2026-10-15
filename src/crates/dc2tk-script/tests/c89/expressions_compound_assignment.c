// 3.3.16.2 Compound assignment
// expect: 1
int test(void)
{
    int a = 10;
    int v[2] = {1, 1};
    int i = 0;

    a += 5;
    a -= 3;
    a *= 2;
    a /= 4;
    a %= 4;
    a <<= 3;
    a >>= 1;
    a |= 1;
    a &= 7;
    a ^= 2;
    v[i++] += 4;
    return a == 3 && v[0] == 5 && i == 1;
}
