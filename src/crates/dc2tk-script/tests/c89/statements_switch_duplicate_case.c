// 3.6.4.2 The switch statement: case values differ
// rejects: case 2 is in the switch twice
#define TWO 2

int test(void)
{
    switch (1) {
    case 1 + 1:
        return 1;
    case TWO:
        return 2;
    }
    return 0;
}
