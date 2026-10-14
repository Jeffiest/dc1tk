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

// Mod patches (docs/usage.md#patches). NEW_FLAG(name) and its kin are numbers build gives a mod;
// here they read as constants. original(...) in a #pragma override function calls the function it
// replaced. LANGUAGE is the language of the script a patch is merged into; here it reads as English.
#define NEW_FLAG(name) enum { name = 1024 }
#define NEW_PROGRAM(name) enum { name = 9000 }
#define NEW_MONSTER_VAR(name) enum { name = 8 }
int c2s_original(int unused, ...);
#define original(...) c2s_original(0 __VA_OPT__(,) __VA_ARGS__)
#define LANGUAGE_JAPANESE 0
#define LANGUAGE_ENGLISH 1
#define LANGUAGE_FRENCH 2
#define LANGUAGE_GERMAN 3
#define LANGUAGE_ITALIAN 4
#define LANGUAGE_SPANISH 5
#ifndef LANGUAGE
#define LANGUAGE LANGUAGE_ENGLISH
#endif

#define wait c2s_wait()
#define end c2s_end()
#define skip_end c2s_skip_end()
#define print(...) c2s_print(0, __VA_ARGS__)
#define sin(angle) c2s_sin(angle)
#define cos(angle) c2s_cos(angle)

#endif
