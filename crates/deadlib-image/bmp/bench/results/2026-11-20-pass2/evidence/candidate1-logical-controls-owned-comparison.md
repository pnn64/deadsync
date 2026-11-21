## Before/after alpha controls: thread_cycles

24 paired measurements; geometric mean ratio **1.032731x**; minimum **0.24428x**; maximum **1.809203x**. Worst: `target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp` / `native-owned-rgba`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | external | native-owned-rgba | 1 | 0.97417 | 0.97417 | 0.97417 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | external | native-owned-rgba | 1 | 0.98307 | 0.98307 | 0.98307 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | external | native-owned-rgba | 1 | 0.993736 | 0.993736 | 0.993736 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | external | native-owned-rgba | 1 | 0.986265 | 0.986265 | 0.986265 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | external | native-owned-rgba | 1 | 0.24428 | 0.24428 | 0.24428 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | external | native-owned-rgba | 1 | 1.809203 | 1.809203 | 1.809203 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | external | native-owned-rgba | 1 | 0.958249 | 0.958249 | 0.958249 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | external | native-owned-rgba | 1 | 0.997022 | 0.997022 | 0.997022 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | external | native-owned-rgba | 1 | 0.990994 | 0.990994 | 0.990994 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | external | native-owned-rgba | 1 | 1.01303 | 1.01303 | 1.01303 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | external | native-owned-rgba | 1 | 1.105298 | 1.105298 | 1.105298 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | external | native-owned-rgba | 1 | 1.119569 | 1.119569 | 1.119569 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | external | image-rgba | 1 | 1.009613 | 1.009613 | 1.009613 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | external | native-owned-rgba | 1 | 1.115741 | 1.115741 | 1.115741 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | external | image-rgba | 1 | 0.999133 | 0.999133 | 0.999133 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | external | native-owned-rgba | 1 | 1.228203 | 1.228203 | 1.228203 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | external | image-rgba | 1 | 1.035793 | 1.035793 | 1.035793 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | external | native-owned-rgba | 1 | 1.277441 | 1.277441 | 1.277441 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | external | image-rgba | 1 | 0.977575 | 0.977575 | 0.977575 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | external | native-owned-rgba | 1 | 1.173958 | 1.173958 | 1.173958 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | external | image-rgba | 1 | 1.072803 | 1.072803 | 1.072803 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | external | native-owned-rgba | 1 | 1.332357 | 1.332357 | 1.332357 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | external | image-rgba | 1 | 1.064126 | 1.064126 | 1.064126 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | external | native-owned-rgba | 1 | 1.37804 | 1.37804 | 1.37804 |

### Observed regressions (10)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-owned-rgba | 178641.55 | 731297.15 | 0.24428 | 309.365654 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | native-owned-rgba | 909.57 | 949.20 | 0.958249 | 4.357004 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-owned-rgba | 679.24 | 697.25 | 0.97417 | 2.651493 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | image-rgba | 32303339.83 | 33044370.83 | 0.977575 | 2.293976 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-owned-rgba | 675.88 | 687.52 | 0.98307 | 1.722199 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | native-owned-rgba | 7915838.30 | 8026077.23 | 0.986265 | 1.392637 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | native-owned-rgba | 10928323.82 | 11027638.62 | 0.990994 | 0.908783 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-owned-rgba | 7914310.96 | 7964198.23 | 0.993736 | 0.630343 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-owned-rgba | 914.11 | 916.84 | 0.997022 | 0.298651 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | image-rgba | 2835.67 | 2838.13 | 0.999133 | 0.086752 |

### All paired measurements

| Case | Mode | Before | After | Before/after |
| --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-owned-rgba | 679.24 | 697.25 | 0.97417 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-owned-rgba | 675.88 | 687.52 | 0.98307 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-owned-rgba | 7914310.96 | 7964198.23 | 0.993736 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | native-owned-rgba | 7915838.30 | 8026077.23 | 0.986265 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-owned-rgba | 178641.55 | 731297.15 | 0.24428 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | native-owned-rgba | 685104.95 | 378677.70 | 1.809203 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | native-owned-rgba | 909.57 | 949.20 | 0.958249 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-owned-rgba | 914.11 | 916.84 | 0.997022 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | native-owned-rgba | 10928323.82 | 11027638.62 | 0.990994 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | native-owned-rgba | 10758134.12 | 10619758.53 | 1.01303 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | native-owned-rgba | 526882.96 | 476688.63 | 1.105298 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | native-owned-rgba | 459320.33 | 410265.32 | 1.119569 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | image-rgba | 2993.09 | 2964.59 | 1.009613 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | native-owned-rgba | 1043.53 | 935.28 | 1.115741 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | image-rgba | 2835.67 | 2838.13 | 0.999133 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | native-owned-rgba | 1077.38 | 877.20 | 1.228203 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | image-rgba | 39618871.60 | 38249805.20 | 1.035793 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | native-owned-rgba | 13869309.85 | 10857102.65 | 1.277441 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | image-rgba | 32303339.83 | 33044370.83 | 0.977575 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | native-owned-rgba | 12642181.36 | 10768850.53 | 1.173958 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | image-rgba | 1812903.23 | 1689875.63 | 1.072803 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | native-owned-rgba | 525296.81 | 394261.20 | 1.332357 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | image-rgba | 1686716.10 | 1585072.21 | 1.064126 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | native-owned-rgba | 490331.91 | 355818.35 | 1.37804 |

## Equivalent RGBA comparison: native-owned-rgba, thread_cycles

6 exact-output cases; 12 non-equivalent native cases excluded. Image/native geometric mean **3.58329x**; worst **3.068514x**; best **4.454723x**; **0 native regressions versus image**. A ratio below 1 means native is slower or uses more allocations/bytes.

| Case | Pixels | Native | Image | Image/native | Native result |
| --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | 153 | 935.28 | 2964.59 | 3.169735 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | 153 | 877.20 | 2838.13 | 3.235442 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | 2073600 | 10857102.65 | 38249805.20 | 3.523021 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | 2073600 | 10768850.53 | 33044370.83 | 3.068514 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | 131327 | 394261.20 | 1689875.63 | 4.286183 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | 131327 | 355818.35 | 1585072.21 | 4.454723 | faster |

