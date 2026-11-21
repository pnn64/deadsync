## Before/after alpha controls: thread_cycles

24 paired measurements; geometric mean ratio **1.063418x**; minimum **0.742726x**; maximum **1.630245x**. Worst: `target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp` / `native-owned-rgba`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | external | native-owned-rgba | 1 | 0.742726 | 0.742726 | 0.742726 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | external | native-owned-rgba | 1 | 0.947143 | 0.947143 | 0.947143 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | external | native-owned-rgba | 1 | 0.796202 | 0.796202 | 0.796202 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | external | native-owned-rgba | 1 | 1.027269 | 1.027269 | 1.027269 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | external | native-owned-rgba | 1 | 0.807648 | 0.807648 | 0.807648 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | external | native-owned-rgba | 1 | 0.949133 | 0.949133 | 0.949133 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | external | native-owned-rgba | 1 | 1.216384 | 1.216384 | 1.216384 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | external | native-owned-rgba | 1 | 0.961797 | 0.961797 | 0.961797 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | external | native-owned-rgba | 1 | 1.389497 | 1.389497 | 1.389497 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | external | native-owned-rgba | 1 | 1.051341 | 1.051341 | 1.051341 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | external | native-owned-rgba | 1 | 1.630245 | 1.630245 | 1.630245 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | external | native-owned-rgba | 1 | 1.04081 | 1.04081 | 1.04081 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | external | image-rgba | 1 | 0.971496 | 0.971496 | 0.971496 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | external | native-owned-rgba | 1 | 1.128247 | 1.128247 | 1.128247 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | external | image-rgba | 1 | 0.96024 | 0.96024 | 0.96024 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | external | native-owned-rgba | 1 | 1.113924 | 1.113924 | 1.113924 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | external | image-rgba | 1 | 1.025895 | 1.025895 | 1.025895 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | external | native-owned-rgba | 1 | 1.289555 | 1.289555 | 1.289555 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | external | image-rgba | 1 | 1.004465 | 1.004465 | 1.004465 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | external | native-owned-rgba | 1 | 1.191501 | 1.191501 | 1.191501 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | external | image-rgba | 1 | 1.020092 | 1.020092 | 1.020092 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | external | native-owned-rgba | 1 | 1.360675 | 1.360675 | 1.360675 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | external | image-rgba | 1 | 0.985057 | 0.985057 | 0.985057 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | external | native-owned-rgba | 1 | 1.347438 | 1.347438 | 1.347438 |

### Observed regressions (9)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-owned-rgba | 630.47 | 848.86 | 0.742726 | 34.639237 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-owned-rgba | 8459351.95 | 10624626.29 | 0.796202 | 25.59622 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-owned-rgba | 705343.53 | 873330.34 | 0.807648 | 23.816311 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-owned-rgba | 651.53 | 687.89 | 0.947143 | 5.58071 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | native-owned-rgba | 337795.30 | 355898.88 | 0.949133 | 5.359334 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | image-rgba | 2663.57 | 2773.86 | 0.96024 | 4.140683 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-owned-rgba | 877.39 | 912.24 | 0.961797 | 3.972008 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | image-rgba | 2843.53 | 2926.96 | 0.971496 | 2.934029 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | image-rgba | 1505585.22 | 1528424.27 | 0.985057 | 1.516955 |

### All paired measurements

| Case | Mode | Before | After | Before/after |
| --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-bottom.bmp | native-owned-rgba | 630.47 | 848.86 | 0.742726 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-17x9-top.bmp | native-owned-rgba | 651.53 | 687.89 | 0.947143 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-bottom.bmp | native-owned-rgba | 8459351.95 | 10624626.29 | 0.796202 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-1920x1080-top.bmp | native-owned-rgba | 8296351.45 | 8076123.32 | 1.027269 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-bottom.bmp | native-owned-rgba | 705343.53 | 873330.34 | 0.807648 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-first-511x257-top.bmp | native-owned-rgba | 337795.30 | 355898.88 | 0.949133 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-bottom.bmp | native-owned-rgba | 887.96 | 730.00 | 1.216384 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-17x9-top.bmp | native-owned-rgba | 877.39 | 912.24 | 0.961797 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-bottom.bmp | native-owned-rgba | 10745067.12 | 7733061.57 | 1.389497 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-1920x1080-top.bmp | native-owned-rgba | 10942308.25 | 10407954.12 | 1.051341 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-bottom.bmp | native-owned-rgba | 459136.85 | 281636.79 | 1.630245 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-last-511x257-top.bmp | native-owned-rgba | 393574.77 | 378142.68 | 1.04081 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | image-rgba | 2843.53 | 2926.96 | 0.971496 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | native-owned-rgba | 1000.36 | 886.65 | 1.128247 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | image-rgba | 2663.57 | 2773.86 | 0.96024 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | native-owned-rgba | 968.59 | 869.53 | 1.113924 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | image-rgba | 36290066.00 | 35374048.80 | 1.025895 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | native-owned-rgba | 13270389.71 | 10290672.17 | 1.289555 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | image-rgba | 31987368.50 | 31845169.67 | 1.004465 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | native-owned-rgba | 12645912.14 | 10613432.00 | 1.191501 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | image-rgba | 1577466.12 | 1546395.75 | 1.020092 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | native-owned-rgba | 476526.14 | 350213.12 | 1.360675 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | image-rgba | 1505585.22 | 1528424.27 | 0.985057 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | native-owned-rgba | 460174.33 | 341517.99 | 1.347438 |

## Equivalent RGBA comparison: native-owned-rgba, thread_cycles

6 exact-output cases; 12 non-equivalent native cases excluded. Image/native geometric mean **3.591578x**; worst **3.000459x**; best **4.475384x**; **0 native regressions versus image**. A ratio below 1 means native is slower or uses more allocations/bytes.

| Case | Pixels | Native | Image | Image/native | Native result |
| --- | --- | --- | --- | --- | --- |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-bottom.bmp | 153 | 886.65 | 2926.96 | 3.301145 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-17x9-top.bmp | 153 | 869.53 | 2773.86 | 3.190068 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-bottom.bmp | 2073600 | 10290672.17 | 35374048.80 | 3.437487 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-1920x1080-top.bmp | 2073600 | 10613432.00 | 31845169.67 | 3.000459 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-bottom.bmp | 131327 | 350213.12 | 1546395.75 | 4.415585 | faster |
| target/bmp-decode-opt-20261120-pass2/alpha-controls\bgra32-zero-511x257-top.bmp | 131327 | 341517.99 | 1528424.27 | 4.475384 | faster |

