// <stddef.h>: common definitions (C89 4.1.5). sizeof gives a size_t; a wide character holds a
// Shift-JIS code. offsetof needs structs, which the compiler does not have yet.

#ifndef C2S_STDDEF_H
#define C2S_STDDEF_H

typedef unsigned int size_t;
typedef int ptrdiff_t;
typedef unsigned short wchar_t;

#define NULL 0

#endif
