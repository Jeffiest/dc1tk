// 3.6.5.3 The for statement
// expect: 1
// Any part may be left out; a missing condition is true.
int test(void)
{
    int sum = 0;
    int i;

    for (i = 1; i <= 4; i++) {
        sum += i;
    }
    for (;;) {
        if (sum > 12) {
            break;
        }
        sum++;
    }
    return sum == 13 && i == 5;
}
