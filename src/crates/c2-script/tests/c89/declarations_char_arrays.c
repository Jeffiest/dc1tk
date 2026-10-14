// 3.5.7 Initialization: character arrays from strings
// expect: 1
// A char array holds one character in each element, and its 0.
int test(void)
{
    char s[] = "abc";
    char t[3] = "xyz";
    unsigned char u[2] = "\177";

    return sizeof s == 4 && s[1] == 'b' && s[3] == 0 && t[2] == 'z' && u[0] == 127;
}
