// 3.1.3.1 Floating constants
// expect: 1
// Digits with a point or an exponent, f suffixes. The VM's floats are 32 bits.
int test(void)
{
    float a = 1.5;
    float b = .25;
    float c = 2.;
    float d = 1e2;
    float e = 2.5e-1f;
    float f = 3.0F;

    return a == 1.5f && b + e == 0.5 && c == 2 && d == 100 && f == 3;
}
