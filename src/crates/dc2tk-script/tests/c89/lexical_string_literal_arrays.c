// 3.1.4 String literals: arrays of char
// expect: 15
// A string literal is the VM's string, which natives and string variables take; indexed,
// dereferenced, or stored in or passed as a char pointer, it is an array of its characters and a
// 0, kept as long as the script runs.
char *greeting(void)
{
    return "hello";
}

int length(const char *text)
{
    int count = 0;

    while (text[count] != 0) {
        count++;
    }
    return count;
}

int test(void)
{
    char *p = "abc";

    return ("xyz"[1] == 'y') + (*p == 'a') * 2 + (length(greeting()) == 5) * 4 + (length("ab") == 2) * 8;
}
