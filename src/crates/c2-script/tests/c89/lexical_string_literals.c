// 3.1.4 String literals
// expect: 1
// Escapes, adjacent literals joined, and sizeof counting the closing 0.
int test(void)
{
    string s = "ab" "cd";

    return s == "abcd" && sizeof("abc") == 4 && sizeof "" == 1 && "x\ty" != "x y";
}
