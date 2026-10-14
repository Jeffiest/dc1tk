// 3.1.4 String literals: wide strings
// expect: 15
// L"..." is an array of wchar_t holding each character's Shift-JIS code, then 0.
#include <stddef.h>

int test(void)
{
    wchar_t s[] = L"ab";

    return (L"ab"[1] == 98) + (s[0] == L'a') * 2 + (sizeof s == 6) * 4 + (s[2] == 0) * 8;
}
