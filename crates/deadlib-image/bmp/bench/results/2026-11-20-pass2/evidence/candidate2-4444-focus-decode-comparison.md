## Before/after 16-bit focus: thread_cycles

32 paired measurements; geometric mean ratio **1.035086x**; minimum **0.747486x**; maximum **1.576657x**. Worst: `rgb565-511x257-top` / `native-decode`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| rgb333 | 1x1 | native-decode | 2 | 0.997417 | 0.98544 | 1.00954 |
| rgb333 | 17x9 | native-decode | 2 | 0.954322 | 0.933942 | 0.975146 |
| rgb333 | 511x257 | native-decode | 2 | 1.133663 | 0.912431 | 1.408536 |
| rgb333 | 1920x1080 | native-decode | 2 | 0.935847 | 0.926771 | 0.945012 |
| rgb555 | 1x1 | native-decode | 2 | 0.951866 | 0.946117 | 0.95765 |
| rgb555 | 17x9 | native-decode | 2 | 0.9332 | 0.916795 | 0.949898 |
| rgb555 | 511x257 | native-decode | 2 | 0.960399 | 0.957785 | 0.96302 |
| rgb555 | 1920x1080 | native-decode | 2 | 0.957685 | 0.957566 | 0.957805 |
| rgb565 | 1x1 | native-decode | 2 | 0.937727 | 0.927595 | 0.94797 |
| rgb565 | 17x9 | native-decode | 2 | 0.909422 | 0.908893 | 0.909952 |
| rgb565 | 511x257 | native-decode | 2 | 1.077874 | 0.747486 | 1.554293 |
| rgb565 | 1920x1080 | native-decode | 2 | 0.917744 | 0.899463 | 0.936396 |
| rgba4444 | 1x1 | native-decode | 2 | 0.952858 | 0.947118 | 0.958633 |
| rgba4444 | 17x9 | native-decode | 2 | 1.247466 | 1.243205 | 1.251741 |
| rgba4444 | 511x257 | native-decode | 2 | 1.569347 | 1.562071 | 1.576657 |
| rgba4444 | 1920x1080 | native-decode | 2 | 1.337294 | 1.333422 | 1.341178 |

### Observed regressions (23)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| rgb565-511x257-top | native-decode | 570056.26 | 762630.99 | 0.747486 | 33.781706 |
| rgb565-1920x1080-top | native-decode | 12467000.00 | 13860496.31 | 0.899463 | 11.177479 |
| rgb565-17x9-bottom | native-decode | 1081.01 | 1189.37 | 0.908893 | 10.023959 |
| rgb565-17x9-top | native-decode | 1092.57 | 1200.69 | 0.909952 | 9.895933 |
| rgb333-511x257-top | native-decode | 703065.38 | 770540.57 | 0.912431 | 9.597285 |
| rgb555-17x9-top | native-decode | 1140.20 | 1243.68 | 0.916795 | 9.075601 |
| rgb333-1920x1080-top | native-decode | 14554369.62 | 15704383.17 | 0.926771 | 7.9015 |
| rgb565-1x1-bottom | native-decode | 384.59 | 414.61 | 0.927595 | 7.805715 |
| rgb333-17x9-top | native-decode | 1282.91 | 1373.65 | 0.933942 | 7.072983 |
| rgb565-1920x1080-bottom | native-decode | 12839778.21 | 13711912.15 | 0.936396 | 6.792438 |
| rgb333-1920x1080-bottom | native-decode | 14802074.08 | 15663373.92 | 0.945012 | 5.818778 |
| rgb555-1x1-bottom | native-decode | 387.17 | 409.22 | 0.946117 | 5.695173 |
| rgba4444-1x1-top | native-decode | 398.14 | 420.37 | 0.947118 | 5.583463 |
| rgb565-1x1-top | native-decode | 383.16 | 404.19 | 0.94797 | 5.488569 |
| rgb555-17x9-bottom | native-decode | 1169.22 | 1230.89 | 0.949898 | 5.274456 |
| rgb555-1920x1080-top | native-decode | 13331598.57 | 13922378.85 | 0.957566 | 4.431429 |
| rgb555-1x1-top | native-decode | 393.69 | 411.10 | 0.95765 | 4.422261 |
| rgb555-511x257-top | native-decode | 608627.77 | 635453.28 | 0.957785 | 4.40754 |
| rgb555-1920x1080-bottom | native-decode | 13328400.71 | 13915574.00 | 0.957805 | 4.405429 |
| rgba4444-1x1-bottom | native-decode | 395.81 | 412.89 | 0.958633 | 4.315202 |
| rgb555-511x257-bottom | native-decode | 1000886.34 | 1039320.73 | 0.96302 | 3.840035 |
| rgb333-17x9-bottom | native-decode | 1288.88 | 1321.73 | 0.975146 | 2.548724 |
| rgb333-1x1-bottom | native-decode | 417.59 | 423.76 | 0.98544 | 1.477526 |

