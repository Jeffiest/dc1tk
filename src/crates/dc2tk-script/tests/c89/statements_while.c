// 3.6.5.1 The while statement
// expect: 55
int test(void)
{
    int i = 0;
    int sum = 0;

    while (i < 10) {
        i++;
        sum += i;
    }
    return sum;
}
