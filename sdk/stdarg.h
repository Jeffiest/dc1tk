// <stdarg.h>: variable arguments (C89 4.8). A function whose parameters end in ... takes the
// arguments after its named ones in a hidden array its caller fills, and a hidden pointer to it;
// va_list points into the array, and va_arg reads the next argument and steps past it. An
// argument after ... is passed as it is: char and short as int, float as float (there is no
// double); va_arg(ap, char) and va_arg(ap, short) read the int and convert it back.

#ifndef DC2TK_STDARG_H
#define DC2TK_STDARG_H

#define va_list __va_list
#define va_start(ap, last) __va_start(ap, last)
#define va_arg(ap, type) __va_arg(ap, type)
#define va_end(ap) ((void)(ap))

#endif
