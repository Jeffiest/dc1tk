// 3.6.5.2 The do statement
// expect: 1
int test(void)
{
    int runs = 0;

    do {
        runs++;
    } while (0);
    return runs;
}
