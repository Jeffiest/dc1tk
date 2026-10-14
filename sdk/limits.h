// <limits.h>: the ranges of the integer types (C89 2.2.4.2.1). The VM's values are 32-bit ints:
// int and long are those 32 bits, and char (signed, as on the PS2) and short are C's 8 and 16
// bits, wrapped to their width when stored. A string's characters are Shift-JIS, which takes at
// most two bytes for one.

#ifndef C2S_LIMITS_H
#define C2S_LIMITS_H

#define CHAR_BIT 8
#define SCHAR_MIN (-128)
#define SCHAR_MAX 127
#define UCHAR_MAX 255
#define CHAR_MIN (-128)
#define CHAR_MAX 127
#define MB_LEN_MAX 2
#define SHRT_MIN (-32768)
#define SHRT_MAX 32767
#define USHRT_MAX 65535
#define INT_MIN (-2147483647 - 1)
#define INT_MAX 2147483647
#define UINT_MAX 4294967295U
#define LONG_MIN (-2147483647L - 1L)
#define LONG_MAX 2147483647L
#define ULONG_MAX 4294967295UL

#endif
