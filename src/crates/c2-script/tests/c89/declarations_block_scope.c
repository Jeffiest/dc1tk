// 3.1.2.1 Scopes of identifiers: blocks
// expect: 1
// A name declared in an inner block hides the outer one until the block ends.
int test(void)
{
    int x = 1;
    int seen;

    {
        float x = 2.5;

        seen = x == 2.5;
        {
            int x = 3;

            seen = seen && x == 3;
        }
    }
    for (int i = 0; i < 2; i++) {
        int x = 10;

        seen = seen && x == 10;
    }
    return seen && x == 1;
}
