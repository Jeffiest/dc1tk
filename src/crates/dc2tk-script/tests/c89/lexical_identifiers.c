// 3.1.2 Identifiers
// expect: 6
// Letters, digits and underscores, case-sensitive, at least 31 significant characters.
int test(void)
{
    int _leading = 1;
    int Case = 2;
    int case_ = 3;
    int an_identifier_of_31_characters_ = 0;
    int x1y2 = 0;

    an_identifier_of_31_characters_ = Case - _leading;
    x1y2 = case_;
    return _leading + Case + an_identifier_of_31_characters_ + x1y2 - 1;
}
