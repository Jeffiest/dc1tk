// 3.8.2 Source file inclusion: a header's macros in the including file's constant expressions
// expect: 8623
// A header's #defines expand where the file uses them, as C expands them: as array sizes (with an
// initialiser, of structs, of two dimensions, in sizeof), enumerator values, case labels and
// initialisers, a parenthesised value (CHAR_MIN) as well as a number.
#include <limits.h>

enum sizes { BITS = CHAR_BIT, MORE, LOW = CHAR_MIN };

struct pair {
    int a;
    int b;
};

int table[CHAR_BIT] = {1, 2};
struct pair pairs[MB_LEN_MAX] = {{3, 4}, {5, 6}};
int grid[MB_LEN_MAX][CHAR_BIT];
int bits = CHAR_BIT * 2;

int which(int n)
{
    switch (n) {
    case CHAR_BIT:
        return 1;
    case MB_LEN_MAX:
        return 2;
    case CHAR_MIN:
        return 3;
    }
    return 0;
}

int test(void)
{
    int local[CHAR_BIT] = {7};
    static char text[MB_LEN_MAX + 1] = "ab";
    int i;
    int sum = 0;

    for (i = 0; i < CHAR_BIT; i++) {
        sum += table[i];
    }
    return (int)(sizeof(table) / sizeof(table[0])) * 1000 + pairs[1].b * 100 + which(CHAR_BIT) * 10
        + which(2) + which(-128) + (MORE == 9) + (LOW == -128) + (local[0] == 7) + (text[1] == 'b')
        + (sizeof(grid) == 2 * 8 * 4) + (sizeof(int[CHAR_BIT]) == 32) + (bits == 16) + (sum == 3);
}
