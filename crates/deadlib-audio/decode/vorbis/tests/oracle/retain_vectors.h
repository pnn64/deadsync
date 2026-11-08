/* Include stdio before replacing remove, preserving the CRT declaration. */
#pragma once
#include <stdio.h>
int deadlib_keep_vector(const char *filename);
#define remove deadlib_keep_vector
