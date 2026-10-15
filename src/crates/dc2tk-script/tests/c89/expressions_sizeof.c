// 3.3.3.4 The sizeof operator
// expect: 1
// int, long, float and pointers are the VM's 4 bytes; char and short are C's 1 and 2.
int test(void)
{
    int a[10];
    char c;
    short s;
    int x;

    return sizeof(int) == 4 && sizeof(long) == 4 && sizeof(float) == 4 && sizeof(char) == 1
        && sizeof(short) == 2 && sizeof(int *) == 4 && sizeof a == 40 && sizeof a / sizeof a[0] == 10
        && sizeof c == 1 && sizeof s == 2 && sizeof(x + c) == 4 && sizeof(int[3]) == 12 && sizeof(x = 9) == 4
        && x != 9;
}
