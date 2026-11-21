## Before/after 16-bit focus: thread_cycles

50 paired measurements; geometric mean ratio **1.01525x**; minimum **0.690797x**; maximum **1.993956x**. Worst: `rgb333-511x257-bottom` / `native-owned-rgba`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| rgb333 | 1x1 | image-rgba | 2 | 0.997299 | 0.989376 | 1.005286 |
| rgb333 | 1x1 | native-owned-rgba | 2 | 0.979081 | 0.965113 | 0.993251 |
| rgb333 | 17x9 | image-rgba | 2 | 0.958441 | 0.957233 | 0.95965 |
| rgb333 | 17x9 | native-owned-rgba | 2 | 0.920656 | 0.909564 | 0.931883 |
| rgb333 | 511x257 | image-rgba | 2 | 0.89346 | 0.828558 | 0.963447 |
| rgb333 | 511x257 | native-owned-rgba | 2 | 0.797252 | 0.690797 | 0.920113 |
| rgb333 | 1920x1080 | image-rgba | 2 | 0.969388 | 0.962935 | 0.975885 |
| rgb333 | 1920x1080 | native-owned-rgba | 2 | 0.932306 | 0.916046 | 0.948853 |
| rgb555 | 1x1 | native-owned-rgba | 2 | 0.953156 | 0.952677 | 0.953635 |
| rgb555 | 17x9 | native-owned-rgba | 2 | 0.901494 | 0.888126 | 0.915064 |
| rgb555 | 511x257 | native-owned-rgba | 2 | 1.123557 | 0.969358 | 1.302285 |
| rgb555 | 1920x1080 | native-owned-rgba | 2 | 0.970549 | 0.96141 | 0.979775 |
| rgb565 | 1x1 | image-rgba | 2 | 0.99788 | 0.994448 | 1.001323 |
| rgb565 | 1x1 | native-owned-rgba | 2 | 1.018373 | 1.002967 | 1.034016 |
| rgb565 | 17x9 | native-owned-rgba | 2 | 0.966914 | 0.964184 | 0.969651 |
| rgb565 | 511x257 | native-owned-rgba | 2 | 1.133083 | 0.969453 | 1.324331 |
| rgb565 | 1920x1080 | native-owned-rgba | 2 | 0.98639 | 0.967085 | 1.00608 |
| rgba4444 | 1x1 | image-rgba | 2 | 1.023934 | 1.020564 | 1.027315 |
| rgba4444 | 1x1 | native-owned-rgba | 2 | 0.93925 | 0.925303 | 0.953406 |
| rgba4444 | 17x9 | image-rgba | 2 | 1.004121 | 1.001911 | 1.006335 |
| rgba4444 | 17x9 | native-owned-rgba | 2 | 1.255723 | 1.255655 | 1.255791 |
| rgba4444 | 511x257 | image-rgba | 2 | 1.051252 | 1.028081 | 1.074946 |
| rgba4444 | 511x257 | native-owned-rgba | 2 | 1.802809 | 1.629986 | 1.993956 |
| rgba4444 | 1920x1080 | image-rgba | 2 | 0.961352 | 0.944808 | 0.978186 |
| rgba4444 | 1920x1080 | native-owned-rgba | 2 | 1.157696 | 1.153741 | 1.161664 |

