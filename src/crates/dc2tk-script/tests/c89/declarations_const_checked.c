// 3.5.3 Type qualifiers: const objects are not modifiable
// rejects: two is const
int test(void)
{
    const int two = 2;

    two = 3;
    return two;
}
