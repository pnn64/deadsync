# Full baseline/candidate3 alpha-position controls

Original frozen baseline versus candidate3-direct-rgba (logical alpha + RGBA4444 + direct RGBA). CPU 2; five 80 ms batches. 30 strict process checks had no matching external compute. All raw invocation metadata and executable hashes were checked. Ratios below are candidate / baseline; lower is fewer cycles. Source selection remains provisional because non-target losses remain unresolved.

Geometric means weight cases equally. The sum-of-cycles ratio instead compares one operation of every recorded native case; neither predicts an application's input mix. All sample ranges and every native/reference record are retained in the JSON and raw TSVs.

## decode

18 native cases; cycle geometric ratio 0.951301846, elapsed-ns geometric ratio 0.952380489, summed-cycle ratio 0.940261253. Image control ratio not applicable. 10 native cases have ratios above one.

| Group | Native after/before | ns after/before | Image after/before | Native losses |
| --- | ---: | ---: | ---: | ---: |
| bgra32-first-bottom.bmp | 0.651111 | 0.651424 | — | 2/3 |
| bgra32-first-top.bmp | 1.310361 | 1.312729 | — | 3/3 |
| bgra32-last-bottom.bmp | 1.117798 | 1.120012 | — | 3/3 |
| bgra32-last-top.bmp | 1.054441 | 1.055442 | — | 2/3 |
| bgra32-zero-bottom.bmp | 0.859756 | 0.860484 | — | 0/3 |
| bgra32-zero-top.bmp | 0.857241 | 0.857876 | — | 0/3 |

### Every native and reference case

| Case | Mode | Baseline cycles | Candidate cycles | After/before | Baseline range | Candidate range |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-17x9-bottom.bmp | native-decode | 625.96 | 654.61 | 1.045770 | 621.19–633.09 | 631.44–662.45 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-17x9-top.bmp | native-decode | 617.89 | 654.18 | 1.058732 | 609.77–620.82 | 637.27–785.41 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-1920x1080-bottom.bmp | native-decode | 7687205.13 | 7740562.26 | 1.006941 | 7552885.08–8004895.41 | 7660750.61–7925057.09 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-1920x1080-top.bmp | native-decode | 7614747.79 | 7889364.61 | 1.036064 | 7509570.58–8908572.00 | 7784586.65–7899642.87 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-511x257-bottom.bmp | native-decode | 689521.53 | 180747.45 | 0.262135 | 680452.50–703877.34 | 179095.93–181937.15 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-511x257-top.bmp | native-decode | 333437.13 | 683934.57 | 2.051165 | 226135.11–345011.57 | 365320.70–704528.98 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-17x9-bottom.bmp | native-decode | 847.71 | 947.34 | 1.117528 | 837.52–863.82 | 938.28–963.51 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-17x9-top.bmp | native-decode | 855.17 | 896.22 | 1.048002 | 827.96–884.87 | 882.52–898.89 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-1920x1080-bottom.bmp | native-decode | 10349347.65 | 10690553.71 | 1.032969 | 9997176.17–11257688.50 | 10542120.41–10738701.82 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-1920x1080-top.bmp | native-decode | 10616621.06 | 10393350.88 | 0.978970 | 10171069.22–13025396.79 | 10229163.44–10411995.47 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-511x257-bottom.bmp | native-decode | 448972.68 | 543205.60 | 1.209886 | 416680.38–451166.02 | 539608.50–545042.58 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-511x257-top.bmp | native-decode | 402676.26 | 460141.74 | 1.142709 | 374938.09–403891.97 | 448626.08–476091.91 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-17x9-bottom.bmp | native-decode | 955.26 | 894.99 | 0.936907 | 936.29–964.74 | 869.65–903.70 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-17x9-top.bmp | native-decode | 961.22 | 899.48 | 0.935769 | 957.03–987.09 | 845.35–1139.06 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-1920x1080-bottom.bmp | native-decode | 12823048.71 | 10517575.12 | 0.820209 | 12350664.80–13222335.14 | 10425217.06–10896832.18 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-1920x1080-top.bmp | native-decode | 12188966.73 | 10410859.18 | 0.854122 | 11987612.07–12703406.00 | 10255357.17–10985769.50 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-511x257-bottom.bmp | native-decode | 498639.15 | 412374.45 | 0.827000 | 480764.63–509771.29 | 407658.75–416131.62 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-511x257-top.bmp | native-decode | 476174.24 | 375306.87 | 0.788171 | 461746.26–479701.76 | 364767.61–378687.64 |

## owned

18 native cases; cycle geometric ratio 0.987775187, elapsed-ns geometric ratio 0.987523025, summed-cycle ratio 0.951958743. Image control ratio 1.008769692. 9 native cases have ratios above one.

