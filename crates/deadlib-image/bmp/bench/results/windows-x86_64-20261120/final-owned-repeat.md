Inputs: `crates/deadlib-image/bmp/bench/results/windows-x86_64-20261120/final-owned-r1.tsv` -> `crates/deadlib-image/bmp/bench/results/windows-x86_64-20261120/final-owned-r2.tsv`. Complete key sets match (218 rows).

## Stage comparison: thread_cycles

120 paired measurements; geometric mean ratio **0.9935x**; minimum **0.869172x**; maximum **1.263696x**. Worst: `bgra32-zero-1920x1080-top` / `native-owned-rgba`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| bgra32 | 1x1 | native-owned-rgba | 2 | 0.966625 | 0.946809 | 0.986857 |
| bgra32 | 17x9 | native-owned-rgba | 2 | 0.968391 | 0.964623 | 0.972174 |
| bgra32 | 511x257 | native-owned-rgba | 2 | 0.982959 | 0.976502 | 0.989459 |
| bgra32 | 1920x1080 | native-owned-rgba | 2 | 1.157959 | 1.061069 | 1.263696 |
| bgra32-zero | 1x1 | native-owned-rgba | 2 | 0.986588 | 0.965558 | 1.008076 |
| bgra32-zero | 17x9 | native-owned-rgba | 2 | 0.978126 | 0.976805 | 0.979449 |
| bgra32-zero | 511x257 | native-owned-rgba | 2 | 0.990158 | 0.980545 | 0.999865 |
| bgra32-zero | 1920x1080 | native-owned-rgba | 2 | 1.003979 | 0.869172 | 1.159694 |
| bgrx32 | 1x1 | native-owned-rgba | 2 | 1.024247 | 1.013653 | 1.034951 |
| bgrx32 | 17x9 | native-owned-rgba | 2 | 1.019234 | 1.004195 | 1.034498 |
| bgrx32 | 511x257 | native-owned-rgba | 2 | 1.009738 | 1.009147 | 1.010328 |
| bgrx32 | 1920x1080 | native-owned-rgba | 2 | 0.997798 | 0.975569 | 1.020533 |
| core24 | 1x1 | native-owned-rgba | 1 | 1.02814 | 1.02814 | 1.02814 |
| core24 | 17x9 | native-owned-rgba | 1 | 1.013287 | 1.013287 | 1.013287 |
| core24 | 511x257 | native-owned-rgba | 1 | 0.978913 | 0.978913 | 0.978913 |
| core24 | 1920x1080 | native-owned-rgba | 1 | 0.998721 | 0.998721 | 0.998721 |
| core8 | 1x1 | native-owned-rgba | 1 | 0.947996 | 0.947996 | 0.947996 |
| core8 | 17x9 | native-owned-rgba | 1 | 0.991163 | 0.991163 | 0.991163 |
| core8 | 511x257 | native-owned-rgba | 1 | 0.979923 | 0.979923 | 0.979923 |
| core8 | 1920x1080 | native-owned-rgba | 1 | 0.913842 | 0.913842 | 0.913842 |
| indexed1 | 1x1 | native-owned-rgba | 2 | 0.976002 | 0.974264 | 0.977742 |
| indexed1 | 17x9 | native-owned-rgba | 2 | 0.999336 | 0.981073 | 1.017938 |
| indexed1 | 511x257 | native-owned-rgba | 2 | 1.004717 | 0.980059 | 1.029996 |
| indexed1 | 1920x1080 | native-owned-rgba | 2 | 1.005077 | 0.995793 | 1.014448 |
| indexed4 | 1x1 | native-owned-rgba | 2 | 0.974028 | 0.961311 | 0.986914 |
| indexed4 | 17x9 | native-owned-rgba | 2 | 0.973444 | 0.971176 | 0.975718 |
| indexed4 | 511x257 | native-owned-rgba | 2 | 0.973904 | 0.970156 | 0.977666 |
| indexed4 | 1920x1080 | native-owned-rgba | 2 | 0.982675 | 0.965943 | 0.999696 |
| indexed8 | 1x1 | native-owned-rgba | 2 | 0.96704 | 0.96597 | 0.96811 |
| indexed8 | 17x9 | native-owned-rgba | 2 | 0.983936 | 0.97787 | 0.99004 |
| indexed8 | 511x257 | native-owned-rgba | 2 | 0.987071 | 0.98632 | 0.987822 |
| indexed8 | 1920x1080 | native-owned-rgba | 2 | 0.992561 | 0.987102 | 0.998051 |
| indexed8-short | 1x1 | native-owned-rgba | 2 | 1.007113 | 0.952905 | 1.064405 |
| indexed8-short | 17x9 | native-owned-rgba | 2 | 0.94786 | 0.94617 | 0.949553 |
| indexed8-short | 511x257 | native-owned-rgba | 2 | 0.975836 | 0.975342 | 0.97633 |
| indexed8-short | 1920x1080 | native-owned-rgba | 2 | 0.96388 | 0.95212 | 0.975786 |
| rgb24 | 1x1 | native-owned-rgba | 2 | 0.98141 | 0.972239 | 0.990667 |
| rgb24 | 17x9 | native-owned-rgba | 2 | 0.960998 | 0.959739 | 0.962258 |
| rgb24 | 511x257 | native-owned-rgba | 2 | 0.98149 | 0.977779 | 0.985215 |
| rgb24 | 1920x1080 | native-owned-rgba | 2 | 0.983387 | 0.977416 | 0.989394 |
| rgb333 | 1x1 | native-owned-rgba | 2 | 0.977034 | 0.963509 | 0.990749 |
| rgb333 | 17x9 | native-owned-rgba | 2 | 0.972885 | 0.971089 | 0.974684 |
| rgb333 | 511x257 | native-owned-rgba | 2 | 0.998574 | 0.99497 | 1.002191 |
| rgb333 | 1920x1080 | native-owned-rgba | 2 | 1.004122 | 0.990719 | 1.017705 |
| rgb555 | 1x1 | native-owned-rgba | 2 | 0.969515 | 0.953424 | 0.985876 |
| rgb555 | 17x9 | native-owned-rgba | 2 | 0.978758 | 0.976569 | 0.980952 |
| rgb555 | 511x257 | native-owned-rgba | 2 | 0.985851 | 0.981724 | 0.989996 |
| rgb555 | 1920x1080 | native-owned-rgba | 2 | 0.991342 | 0.987099 | 0.995603 |
| rgb565 | 1x1 | native-owned-rgba | 2 | 0.995897 | 0.969163 | 1.023369 |
| rgb565 | 17x9 | native-owned-rgba | 2 | 1.001671 | 0.986215 | 1.017368 |
| rgb565 | 511x257 | native-owned-rgba | 2 | 0.990391 | 0.989842 | 0.99094 |
| rgb565 | 1920x1080 | native-owned-rgba | 2 | 0.999785 | 0.992886 | 1.006731 |
| rgba32 | 1x1 | native-owned-rgba | 2 | 0.972503 | 0.969377 | 0.97564 |
| rgba32 | 17x9 | native-owned-rgba | 2 | 1.001764 | 0.995608 | 1.007959 |
| rgba32 | 511x257 | native-owned-rgba | 2 | 1.013095 | 1.011618 | 1.014576 |
| rgba32 | 1920x1080 | native-owned-rgba | 2 | 1.134735 | 1.118283 | 1.151429 |
| rgba32-swapped | 1x1 | native-owned-rgba | 2 | 1.008975 | 0.991398 | 1.026863 |
| rgba32-swapped | 17x9 | native-owned-rgba | 2 | 1.008245 | 1.006398 | 1.010095 |
| rgba32-swapped | 511x257 | native-owned-rgba | 2 | 0.988873 | 0.987302 | 0.990446 |
| rgba32-swapped | 1920x1080 | native-owned-rgba | 2 | 0.968289 | 0.957937 | 0.978753 |
| rgba4444 | 1x1 | native-owned-rgba | 2 | 1.020372 | 1.017639 | 1.023113 |
| rgba4444 | 17x9 | native-owned-rgba | 2 | 1.011554 | 1.002069 | 1.021129 |
| rgba4444 | 511x257 | native-owned-rgba | 2 | 1.011576 | 1.006622 | 1.016555 |
| rgba4444 | 1920x1080 | native-owned-rgba | 2 | 1.003258 | 0.995874 | 1.010697 |

