// 3.1.3.4 Character constants: wide characters
// expect: 1
// L'x' is a wchar_t: the character's Shift-JIS code, the game's text encoding.
int test(void)
{
    return L'a' == 97 && L'あ' == 0x82A0 && L'\n' == 10;
}
