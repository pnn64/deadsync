/* Test-only override of upstream test.c's successful-fixture removal. All
   generation and upstream encode/decode self-checks remain unmodified. */
int deadlib_keep_vector(const char *filename) {
    (void)filename;
    return 0;
}