| Group | Native after/before | ns after/before | Image after/before | Native losses |
| --- | ---: | ---: | ---: | ---: |
| bgra32-first-bottom.bmp | 1.596046 | 1.594932 | — | 3/3 |
| bgra32-first-top.bmp | 0.839909 | 0.838806 | — | 2/3 |
| bgra32-last-bottom.bmp | 1.010079 | 1.010420 | — | 2/3 |
| bgra32-last-top.bmp | 0.989964 | 0.989750 | — | 2/3 |
| bgra32-zero-bottom.bmp | 0.849365 | 0.849612 | 1.007312 | 0/3 |
| bgra32-zero-top.bmp | 0.815834 | 0.815890 | 1.010229 | 0/3 |

| Reference comparison | Wins | Geometric image/native | Worst image/native |
| --- | ---: | ---: | ---: |
| before | 6/6 | 2.902516 | 2.474483 |
| after | 6/6 | 3.517373 | 2.943093 |

### Every native and reference case

| Case | Mode | Baseline cycles | Candidate cycles | After/before | Baseline range | Candidate range |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-17x9-bottom.bmp | native-owned-rgba | 637.95 | 654.23 | 1.025519 | 631.94–647.57 | 643.78–685.62 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-17x9-top.bmp | native-owned-rgba | 642.51 | 664.89 | 1.034832 | 636.25–646.52 | 645.03–671.11 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-1920x1080-bottom.bmp | native-owned-rgba | 7550525.79 | 7958112.00 | 1.053981 | 7484446.88–8057226.23 | 7718183.09–8073230.05 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-1920x1080-top.bmp | native-owned-rgba | 7616545.00 | 7774279.61 | 1.020709 | 7518359.62–7850235.83 | 7721714.00–8045433.27 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-511x257-bottom.bmp | native-owned-rgba | 178603.40 | 671813.83 | 3.761484 | 173515.40–183519.15 | 652900.33–673055.18 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-first-511x257-top.bmp | native-owned-rgba | 659214.99 | 369787.07 | 0.560951 | 344145.13–667329.89 | 244163.60–372336.19 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-17x9-bottom.bmp | native-owned-rgba | 880.37 | 944.91 | 1.073310 | 860.38–892.29 | 921.45–961.02 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-17x9-top.bmp | native-owned-rgba | 876.02 | 907.24 | 1.035638 | 872.41–887.52 | 895.61–912.12 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-1920x1080-bottom.bmp | native-owned-rgba | 10249979.50 | 10422686.18 | 1.016849 | 10230273.44–10325641.35 | 10317997.76–10463616.59 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-1920x1080-top.bmp | native-owned-rgba | 10484946.71 | 10652296.24 | 1.015961 | 10097121.94–10613393.18 | 10449052.29–11358397.62 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-511x257-bottom.bmp | native-owned-rgba | 501226.38 | 473279.94 | 0.944244 | 456796.65–513266.93 | 472479.59–479515.65 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-last-511x257-top.bmp | native-owned-rgba | 448465.37 | 413525.58 | 0.922090 | 413875.02–457235.06 | 409220.15–424003.82 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-17x9-bottom.bmp | native-owned-rgba | 1012.09 | 936.19 | 0.925007 | 972.32–1023.13 | 924.74–945.53 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-17x9-top.bmp | native-owned-rgba | 996.31 | 854.19 | 0.857354 | 980.48–1023.09 | 841.90–865.74 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-1920x1080-bottom.bmp | native-owned-rgba | 12352699.40 | 10339366.59 | 0.837013 | 12100581.27–12376990.67 | 10139778.22–10566330.00 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-1920x1080-top.bmp | native-owned-rgba | 12427197.80 | 10568705.41 | 0.850450 | 12151022.47–12740767.14 | 10199518.89–10682910.06 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-511x257-bottom.bmp | native-owned-rgba | 488489.93 | 386599.68 | 0.791418 | 467725.89–492085.46 | 372423.61–387893.57 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-511x257-top.bmp | native-owned-rgba | 469490.07 | 349642.15 | 0.744727 | 457808.42–485507.16 | 342009.28–357163.67 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-17x9-bottom.bmp | image-rgba | 2898.09 | 2879.97 | 0.993748 | 2882.49–2927.45 | 2811.79–2979.63 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-17x9-top.bmp | image-rgba | 2709.48 | 2796.00 | 1.031932 | 2680.04–2770.65 | 2667.08–2805.78 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-1920x1080-bottom.bmp | image-rgba | 35128472.80 | 35758130.80 | 1.017924 | 34591517.33–35550571.60 | 35043759.83–36674806.40 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-1920x1080-top.bmp | image-rgba | 30750888.67 | 31104687.17 | 1.011505 | 29804697.83–31620182.67 | 30952097.00–32741389.17 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-511x257-bottom.bmp | image-rgba | 1610453.84 | 1627229.93 | 1.010417 | 1532938.57–1638785.06 | 1571318.19–1705521.33 |
| target/bmp-decode-opt-20261120-pass2/alpha-controls/bgra32-zero-511x257-top.bmp | image-rgba | 1553871.57 | 1534813.51 | 0.987735 | 1524339.68–1584989.90 | 1519080.83–1572917.40 |
