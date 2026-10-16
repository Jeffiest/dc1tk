// <stddef.h>: common definitions (C89 4.1.5). sizeof gives a size_t; a wide character holds a
// Shift-JIS code; offsetof gives a member's offset in C's bytes.

#ifndef DC2TK_STDDEF_H
#define DC2TK_STDDEF_H

typedef unsigned int size_t;
typedef int ptrdiff_t;
typedef unsigned short wchar_t;

#define NULL 0
#define offsetof(type, member) __offsetof(type, member)

#endif
