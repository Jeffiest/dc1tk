// 3.2.2.2 void
// expect: 2
// An expression cast to void is worked out and its value dropped.
int count;

int bump(void)
{
    count++;
    return count;
}

int test(void)
{
    (void)bump();
    (void)bump();
    return count;
}