### All paired measurements

| Case | Mode | Before | After | Before/after |
| --- | --- | --- | --- | --- |
| rgb333-17x9-bottom | native-decode | 1288.88 | 1321.73 | 0.975146 |
| rgb333-17x9-top | native-decode | 1282.91 | 1373.65 | 0.933942 |
| rgb333-1920x1080-bottom | native-decode | 14802074.08 | 15663373.92 | 0.945012 |
| rgb333-1920x1080-top | native-decode | 14554369.62 | 15704383.17 | 0.926771 |
| rgb333-1x1-bottom | native-decode | 417.59 | 423.76 | 0.98544 |
| rgb333-1x1-top | native-decode | 412.72 | 408.82 | 1.00954 |
| rgb333-511x257-bottom | native-decode | 1078292.44 | 765541.24 | 1.408536 |
| rgb333-511x257-top | native-decode | 703065.38 | 770540.57 | 0.912431 |
| rgb555-17x9-bottom | native-decode | 1169.22 | 1230.89 | 0.949898 |
| rgb555-17x9-top | native-decode | 1140.20 | 1243.68 | 0.916795 |
| rgb555-1920x1080-bottom | native-decode | 13328400.71 | 13915574.00 | 0.957805 |
| rgb555-1920x1080-top | native-decode | 13331598.57 | 13922378.85 | 0.957566 |
| rgb555-1x1-bottom | native-decode | 387.17 | 409.22 | 0.946117 |
| rgb555-1x1-top | native-decode | 393.69 | 411.10 | 0.95765 |
| rgb555-511x257-bottom | native-decode | 1000886.34 | 1039320.73 | 0.96302 |
| rgb555-511x257-top | native-decode | 608627.77 | 635453.28 | 0.957785 |
| rgb565-17x9-bottom | native-decode | 1081.01 | 1189.37 | 0.908893 |
| rgb565-17x9-top | native-decode | 1092.57 | 1200.69 | 0.909952 |
| rgb565-1920x1080-bottom | native-decode | 12839778.21 | 13711912.15 | 0.936396 |
| rgb565-1920x1080-top | native-decode | 12467000.00 | 13860496.31 | 0.899463 |
| rgb565-1x1-bottom | native-decode | 384.59 | 414.61 | 0.927595 |
| rgb565-1x1-top | native-decode | 383.16 | 404.19 | 0.94797 |
| rgb565-511x257-bottom | native-decode | 958969.25 | 616980.97 | 1.554293 |
| rgb565-511x257-top | native-decode | 570056.26 | 762630.99 | 0.747486 |
| rgba4444-17x9-bottom | native-decode | 1250.74 | 999.20 | 1.251741 |
| rgba4444-17x9-top | native-decode | 1207.04 | 970.91 | 1.243205 |
| rgba4444-1920x1080-bottom | native-decode | 16683756.00 | 12511986.07 | 1.333422 |
| rgba4444-1920x1080-top | native-decode | 16392379.73 | 12222375.13 | 1.341178 |
| rgba4444-1x1-bottom | native-decode | 395.81 | 412.89 | 0.958633 |
| rgba4444-1x1-top | native-decode | 398.14 | 420.37 | 0.947118 |
| rgba4444-511x257-bottom | native-decode | 709242.55 | 449839.48 | 1.576657 |
| rgba4444-511x257-top | native-decode | 721099.10 | 461630.22 | 1.562071 |

