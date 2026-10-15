// 3.6.4.2 The switch statement
// expect: 1
int pick(int v)
{
    int out = 0;

    switch (v) {
    case 1:
        out += 10;
    case 2:
        out += 20;
        break;
    case 'a':
    case -3:
        out = 3;
        break;
    default:
        out = -1;
    }
    return out;
}

int test(void)
{
    return pick(1) == 30 && pick(2) == 20 && pick(97) == 3 && pick(-3) == 3 && pick(5) == -1;
}
