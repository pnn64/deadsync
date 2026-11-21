## Before/after alpha controls: thread_cycles

18 paired measurements; geometric mean ratio **1.095178x**; minimum **0.528591x**; maximum **4.157414x**. Worst: `target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp` / `native-decode`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | external | native-decode | 1 | 0.95939 | 0.95939 | 0.95939 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | external | native-decode | 1 | 0.965453 | 0.965453 | 0.965453 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | external | native-decode | 1 | 1.008399 | 1.008399 | 1.008399 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | external | native-decode | 1 | 1.021353 | 1.021353 | 1.021353 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | external | native-decode | 1 | 4.157414 | 4.157414 | 4.157414 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | external | native-decode | 1 | 0.528591 | 0.528591 | 0.528591 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | external | native-decode | 1 | 0.929295 | 0.929295 | 0.929295 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | external | native-decode | 1 | 0.996203 | 0.996203 | 0.996203 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | external | native-decode | 1 | 1.022053 | 1.022053 | 1.022053 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | external | native-decode | 1 | 1.007094 | 1.007094 | 1.007094 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | external | native-decode | 1 | 0.878666 | 0.878666 | 0.878666 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | external | native-decode | 1 | 0.937063 | 0.937063 | 0.937063 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | external | native-decode | 1 | 1.11449 | 1.11449 | 1.11449 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | external | native-decode | 1 | 1.167512 | 1.167512 | 1.167512 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | external | native-decode | 1 | 1.211642 | 1.211642 | 1.211642 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | external | native-decode | 1 | 1.249949 | 1.249949 | 1.249949 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | external | native-decode | 1 | 1.195991 | 1.195991 | 1.195991 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | external | native-decode | 1 | 1.325198 | 1.325198 | 1.325198 |

### Observed regressions (7)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | native-decode | 354617.74 | 670873.48 | 0.528591 | 89.182154 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | native-decode | 453758.03 | 516417.13 | 0.878666 | 13.808924 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | native-decode | 873.76 | 940.24 | 0.929295 | 7.608497 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | native-decode | 406553.12 | 433858.97 | 0.937063 | 6.716429 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-decode | 630.54 | 657.23 | 0.95939 | 4.23288 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-decode | 626.56 | 648.98 | 0.965453 | 3.578269 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-decode | 873.61 | 876.94 | 0.996203 | 0.381177 |

### All paired measurements

| Case | Mode | Before | After | Before/after |
| --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-decode | 630.54 | 657.23 | 0.95939 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-decode | 626.56 | 648.98 | 0.965453 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-decode | 7845111.48 | 7779767.09 | 1.008399 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | native-decode | 7906180.22 | 7740887.09 | 1.021353 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-decode | 735817.97 | 176989.35 | 4.157414 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | native-decode | 354617.74 | 670873.48 | 0.528591 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | native-decode | 873.76 | 940.24 | 0.929295 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-decode | 873.61 | 876.94 | 0.996203 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | native-decode | 10874184.76 | 10639552.53 | 1.022053 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | native-decode | 10903107.47 | 10826308.24 | 1.007094 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | native-decode | 453758.03 | 516417.13 | 0.878666 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | native-decode | 406553.12 | 433858.97 | 0.937063 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | native-decode | 994.95 | 892.74 | 1.11449 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | native-decode | 984.68 | 843.40 | 1.167512 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | native-decode | 12984851.50 | 10716738.59 | 1.211642 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | native-decode | 13300994.43 | 10641230.82 | 1.249949 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | native-decode | 503537.99 | 421021.57 | 1.195991 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | native-decode | 493057.36 | 372063.19 | 1.325198 |

