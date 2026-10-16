// dc2tk.h: lets a C compiler or IDE read the scripts. dc2tk's compiler does not
// read this file; it gives C tools a meaning for the scripts' few extensions.
#ifndef DC2TK_H
#define DC2TK_H

typedef const char *string;
typedef union { int i; float f; const char *s; int *ip; float *fp; } any __attribute__((__transparent_union__));

#define NATIVE(...)
#define OPTIONAL(...) ...
#define PROGRAM(number, function) extern int dc2tk_program_##function
#define MAIN(function) extern int dc2tk_main_##function
#define AT(base)
#define DIRECT

void dc2tk_wait(void);
void dc2tk_end(void);
void dc2tk_skip_end(void);
void dc2tk_print(int unused, ...);
float dc2tk_sin(float angle);
float dc2tk_cos(float angle);

// Mod patches (docs/usage.md#mods). NEW_FLAG(name) and its kin are numbers build gives a mod;
// here they read as constants. original(...) in a #pragma override function calls the function it
// replaced. LANGUAGE is the language of the script a patch is merged into; here it reads as English.
#define NEW_FLAG(name) enum { name = 1024 }
#define NEW_COUNTER(name) enum { name = 32 }
#define NEW_PROGRAM(name) enum { name = 9000 }
#define NEW_MONSTER_VAR(name) enum { name = 8 }
int dc2tk_original(int unused, ...);
#define original(...) dc2tk_original(0 __VA_OPT__(,) __VA_ARGS__)
#define LANGUAGE_JAPANESE 0
#define LANGUAGE_ENGLISH 1
#define LANGUAGE_FRENCH 2
#define LANGUAGE_GERMAN 3
#define LANGUAGE_ITALIAN 4
#define LANGUAGE_SPANISH 5
#ifndef LANGUAGE
#define LANGUAGE LANGUAGE_ENGLISH
#endif

#define wait dc2tk_wait()
#define end dc2tk_end()
#define skip_end dc2tk_skip_end()
#define print(...) dc2tk_print(0, __VA_ARGS__)
#define sin(angle) dc2tk_sin(angle)
#define cos(angle) dc2tk_cos(angle)

#endif
