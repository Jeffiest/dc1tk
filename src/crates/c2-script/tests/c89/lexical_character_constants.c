// 3.1.3.4 Character constants
// expect: 1
// Characters, simple escapes, octal and hexadecimal escapes. char is signed.
int test(void)
{
    return 'a' == 97 && '\n' == 10 && '\t' == 9 && '\0' == 0 && '\\' == 92 && '\'' == 39
        && '\"' == 34 && '\?' == 63 && '\a' == 7 && '\b' == 8 && '\f' == 12 && '\v' == 11
        && '\r' == 13 && '\101' == 65 && '\x41' == 65 && '\377' == -1;
}
