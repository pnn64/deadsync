## Before/after alpha controls: thread_cycles

18 paired measurements; geometric mean ratio **1.224256x**; minimum **0.513185x**; maximum **3.979547x**. Worst: `target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp` / `native-decode`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | external | native-decode | 1 | 0.79382 | 0.79382 | 0.79382 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | external | native-decode | 1 | 0.997739 | 0.997739 | 0.997739 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | external | native-decode | 1 | 0.716866 | 0.716866 | 0.716866 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | external | native-decode | 1 | 1.062954 | 1.062954 | 1.062954 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | external | native-decode | 1 | 0.513185 | 0.513185 | 0.513185 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | external | native-decode | 1 | 3.979547 | 3.979547 | 3.979547 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | external | native-decode | 1 | 1.281113 | 1.281113 | 1.281113 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | external | native-decode | 1 | 0.960901 | 0.960901 | 0.960901 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | external | native-decode | 1 | 1.180924 | 1.180924 | 1.180924 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | external | native-decode | 1 | 1.070956 | 1.070956 | 1.070956 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | external | native-decode | 1 | 2.923889 | 2.923889 | 2.923889 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | external | native-decode | 1 | 1.378841 | 1.378841 | 1.378841 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | external | native-decode | 1 | 1.222055 | 1.222055 | 1.222055 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | external | native-decode | 1 | 1.216907 | 1.216907 | 1.216907 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | external | native-decode | 1 | 1.240582 | 1.240582 | 1.240582 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | external | native-decode | 1 | 1.207309 | 1.207309 | 1.207309 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | external | native-decode | 1 | 1.514634 | 1.514634 | 1.514634 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | external | native-decode | 1 | 1.462403 | 1.462403 | 1.462403 |

### Observed regressions (5)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-decode | 177103.31 | 345105.88 | 0.513185 | 94.861338 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-decode | 7603270.00 | 10606265.71 | 0.716866 | 39.496108 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-decode | 663.53 | 835.87 | 0.79382 | 25.973204 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-decode | 857.71 | 892.61 | 0.960901 | 4.068974 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-decode | 644.29 | 645.75 | 0.997739 | 0.226606 |

### All paired measurements

| Case | Mode | Before | After | Before/after |
| --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-decode | 663.53 | 835.87 | 0.79382 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-decode | 644.29 | 645.75 | 0.997739 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-decode | 7603270.00 | 10606265.71 | 0.716866 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | native-decode | 8516505.86 | 8012109.32 | 1.062954 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-decode | 177103.31 | 345105.88 | 0.513185 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | native-decode | 674723.88 | 169547.91 | 3.979547 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | native-decode | 868.62 | 678.02 | 1.281113 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-decode | 857.71 | 892.61 | 0.960901 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | native-decode | 10662896.76 | 9029286.20 | 1.180924 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | native-decode | 11718734.44 | 10942308.44 | 1.070956 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | native-decode | 518293.74 | 177261.74 | 2.923889 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | native-decode | 465507.49 | 337607.89 | 1.378841 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | native-decode | 1001.01 | 819.12 | 1.222055 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | native-decode | 970.97 | 797.90 | 1.216907 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | native-decode | 13208083.14 | 10646679.82 | 1.240582 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | native-decode | 12955172.14 | 10730619.06 | 1.207309 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | native-decode | 516917.40 | 341281.97 | 1.514634 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | native-decode | 497194.35 | 339984.39 | 1.462403 |

