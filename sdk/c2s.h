// c2s.h: lets a C compiler or IDE read c2s scripts. The c2s compiler does not
// read this file; it gives C tools a meaning for c2s's few extensions.
#ifndef C2S_H
#define C2S_H

typedef const char *string;
typedef union { int i; float f; const char *s; int *ip; float *fp; } any __attribute__((__transparent_union__));

#define NATIVE(...)
#define OPTIONAL(...) ...
#define PROGRAM(number, function) extern int c2s_program_##function
#define MAIN(function) extern int c2s_main_##function
#define AT(base)
#define DIRECT

void c2s_wait(void);
void c2s_end(void);
void c2s_skip_end(void);
void c2s_print(int unused, ...);
float c2s_sin(float angle);
float c2s_cos(float angle);

#define wait c2s_wait()
#define end c2s_end()
#define skip_end c2s_skip_end()
#define print(...) c2s_print(0, __VA_ARGS__)
#define sin(angle) c2s_sin(angle)
#define cos(angle) c2s_cos(angle)

#endif
