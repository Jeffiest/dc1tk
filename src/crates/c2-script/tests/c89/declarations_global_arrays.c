// 3.5.4.2 Array declarators: arrays with static storage
// planned: step 5: global arrays are not supported
int table[4];

int test(void)
{
    table[1] = 2;
    return table[1];
}
