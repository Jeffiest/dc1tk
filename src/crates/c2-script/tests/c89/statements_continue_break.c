// 3.6.6.2 continue, 3.6.6.3 break
// expect: 1
// continue in a for runs its step; break leaves the innermost loop or switch.
int test(void)
{
    int odd = 0;
    int i;
    int j;
    int pairs = 0;

    for (i = 0; i < 10; i++) {
        if (i % 2 == 0) {
            continue;
        }
        odd += i;
    }
    for (i = 0; i < 3; i++) {
        for (j = 0; j < 3; j++) {
            if (j == 1) {
                break;
            }
            pairs++;
        }
    }
    return odd == 25 && pairs == 3;
}
