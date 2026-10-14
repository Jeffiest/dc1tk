// 3.5.7 Initialization: objects with static storage
// expect: 1
// Globals start at their initializers, and others at 0.
int a = 5;
float b = 1.5;
int c;
int d = -3 + 2 * 4;
string s = "set";

int test(void)
{
    return a == 5 && b == 1.5 && c == 0 && d == 5 && s == "set";
}
