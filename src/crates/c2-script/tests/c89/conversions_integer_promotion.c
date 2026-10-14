// 3.2.1.1 Characters and integers
// expect: 1
// char and short take part in arithmetic as int; storing wraps to their width.
int test(void)
{
    char c = 100;
    unsigned char u = 200;
    short s = 30000;

    c = c + 100;
    u = u + 100;
    s = s + 10000;
    return c == -56 && u == 44 && s == -25536 && c + u == -12;
}