### Observed regressions (83)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| bgra32-zero-1920x1080-top | native-owned-rgba | 16041023.83 | 18455516.00 | 0.869172 | 15.051983 |
| core8-1920x1080-bottom | native-owned-rgba | 10365496.89 | 11342772.12 | 0.913842 | 9.428156 |
| indexed8-short-17x9-bottom | native-owned-rgba | 1409.33 | 1489.51 | 0.94617 | 5.689228 |
| bgra32-1x1-bottom | native-owned-rgba | 417.41 | 440.86 | 0.946809 | 5.617978 |
| core8-1x1-bottom | native-owned-rgba | 1494.80 | 1576.80 | 0.947996 | 5.485684 |
| indexed8-short-17x9-top | native-owned-rgba | 1412.85 | 1487.91 | 0.949553 | 5.312666 |
| indexed8-short-1920x1080-top | native-owned-rgba | 10431859.44 | 10956452.25 | 0.95212 | 5.028757 |
| indexed8-short-1x1-bottom | native-owned-rgba | 706.15 | 741.05 | 0.952905 | 4.942293 |
| rgb555-1x1-top | native-owned-rgba | 589.14 | 617.92 | 0.953424 | 4.885087 |
| rgba32-swapped-1920x1080-bottom | native-owned-rgba | 7822321.67 | 8165799.09 | 0.957937 | 4.390991 |
| rgb24-17x9-bottom | native-owned-rgba | 1315.14 | 1370.31 | 0.959739 | 4.194991 |
| indexed4-1x1-top | native-owned-rgba | 714.35 | 743.10 | 0.961311 | 4.024638 |
| rgb24-17x9-top | native-owned-rgba | 1299.00 | 1349.95 | 0.962258 | 3.922248 |
| rgb333-1x1-bottom | native-owned-rgba | 594.36 | 616.87 | 0.963509 | 3.787267 |
| bgra32-17x9-top | native-owned-rgba | 652.22 | 676.14 | 0.964623 | 3.667474 |
| bgra32-zero-1x1-bottom | native-owned-rgba | 429.20 | 444.51 | 0.965558 | 3.567102 |
| indexed4-1920x1080-top | native-owned-rgba | 10508708.78 | 10879224.78 | 0.965943 | 3.525799 |
| indexed8-1x1-bottom | native-owned-rgba | 1523.20 | 1576.86 | 0.96597 | 3.522847 |
| indexed8-1x1-top | native-owned-rgba | 1526.08 | 1576.35 | 0.96811 | 3.294061 |
| rgb565-1x1-bottom | native-owned-rgba | 584.88 | 603.49 | 0.969163 | 3.181849 |
| rgba32-1x1-bottom | native-owned-rgba | 431.46 | 445.09 | 0.969377 | 3.159041 |
| indexed4-511x257-bottom | native-owned-rgba | 288702.43 | 297583.51 | 0.970156 | 3.076205 |
| rgb333-17x9-bottom | native-owned-rgba | 1853.79 | 1908.98 | 0.971089 | 2.977144 |
| indexed4-17x9-bottom | native-owned-rgba | 1355.14 | 1395.36 | 0.971176 | 2.967959 |
| bgra32-17x9-bottom | native-owned-rgba | 644.96 | 663.42 | 0.972174 | 2.862193 |
| rgb24-1x1-bottom | native-owned-rgba | 496.60 | 510.78 | 0.972239 | 2.855417 |
| indexed1-1x1-top | native-owned-rgba | 660.60 | 678.05 | 0.974264 | 2.641538 |
| rgb333-17x9-top | native-owned-rgba | 1845.70 | 1893.64 | 0.974684 | 2.597389 |
| indexed8-short-511x257-bottom | native-owned-rgba | 286857.65 | 294109.78 | 0.975342 | 2.528128 |
| bgrx32-1920x1080-top | native-owned-rgba | 21680761.20 | 22223717.00 | 0.975569 | 2.504321 |
| rgba32-1x1-top | native-owned-rgba | 431.34 | 442.11 | 0.97564 | 2.49687 |
| indexed4-17x9-top | native-owned-rgba | 1351.76 | 1385.40 | 0.975718 | 2.488607 |
| indexed8-short-1920x1080-bottom | native-owned-rgba | 10722501.78 | 10988581.88 | 0.975786 | 2.481511 |
| indexed8-short-511x257-top | native-owned-rgba | 279883.77 | 286669.11 | 0.97633 | 2.424342 |
| bgra32-511x257-bottom | native-owned-rgba | 175128.34 | 179342.56 | 0.976502 | 2.406361 |
| rgb555-17x9-top | native-owned-rgba | 1753.02 | 1795.08 | 0.976569 | 2.399288 |
| bgra32-zero-17x9-top | native-owned-rgba | 1008.16 | 1032.10 | 0.976805 | 2.374623 |
| rgb24-1920x1080-bottom | native-owned-rgba | 21406869.20 | 21901490.60 | 0.977416 | 2.310573 |
| indexed4-511x257-top | native-owned-rgba | 268572.31 | 274707.69 | 0.977666 | 2.284443 |
| indexed1-1x1-bottom | native-owned-rgba | 681.33 | 696.84 | 0.977742 | 2.27643 |
| rgb24-511x257-top | native-owned-rgba | 779870.71 | 797594.43 | 0.977779 | 2.272649 |
| indexed8-17x9-bottom | native-owned-rgba | 2116.61 | 2164.51 | 0.97787 | 2.263053 |
| rgba32-swapped-1920x1080-top | native-owned-rgba | 7810139.33 | 7979683.09 | 0.978753 | 2.170816 |
| core24-511x257-bottom | native-owned-rgba | 796620.38 | 813780.56 | 0.978913 | 2.154123 |
| bgra32-zero-17x9-bottom | native-owned-rgba | 1030.85 | 1052.48 | 0.979449 | 2.098268 |
| core8-511x257-bottom | native-owned-rgba | 265321.93 | 270757.92 | 0.979923 | 2.048828 |
| indexed1-511x257-top | native-owned-rgba | 556186.92 | 567503.53 | 0.980059 | 2.034677 |
| bgra32-zero-511x257-bottom | native-owned-rgba | 472532.05 | 481907.47 | 0.980545 | 1.984081 |
| rgb555-17x9-bottom | native-owned-rgba | 1758.22 | 1792.36 | 0.980952 | 1.941737 |
| indexed1-17x9-top | native-owned-rgba | 1366.87 | 1393.24 | 0.981073 | 1.929225 |
| rgb555-511x257-bottom | native-owned-rgba | 1006766.68 | 1025508.99 | 0.981724 | 1.861634 |
| rgb24-511x257-bottom | native-owned-rgba | 798562.59 | 810546.86 | 0.985215 | 1.50073 |
| rgb555-1x1-bottom | native-owned-rgba | 591.93 | 600.41 | 0.985876 | 1.432602 |
| rgb565-17x9-top | native-owned-rgba | 1697.75 | 1721.48 | 0.986215 | 1.397732 |
| indexed8-511x257-bottom | native-owned-rgba | 266217.57 | 269910.03 | 0.98632 | 1.387008 |
| bgra32-1x1-top | native-owned-rgba | 424.24 | 429.89 | 0.986857 | 1.331793 |
| indexed4-1x1-bottom | native-owned-rgba | 729.26 | 738.93 | 0.986914 | 1.326002 |
| rgb555-1920x1080-bottom | native-owned-rgba | 24994520.75 | 25321191.25 | 0.987099 | 1.306968 |
| indexed8-1920x1080-top | native-owned-rgba | 10515781.56 | 10653188.89 | 0.987102 | 1.306677 |
| rgba32-swapped-511x257-bottom | native-owned-rgba | 158136.03 | 160169.79 | 0.987302 | 1.286083 |
| indexed8-511x257-top | native-owned-rgba | 266631.09 | 269918.09 | 0.987822 | 1.232789 |
| rgb24-1920x1080-top | native-owned-rgba | 21792662.20 | 22026276.25 | 0.989394 | 1.071985 |
| bgra32-511x257-top | native-owned-rgba | 176963.46 | 178848.69 | 0.989459 | 1.065322 |
| rgb565-511x257-bottom | native-owned-rgba | 999455.82 | 1009712.45 | 0.989842 | 1.026221 |
| rgb555-511x257-top | native-owned-rgba | 1007634.69 | 1017816.52 | 0.989996 | 1.010468 |
| indexed8-17x9-top | native-owned-rgba | 2129.13 | 2150.55 | 0.99004 | 1.006045 |
| rgba32-swapped-511x257-top | native-owned-rgba | 156365.54 | 157873.81 | 0.990446 | 0.96458 |
| rgb24-1x1-top | native-owned-rgba | 509.51 | 514.31 | 0.990667 | 0.942082 |
| rgb333-1920x1080-bottom | native-owned-rgba | 27093323.50 | 27347121.50 | 0.990719 | 0.936755 |
| rgb333-1x1-top | native-owned-rgba | 594.36 | 599.91 | 0.990749 | 0.933778 |
| rgb565-511x257-top | native-owned-rgba | 1005317.49 | 1014509.02 | 0.99094 | 0.914291 |
| core8-17x9-bottom | native-owned-rgba | 2140.01 | 2159.09 | 0.991163 | 0.891585 |
| rgba32-swapped-1x1-top | native-owned-rgba | 426.44 | 430.14 | 0.991398 | 0.867648 |
| rgb565-1920x1080-top | native-owned-rgba | 25006427.75 | 25185594.50 | 0.992886 | 0.716483 |
| rgb333-511x257-bottom | native-owned-rgba | 1132205.69 | 1137929.12 | 0.99497 | 0.505512 |
| rgb555-1920x1080-top | native-owned-rgba | 25558745.00 | 25671621.75 | 0.995603 | 0.441637 |
| rgba32-17x9-top | native-owned-rgba | 666.40 | 669.34 | 0.995608 | 0.441176 |
| indexed1-1920x1080-bottom | native-owned-rgba | 12588199.57 | 12641381.57 | 0.995793 | 0.422475 |
| rgba4444-1920x1080-top | native-owned-rgba | 16891402.83 | 16961386.33 | 0.995874 | 0.414314 |
| indexed8-1920x1080-bottom | native-owned-rgba | 10580144.11 | 10600801.44 | 0.998051 | 0.195246 |
| core24-1920x1080-bottom | native-owned-rgba | 21701394.60 | 21729183.00 | 0.998721 | 0.128049 |
| indexed4-1920x1080-bottom | native-owned-rgba | 10600874.33 | 10604094.00 | 0.999696 | 0.030372 |
| bgra32-zero-511x257-top | native-owned-rgba | 476143.22 | 476207.63 | 0.999865 | 0.013527 |
