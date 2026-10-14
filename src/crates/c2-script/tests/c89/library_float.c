// 2.2.4.2 Numerical limits: <float.h>
// expect: 1
// The VM's float is IEEE single precision; double (and DBL_*) is left out.
#include <float.h>

int test(void)
{
    float one = 1.0f;

#ifdef DBL_MAX
    return 0;
#else
    return FLT_RADIX == 2 && FLT_MANT_DIG == 24 && FLT_DIG == 6 && FLT_MIN_EXP == -125 && FLT_MAX_EXP == 128
        && FLT_MIN_10_EXP == -37 && FLT_MAX_10_EXP == 38 && FLT_ROUNDS == 0 && FLT_MAX > 3.4e38f
        && FLT_MIN < 1.2e-38f && FLT_EPSILON == 1.0f / 8388608 && one + FLT_EPSILON > one;
#endif
}
