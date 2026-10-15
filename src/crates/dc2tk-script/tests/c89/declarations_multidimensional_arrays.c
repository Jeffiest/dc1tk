// 3.5.4.2 Array declarators: arrays of arrays
// expect: 1
int sum_row(int row[], int count)
{
    int total = 0;
    int i;

    for (i = 0; i < count; i++) {
        total += row[i];
    }
    return total;
}

int trace(int m[][3])
{
    return m[0][0] + m[1][1] + m[2][2];
}

int test(void)
{
    int m[3][3] = {{1, 2, 3}, {4, 5, 6}, {7, 8, 9}};
    int flat[2][2] = {1, 2, 3};
    int cube[2][2][2];
    int i;
    int j;
    int k;

    for (i = 0; i < 2; i++) {
        for (j = 0; j < 2; j++) {
            for (k = 0; k < 2; k++) {
                cube[i][j][k] = i * 4 + j * 2 + k;
            }
        }
    }
    return m[1][2] == 6 && sum_row(m[2], 3) == 24 && trace(m) == 15 && flat[1][0] == 3 && flat[1][1] == 0
        && cube[1][1][0] == 6 && sizeof m == 36 && sizeof m[0] == 12;
}
