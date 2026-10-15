// 3.5.2 Type specifiers: long long
// rejects: long long is C99
// C89 has no long long; the VM's integers are 32 bits.
int test(void)
{
    long long x = 1;

    return (int)x;
}
