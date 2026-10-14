// 3.5.2.2 Enumeration specifiers
// expect: 1
enum colour { RED, GREEN = 5, BLUE, BLACK = GREEN * 4 };
enum { ANONYMOUS = -2 };

int test(void)
{
    enum colour c = BLUE;

    return RED == 0 && GREEN == 5 && c == 6 && BLACK == 20 && ANONYMOUS == -2;
}
