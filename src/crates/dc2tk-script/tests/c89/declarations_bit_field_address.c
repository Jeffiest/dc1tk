// 3.3.3.2 Address operator: not of a bit-field
// rejects: a bit-field has no address
struct flags {
    unsigned on : 1;
};

int test(void)
{
    struct flags f;
    unsigned *p = &f.on;

    return *p;
}
