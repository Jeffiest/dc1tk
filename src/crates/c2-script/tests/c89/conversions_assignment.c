// 3.2.1.3 Floating and integral: assignment
// differs: 2.5; C gives 2 (an int keeps the float stored in it; step 4)
int test(void)
{
    int x = 2.5;

    return x;
}
