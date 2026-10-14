// <float.h>: the floating types (C89 2.2.4.2.2). The VM's only one is float, a 32-bit IEEE 754
// single held and worked out by the PS2's FPU. That FPU rounds toward zero (FLT_ROUNDS 0), flushes
// results below FLT_MIN to zero, and has no infinities or NaNs: past FLT_MAX it goes on to about
// 6.8E+38 and then stays there. The compiler reads constants as IEEE singles, rounded to nearest,
// up to FLT_MAX. double and long double are not supported (see docs/c-standard.md), so the DBL_
// and LDBL_ names are left out, and #ifdef DBL_MAX tells code so.

#ifndef C2S_FLOAT_H
#define C2S_FLOAT_H

#define FLT_RADIX 2
#define FLT_ROUNDS 0
#define FLT_MANT_DIG 24
#define FLT_EPSILON 1.19209290E-07F
#define FLT_DIG 6
#define FLT_MIN_EXP (-125)
#define FLT_MIN 1.17549435E-38F
#define FLT_MIN_10_EXP (-37)
#define FLT_MAX_EXP 128
#define FLT_MAX 3.40282347E+38F
#define FLT_MAX_10_EXP 38

#endif
