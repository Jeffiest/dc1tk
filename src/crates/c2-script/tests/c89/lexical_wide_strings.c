// 3.1.4 String literals: wide strings
// planned: step 5: wide string literals are not supported yet
// L"..." is an array of wchar_t, which needs arrays as values.
int test(void)
{
    return L"ab"[1] == 98;
}