### Observed regressions (31)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| rgb333-511x257-bottom | native-owned-rgba | 1105410.24 | 1600196.16 | 0.690797 | 44.760389 |
| rgb333-511x257-bottom | image-rgba | 2355309.15 | 2842662.22 | 0.828558 | 20.691682 |
| rgb555-17x9-bottom | native-owned-rgba | 1637.89 | 1844.21 | 0.888126 | 12.596695 |
| rgb333-17x9-top | native-owned-rgba | 1817.69 | 1998.42 | 0.909564 | 9.94284 |
| rgb555-17x9-top | native-owned-rgba | 1707.19 | 1865.65 | 0.915064 | 9.281919 |
| rgb333-1920x1080-bottom | native-owned-rgba | 25666291.86 | 28018548.57 | 0.916046 | 9.16477 |
| rgb333-511x257-top | native-owned-rgba | 1500747.07 | 1631045.69 | 0.920113 | 8.682251 |
| rgba4444-1x1-bottom | native-owned-rgba | 438.64 | 474.05 | 0.925303 | 8.072679 |
| rgb333-17x9-bottom | native-owned-rgba | 1838.41 | 1972.79 | 0.931883 | 7.309577 |
| rgba4444-1920x1080-top | image-rgba | 45395014.50 | 48046793.50 | 0.944808 | 5.841564 |
| rgb333-1920x1080-top | native-owned-rgba | 26724532.14 | 28165079.86 | 0.948853 | 5.390357 |
| rgb555-1x1-bottom | native-owned-rgba | 570.12 | 598.44 | 0.952677 | 4.967375 |
| rgba4444-1x1-top | native-owned-rgba | 447.71 | 469.59 | 0.953406 | 4.887092 |
| rgb555-1x1-top | native-owned-rgba | 580.64 | 608.87 | 0.953635 | 4.861877 |
| rgb333-17x9-top | image-rgba | 3791.13 | 3960.51 | 0.957233 | 4.467797 |
| rgb333-17x9-bottom | image-rgba | 3893.06 | 4056.75 | 0.95965 | 4.204662 |
| rgb555-1920x1080-top | native-owned-rgba | 24345512.88 | 25322711.43 | 0.96141 | 4.013875 |
| rgb333-1920x1080-top | image-rgba | 45623678.00 | 47379789.25 | 0.962935 | 3.849122 |
| rgb333-511x257-top | image-rgba | 2761687.25 | 2866464.20 | 0.963447 | 3.793947 |
| rgb565-17x9-top | native-owned-rgba | 1797.76 | 1864.54 | 0.964184 | 3.714623 |
| rgb333-1x1-top | native-owned-rgba | 589.52 | 610.83 | 0.965113 | 3.614805 |
| rgb565-1920x1080-top | native-owned-rgba | 25134286.00 | 25989741.14 | 0.967085 | 3.403539 |
| rgb555-511x257-top | native-owned-rgba | 1443480.02 | 1489109.93 | 0.969358 | 3.161104 |
| rgb565-511x257-top | native-owned-rgba | 1456236.77 | 1502122.66 | 0.969453 | 3.150991 |
| rgb565-17x9-bottom | native-owned-rgba | 1795.91 | 1852.12 | 0.969651 | 3.12989 |
| rgb333-1920x1080-bottom | image-rgba | 49349307.75 | 50568795.50 | 0.975885 | 2.471134 |
| rgba4444-1920x1080-bottom | image-rgba | 52384936.75 | 53553171.00 | 0.978186 | 2.230096 |
| rgb555-1920x1080-bottom | native-owned-rgba | 24523281.62 | 25029491.71 | 0.979775 | 2.064202 |
| rgb333-1x1-bottom | image-rgba | 867.90 | 877.22 | 0.989376 | 1.073856 |
| rgb333-1x1-bottom | native-owned-rgba | 621.10 | 625.32 | 0.993251 | 0.67944 |
| rgb565-1x1-bottom | image-rgba | 881.25 | 886.17 | 0.994448 | 0.558298 |

### All paired measurements

| Case | Mode | Before | After | Before/after |
| --- | --- | --- | --- | --- |
| rgb333-17x9-bottom | image-rgba | 3893.06 | 4056.75 | 0.95965 |
| rgb333-17x9-bottom | native-owned-rgba | 1838.41 | 1972.79 | 0.931883 |
| rgb333-17x9-top | image-rgba | 3791.13 | 3960.51 | 0.957233 |
| rgb333-17x9-top | native-owned-rgba | 1817.69 | 1998.42 | 0.909564 |
| rgb333-1920x1080-bottom | image-rgba | 49349307.75 | 50568795.50 | 0.975885 |
| rgb333-1920x1080-bottom | native-owned-rgba | 25666291.86 | 28018548.57 | 0.916046 |
| rgb333-1920x1080-top | image-rgba | 45623678.00 | 47379789.25 | 0.962935 |
| rgb333-1920x1080-top | native-owned-rgba | 26724532.14 | 28165079.86 | 0.948853 |
| rgb333-1x1-bottom | image-rgba | 867.90 | 877.22 | 0.989376 |
| rgb333-1x1-bottom | native-owned-rgba | 621.10 | 625.32 | 0.993251 |
| rgb333-1x1-top | image-rgba | 859.63 | 855.11 | 1.005286 |
| rgb333-1x1-top | native-owned-rgba | 589.52 | 610.83 | 0.965113 |
| rgb333-511x257-bottom | image-rgba | 2355309.15 | 2842662.22 | 0.828558 |
| rgb333-511x257-bottom | native-owned-rgba | 1105410.24 | 1600196.16 | 0.690797 |
| rgb333-511x257-top | image-rgba | 2761687.25 | 2866464.20 | 0.963447 |
| rgb333-511x257-top | native-owned-rgba | 1500747.07 | 1631045.69 | 0.920113 |
| rgb555-17x9-bottom | native-owned-rgba | 1637.89 | 1844.21 | 0.888126 |
| rgb555-17x9-top | native-owned-rgba | 1707.19 | 1865.65 | 0.915064 |
| rgb555-1920x1080-bottom | native-owned-rgba | 24523281.62 | 25029491.71 | 0.979775 |
| rgb555-1920x1080-top | native-owned-rgba | 24345512.88 | 25322711.43 | 0.96141 |
| rgb555-1x1-bottom | native-owned-rgba | 570.12 | 598.44 | 0.952677 |
| rgb555-1x1-top | native-owned-rgba | 580.64 | 608.87 | 0.953635 |
| rgb555-511x257-bottom | native-owned-rgba | 1396777.95 | 1072559.30 | 1.302285 |
| rgb555-511x257-top | native-owned-rgba | 1443480.02 | 1489109.93 | 0.969358 |
| rgb565-17x9-bottom | native-owned-rgba | 1795.91 | 1852.12 | 0.969651 |
| rgb565-17x9-top | native-owned-rgba | 1797.76 | 1864.54 | 0.964184 |
| rgb565-1920x1080-bottom | native-owned-rgba | 25688806.71 | 25533556.43 | 1.00608 |
| rgb565-1920x1080-top | native-owned-rgba | 25134286.00 | 25989741.14 | 0.967085 |
| rgb565-1x1-bottom | image-rgba | 881.25 | 886.17 | 0.994448 |
| rgb565-1x1-bottom | native-owned-rgba | 632.07 | 630.20 | 1.002967 |
| rgb565-1x1-top | image-rgba | 870.27 | 869.12 | 1.001323 |
| rgb565-1x1-top | native-owned-rgba | 641.10 | 620.01 | 1.034016 |
| rgb565-511x257-bottom | native-owned-rgba | 1413955.55 | 1067675.44 | 1.324331 |
| rgb565-511x257-top | native-owned-rgba | 1456236.77 | 1502122.66 | 0.969453 |
| rgba4444-17x9-bottom | image-rgba | 4074.84 | 4049.19 | 1.006335 |
| rgba4444-17x9-bottom | native-owned-rgba | 1309.02 | 1042.50 | 1.255655 |
| rgba4444-17x9-top | image-rgba | 4025.67 | 4017.99 | 1.001911 |
| rgba4444-17x9-top | native-owned-rgba | 1306.60 | 1040.46 | 1.255791 |
| rgba4444-1920x1080-bottom | image-rgba | 52384936.75 | 53553171.00 | 0.978186 |
| rgba4444-1920x1080-bottom | native-owned-rgba | 17050381.18 | 14778349.33 | 1.153741 |
| rgba4444-1920x1080-top | image-rgba | 45395014.50 | 48046793.50 | 0.944808 |
| rgba4444-1920x1080-top | native-owned-rgba | 16706344.55 | 14381386.77 | 1.161664 |
| rgba4444-1x1-bottom | image-rgba | 655.10 | 641.90 | 1.020564 |
| rgba4444-1x1-bottom | native-owned-rgba | 438.64 | 474.05 | 0.925303 |
| rgba4444-1x1-top | image-rgba | 646.14 | 628.96 | 1.027315 |
| rgba4444-1x1-top | native-owned-rgba | 447.71 | 469.59 | 0.953406 |
| rgba4444-511x257-bottom | image-rgba | 2713590.03 | 2639472.24 | 1.028081 |
| rgba4444-511x257-bottom | native-owned-rgba | 754577.00 | 462934.65 | 1.629986 |
| rgba4444-511x257-top | image-rgba | 2858380.47 | 2659093.17 | 1.074946 |
| rgba4444-511x257-top | native-owned-rgba | 912727.88 | 457747.24 | 1.993956 |

