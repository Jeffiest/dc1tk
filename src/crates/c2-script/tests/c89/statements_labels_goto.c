// 3.6.1 Labeled statements, 3.6.6.1 The goto statement
// expect: 10
// goto jumps forwards, backwards, and into and out of loops.
int test(void)
{
    int n = 0;
    int i = 0;

again:
    n++;
    if (n < 3) {
        goto again;
    }
    while (1) {
        i++;
        if (i == 4) {
            goto out;
        }
    }
out:
    goto skip;
    n = 100;
skip:
    return n + i + 3;
}
