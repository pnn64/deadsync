# BMP reduced-candidate alpha controls

Candidate 5 restores the original post-conversion alpha scan/repair exactly. All 18 files are unchanged-path controls; no alpha optimization is claimed. Ratios are candidate/original baseline, with less than 1 meaning fewer cycles.

All four commands returned zero, with 30 clear strict-watcher checks. Runs used five 80 ms batches on CPU 2, decode AB and consuming BA. Six all-zero files admit exact image comparisons; twelve mixed-alpha policy differences remain explicit.

## Results and limitations

| Operation | Native geometric ratio | Native cycle-sum ratio | All-zero ratio | First-alpha ratio | Last-alpha ratio | Image ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| decode | 1.013087 | 1.038002 | 1.062750 | 0.942269 | 1.038326 | — |
| owned | 1.039810 | 1.014733 | 1.019483 | 1.131133 | 0.974918 | 0.991254 |

All-zero consuming cases are 1.006214–1.044983, geometric ratio 1.019483 versus image 0.991254. All-zero decode cases are 1.024876–1.114947, geometric ratio 1.062750. These losses remain in the report despite restored alpha source.

The 511×257 first-alpha cases again change regimes in opposite directions: bottom decode 0.277768 versus consuming 3.729028; top decode 2.086987 versus consuming 0.524566. The overall geometric alpha ratios therefore cannot be attributed to an algorithmic alpha change. The cause is unestablished; no cases are removed and no best repetition is selected.

The reduced-candidate selection recommendation is recorded in the full-matrix comparison and experiment ledger. These data remain valid evidence even if candidate 5 is rejected.

## Every case

### decode

| Case | Mode | Before median | After median | Ratio | Before range | After range |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| bgra32-first-17x9-bottom.bmp | native-decode | 610.93 | 628.11 | 1.028121 | 594.55–634.47 | 602.34–638.51 |
| bgra32-first-17x9-top.bmp | native-decode | 612.43 | 622.31 | 1.016132 | 591.90–629.97 | 614.99–639.34 |
| bgra32-first-1920x1080-bottom.bmp | native-decode | 7633475.43 | 7989540.77 | 1.046645 | 7558491.71–7731066.87 | 7936328.00–9326138.95 |
| bgra32-first-1920x1080-top.bmp | native-decode | 7707790.13 | 8511060.38 | 1.104215 | 7589093.46–9017345.25 | 8189834.36–10147180.22 |
| bgra32-first-511x257-bottom.bmp | native-decode | 647045.64 | 179728.44 | 0.277768 | 639604.16–650623.01 | 179472.06–183711.21 |
| bgra32-first-511x257-top.bmp | native-decode | 333592.61 | 696203.41 | 2.086987 | 237077.11–343811.18 | 395817.78–700900.40 |
| bgra32-last-17x9-bottom.bmp | native-decode | 850.93 | 844.45 | 0.992385 | 812.62–855.61 | 815.32–847.86 |
| bgra32-last-17x9-top.bmp | native-decode | 859.14 | 809.38 | 0.942082 | 843.40–869.30 | 801.78–836.08 |
| bgra32-last-1920x1080-bottom.bmp | native-decode | 10204945.00 | 10093146.56 | 0.989045 | 10070538.33–10499214.65 | 9916205.17–10626666.53 |
| bgra32-last-1920x1080-top.bmp | native-decode | 10401717.71 | 10361110.06 | 0.996096 | 10267904.83–10421562.94 | 10297803.59–10690269.65 |
| bgra32-last-511x257-bottom.bmp | native-decode | 445948.27 | 511315.03 | 1.146579 | 390807.02–453348.14 | 494917.61–518841.21 |
| bgra32-last-511x257-top.bmp | native-decode | 397832.77 | 472077.94 | 1.186624 | 378628.03–403480.40 | 470672.80–490833.71 |
| bgra32-zero-17x9-bottom.bmp | native-decode | 971.20 | 995.36 | 1.024876 | 946.59–1001.50 | 985.45–1004.32 |
| bgra32-zero-17x9-top.bmp | native-decode | 933.61 | 999.52 | 1.070597 | 928.03–951.83 | 994.24–1008.35 |
| bgra32-zero-1920x1080-bottom.bmp | native-decode | 12485745.40 | 13114419.36 | 1.050351 | 12177655.33–12677379.07 | 12611263.00–13509195.23 |
| bgra32-zero-1920x1080-top.bmp | native-decode | 12371283.27 | 13030993.79 | 1.053326 | 12187722.73–12531004.36 | 12305257.53–14389862.77 |
| bgra32-zero-511x257-bottom.bmp | native-decode | 483194.66 | 538736.65 | 1.114947 | 479179.84–578391.89 | 535021.21–541572.08 |
| bgra32-zero-511x257-top.bmp | native-decode | 483298.44 | 514460.14 | 1.064477 | 478384.74–486786.38 | 504925.46–546454.86 |