## Equivalent RGBA comparison: native-owned-rgba, thread_cycles

18 exact-output cases; 14 non-equivalent native cases excluded. Image/native geometric mean **2.2138x**; worst **1.339381x**; best **5.809086x**; **0 native regressions versus image**. A ratio below 1 means native is slower or uses more allocations/bytes.

| Case | Pixels | Native | Image | Image/native | Native result |
| --- | --- | --- | --- | --- | --- |
| rgb333-17x9-bottom | 153 | 1972.79 | 4056.75 | 2.056352 | faster |
| rgb333-17x9-top | 153 | 1998.42 | 3960.51 | 1.981821 | faster |
| rgb333-1920x1080-bottom | 2073600 | 28018548.57 | 50568795.50 | 1.804833 | faster |
| rgb333-1920x1080-top | 2073600 | 28165079.86 | 47379789.25 | 1.682217 | faster |
| rgb333-1x1-bottom | 1 | 625.32 | 877.22 | 1.402834 | faster |
| rgb333-1x1-top | 1 | 610.83 | 855.11 | 1.399915 | faster |
| rgb333-511x257-bottom | 131327 | 1600196.16 | 2842662.22 | 1.776446 | faster |
| rgb333-511x257-top | 131327 | 1631045.69 | 2866464.20 | 1.75744 | faster |
| rgb565-1x1-bottom | 1 | 630.20 | 886.17 | 1.406173 | faster |
| rgb565-1x1-top | 1 | 620.01 | 869.12 | 1.401784 | faster |
| rgba4444-17x9-bottom | 153 | 1042.50 | 4049.19 | 3.884115 | faster |
| rgba4444-17x9-top | 153 | 1040.46 | 4017.99 | 3.861744 | faster |
| rgba4444-1920x1080-bottom | 2073600 | 14778349.33 | 53553171.00 | 3.623759 | faster |
| rgba4444-1920x1080-top | 2073600 | 14381386.77 | 48046793.50 | 3.340901 | faster |
| rgba4444-1x1-bottom | 1 | 474.05 | 641.90 | 1.354077 | faster |
| rgba4444-1x1-top | 1 | 469.59 | 628.96 | 1.339381 | faster |
| rgba4444-511x257-bottom | 131327 | 462934.65 | 2639472.24 | 5.701609 | faster |
| rgba4444-511x257-top | 131327 | 457747.24 | 2659093.17 | 5.809086 | faster |

