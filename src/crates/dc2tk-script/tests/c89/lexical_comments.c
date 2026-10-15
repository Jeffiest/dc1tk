// 3.1.9 Comments
// expect: 3
/* A comment
   over lines. */
int test(void)
{
    int a = 1 /* inside an expression */ + 2;
    /* "a string in a comment" */
    return a; /* after code */
}