### owned

| Case | Mode | Before median | After median | Ratio | Before range | After range |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| bgra32-first-17x9-bottom.bmp | native-owned-rgba | 633.63 | 670.04 | 1.057463 | 629.93–643.57 | 665.04–689.87 |
| bgra32-first-17x9-top.bmp | native-owned-rgba | 635.25 | 644.80 | 1.015033 | 629.21–659.26 | 636.40–683.23 |
| bgra32-first-1920x1080-bottom.bmp | native-owned-rgba | 7623206.26 | 7618835.39 | 0.999427 | 7511847.83–7821195.35 | 7565423.96–7767093.30 |
| bgra32-first-1920x1080-top.bmp | native-owned-rgba | 7772695.61 | 7758208.48 | 0.998136 | 7634524.91–7922737.83 | 7639430.26–7865429.39 |
| bgra32-first-511x257-bottom.bmp | native-owned-rgba | 177119.91 | 660485.18 | 3.729028 | 174455.25–178410.89 | 650846.54–662321.32 |
| bgra32-first-511x257-top.bmp | native-owned-rgba | 680677.82 | 357060.12 | 0.524566 | 342067.17–689464.07 | 246152.49–364685.08 |
| bgra32-last-17x9-bottom.bmp | native-owned-rgba | 861.03 | 885.17 | 1.028036 | 846.08–871.89 | 864.76–892.17 |
| bgra32-last-17x9-top.bmp | native-owned-rgba | 867.75 | 880.83 | 1.015073 | 852.17–893.14 | 864.98–974.84 |
| bgra32-last-1920x1080-bottom.bmp | native-owned-rgba | 10388005.59 | 10385397.18 | 0.999749 | 10264503.17–11233475.06 | 10193836.17–11089139.94 |
| bgra32-last-1920x1080-top.bmp | native-owned-rgba | 10150106.94 | 10302929.71 | 1.015056 | 10122718.11–10425862.71 | 10223236.78–13106894.00 |
| bgra32-last-511x257-bottom.bmp | native-owned-rgba | 521477.12 | 456259.32 | 0.874936 | 443685.23–540057.00 | 446504.15–459857.07 |
| bgra32-last-511x257-top.bmp | native-owned-rgba | 438125.29 | 406017.06 | 0.926715 | 403125.22–450910.67 | 384310.31–422731.69 |
| bgra32-zero-17x9-bottom.bmp | native-owned-rgba | 973.66 | 1002.63 | 1.029754 | 968.51–1006.50 | 990.95–1025.71 |
| bgra32-zero-17x9-bottom.bmp | image-rgba | 2823.85 | 2837.14 | 1.004706 | 2751.25–2885.22 | 2775.02–2877.07 |
| bgra32-zero-17x9-top.bmp | native-owned-rgba | 980.32 | 992.19 | 1.012108 | 952.83–992.27 | 986.90–1048.43 |
| bgra32-zero-17x9-top.bmp | image-rgba | 2726.35 | 2721.40 | 0.998184 | 2684.14–2734.66 | 2655.22–2787.89 |
| bgra32-zero-1920x1080-bottom.bmp | native-owned-rgba | 12449747.40 | 13009780.57 | 1.044983 | 12369893.33–13951166.54 | 12619901.79–13328055.93 |
| bgra32-zero-1920x1080-bottom.bmp | image-rgba | 37491346.00 | 37072891.60 | 0.988839 | 37278563.20–38862168.00 | 36395295.00–37444396.25 |
| bgra32-zero-1920x1080-top.bmp | native-owned-rgba | 12468288.00 | 12643983.71 | 1.014091 | 12197263.73–22773921.00 | 12506247.57–13026196.00 |
| bgra32-zero-1920x1080-top.bmp | image-rgba | 32176102.50 | 31523199.50 | 0.979708 | 30651053.83–74377647.33 | 30781107.67–32127337.50 |
| bgra32-zero-511x257-bottom.bmp | native-owned-rgba | 486812.24 | 489837.42 | 1.006214 | 466975.15–499250.88 | 485860.86–491176.99 |
| bgra32-zero-511x257-bottom.bmp | image-rgba | 1593654.29 | 1565722.89 | 0.982473 | 1554606.93–1712051.48 | 1528846.77–1585511.22 |
| bgra32-zero-511x257-top.bmp | native-owned-rgba | 471074.43 | 475918.94 | 1.010284 | 464109.89–476589.97 | 460624.75–480396.03 |
| bgra32-zero-511x257-top.bmp | image-rgba | 1534663.48 | 1525208.55 | 0.993839 | 1512810.48–1595443.58 | 1492876.20–1542883.77 |

Complete raw records, metadata, and the process boundary are preserved in `candidate5-baseline-alpha-summary.json` and its TSV/log/JSON/command/process sidecars.
