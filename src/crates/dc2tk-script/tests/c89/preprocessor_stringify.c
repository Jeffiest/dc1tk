// 3.8.3.2 The # operator
// expect: 1
#define STR(x) #x
#define QUOTE(x) STR(x)
#define VALUE 42

int test(void)
{
    return STR(hello world) == "hello world" && STR("a\n") == "\"a\\n\"" && QUOTE(VALUE) == "42"
        && STR(VALUE) == "VALUE";
}
