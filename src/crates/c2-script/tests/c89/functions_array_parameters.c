// 3.7.1 Function definitions: array parameters are pointers
// expect: 1
void fill(int *out, int count)
{
    int i;

    for (i = 0; i < count; i++) {
        out[i] = i * i;
    }
}

int last(int values[], int count)
{
    return values[count - 1];
}

int test(void)
{
    int squares[4];

    fill(squares, 4);
    return last(squares, 4) == 9 && *squares == 0;
}
