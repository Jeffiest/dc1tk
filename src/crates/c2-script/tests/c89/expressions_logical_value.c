// 3.3.13, 3.3.14 Logical AND and OR: the result
// differs: 5; C gives 1 (&& and || give the deciding value; step 4)
int test(void)
{
    int a = 0;

    return a || 5;
}
