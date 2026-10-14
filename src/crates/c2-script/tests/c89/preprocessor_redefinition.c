// 3.8.3 Macro replacement: a macro is defined again only as it was
// rejects: LIMIT is #defined again differently; #undef it first
#define LIMIT  (1 + 2)
#define LIMIT (1 + 2)
#define LIMIT (1 + 3)

int test(void)
{
    return LIMIT;
}
