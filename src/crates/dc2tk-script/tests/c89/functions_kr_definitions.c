// 3.7.1 Function definitions: identifier lists
// expect: 1
// An old-style definition: parameters named, then declared; one left undeclared is an int.
int scale(value, factor, offset)
float value;
int factor;
{
    return (int)(value * factor) + offset;
}

int test(void)
{
    return scale(1.5, 4, 1) == 7;
}
