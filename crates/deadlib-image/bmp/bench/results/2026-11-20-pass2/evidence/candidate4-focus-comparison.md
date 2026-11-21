# BMP candidate 4 focus comparison

Parent: `candidate3-direct-rgba`. Candidate: `candidate4-alpha-dispatch`. All ratios below are **after/before**, so less than 1 means fewer cycles.

The run used five median batches of 40 ms, CPU 2, and the preserved binaries. Decode ran parent then candidate per filter; consuming ran candidate then parent. All 20 commands returned zero. The 112 strict process checks found no matching external compute process; the final boundary was clear. This does not prove absence of every background activity.

The comparison has 72 native cases in each mode and 50 exact image controls in consuming mode. Every per-case record and its before/after median and sample range is retained in `candidate4-focus-summary.json`; source TSVs and sidecars remain unchanged.

## Selection recommendation

Reject this dispatch refinement as the remedy for the earlier collateral losses. It recovers RGB555 decode, but unchanged consuming controls have geometric ratio 0.993950 versus the unchanged image reference 0.993880. That is not evidence of a broad improvement independent of reference drift. Direct-RGBA consuming regresses in all eight cases (geometric ratio 1.026648 versus image 1.003770).

The smallest defensible next gate is one rebase retaining only the exact RGBA4444 and direct-RGBA specializations, removing the logical-alpha optimization and its dispatch refinement. This recommendation reduces scope; it does not establish that alpha caused the non-target regressions. The two-specialization rebase still needs exact correctness, unchanged allocations, and a fresh original-baseline full matrix before retention. No full baseline or alpha timing ran for candidate 4 in this lease.

## Per-layout results

| Layout | Decode ratio | Decode wins / 8 | Consuming ratio | Consuming wins / 8 | Image control ratio | Image cases |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| bgrx32 | 1.019489 | 5 | 0.996559 | 5 | 0.989025 | 8 |
| rgba32 | 1.078073 | 3 | 1.004940 | 3 | 1.003587 | 8 |
| rgba32-swapped | 1.077698 | 5 | 1.026648 | 0 | 1.003770 | 8 |
| bgra32 | 1.005004 | 4 | 1.000493 | 5 | — | 0 |
| bgra32-zero | 1.026297 | 2 | 1.007979 | 3 | 0.978669 | 8 |
| rgb333 | 0.991973 | 6 | 0.979871 | 6 | 0.997739 | 8 |
| rgb555 | 0.941207 | 8 | 0.983772 | 5 | — | 0 |
| rgb565 | 0.993479 | 6 | 0.998316 | 4 | 1.009788 | 2 |
| rgba4444 | 0.955775 | 7 | 1.002871 | 5 | 0.986800 | 8 |

RGB555 and most RGB565 inputs have an established native/reference output-policy difference. They remain native before/after controls; unavailable image comparisons are not silently treated as wins.

## Unchanged controls by size

These exclude the three transformed target layouts: RGBA4444, direct RGBA, and implicit BGRA with all-zero alpha.

| Size | Native cases per mode | Decode ratio | Consuming ratio |
| --- | ---: | ---: | ---: |
| 1x1 | 12 | 0.980701 | 0.990396 |
| 17x9 | 12 | 0.989968 | 0.996206 |
| 511x257 | 12 | 1.049026 | 0.987285 |
| 1920x1080 | 12 | 0.997893 | 1.001979 |

## Same-binary parent repeat

The current focus's parent results can be compared with the earlier candidate 3 focus's candidate results: the executable hash, CLI, five batches, and 40 ms duration are identical. The table below is current/earlier for that **same binary**. This records run/context sensitivity and is not an implementation change. RGBA4444 was absent from the earlier candidate 3 focus and is excluded from this comparison.

| Layout | Same C3 decode ratio | Same C3 consuming ratio | Same image ratio |
| --- | ---: | ---: | ---: |
| bgrx32 | 1.040366 | 1.014510 | 1.027777 |
| rgba32 | 1.026675 | 1.028181 | 1.037992 |
| rgba32-swapped | 1.031372 | 1.025798 | 1.041029 |
| bgra32 | 1.045433 | 1.050323 | — |
| bgra32-zero | 1.024641 | 1.030482 | 1.035880 |
| rgb333 | 1.053106 | 1.052395 | 1.053043 |
| rgb555 | 1.059113 | 0.969052 | — |
| rgb565 | 0.988515 | 0.982515 | 0.968012 |

The same C3 binary shifts roughly 5% on some groups; RGB333 consuming and its image reference both shift about 5.3%. These repeats help bound interpretation, but do not erase any measured loss or prove the cause of the medium-size regime changes.

## Every native loss

All medians and ranges below are thread cycles. Rows are sorted by after/before ratio within mode.

### decode

| Case | Before median | After median | After/before | Before range | After range |
| --- | ---: | ---: | ---: | ---: | ---: |
| rgba32-swapped-511x257-bottom | 138715.66 | 213720.71 | 1.540711 | 131987.59–167933.29 | 168083.39–222272.83 |
| rgba32-511x257-top | 249589.58 | 357570.81 | 1.432635 | 244991.87–296336.87 | 285915.64–370280.05 |
| rgba32-swapped-511x257-top | 101404.93 | 134180.41 | 1.323214 | 99740.80–108716.34 | 123841.47–152147.42 |
| rgba32-511x257-bottom | 285363.36 | 364269.39 | 1.276511 | 283309.78–297791.06 | 351094.55–373292.67 |
| bgrx32-1920x1080-bottom | 9499762.80 | 11049355.62 | 1.163119 | 9251354.50–9831648.78 | 10718526.33–11240183.00 |
| bgrx32-511x257-bottom | 770655.36 | 836957.43 | 1.086033 | 764545.93–774436.22 | 789547.38–900224.38 |
| bgra32-zero-511x257-top | 340335.58 | 369478.85 | 1.085631 | 335218.08–346617.94 | 365569.03–370891.89 |
| bgra32-511x257-top | 182599.76 | 195853.77 | 1.072585 | 177552.48–186717.76 | 191277.22–206651.12 |
| bgra32-zero-17x9-bottom | 933.84 | 992.58 | 1.062902 | 898.08–973.89 | 953.36–1028.67 |
| bgra32-zero-511x257-bottom | 361667.84 | 382984.92 | 1.058941 | 346694.26–367532.64 | 378248.63–406847.31 |
| rgba32-swapped-1920x1080-bottom | 7658482.92 | 8025458.91 | 1.047918 | 7510576.58–7729143.50 | 7587767.42–8097235.27 |
| bgra32-511x257-bottom | 193327.04 | 201128.34 | 1.040353 | 185272.63–195832.03 | 199500.56–206563.48 |
| bgra32-zero-17x9-top | 868.24 | 896.55 | 1.032606 | 829.90–918.17 | 884.05–921.62 |
| rgb565-1920x1080-bottom | 13360056.00 | 13685731.00 | 1.024377 | 13248236.14–13389970.00 | 13482819.00–14034317.67 |
| bgra32-zero-1x1-bottom | 412.81 | 421.41 | 1.020833 | 398.36–442.58 | 408.14–424.57 |
| rgb333-1920x1080-bottom | 15786989.33 | 16097581.00 | 1.019674 | 15591743.33–16034035.67 | 15973856.67–16429831.17 |
| rgb565-1x1-bottom | 393.97 | 401.44 | 1.018961 | 391.52–400.38 | 394.70–405.13 |
| rgba32-17x9-bottom | 668.48 | 677.32 | 1.013224 | 662.76–680.57 | 662.75–703.41 |
| rgba32-1920x1080-top | 8086859.09 | 8188327.64 | 1.012547 | 7997881.64–8294485.82 | 7910981.00–8269741.82 |
| bgra32-1x1-bottom | 426.45 | 430.42 | 1.009309 | 416.27–440.18 | 416.43–434.76 |
| rgb333-17x9-top | 1415.23 | 1427.18 | 1.008444 | 1406.54–1456.52 | 1380.80–1433.92 |
| bgrx32-511x257-top | 332150.31 | 334893.82 | 1.008260 | 322375.29–361592.62 | 328414.78–362120.13 |
| rgba32-1x1-top | 430.11 | 433.19 | 1.007161 | 427.21–439.02 | 428.90–609.48 |
| bgra32-zero-1920x1080-bottom | 10943035.38 | 11005778.44 | 1.005734 | 10411885.22–11221059.88 | 10672748.44–11201304.88 |
| bgra32-17x9-bottom | 676.14 | 678.95 | 1.004156 | 670.70–691.14 | 667.46–689.76 |
| rgba4444-17x9-top | 978.02 | 980.95 | 1.002996 | 953.46–999.83 | 949.28–1033.58 |

### owned

| Case | Before median | After median | After/before | Before range | After range |
| --- | ---: | ---: | ---: | ---: | ---: |
| rgba32-swapped-1x1-top | 373.79 | 393.06 | 1.051553 | 368.20–388.77 | 382.21–409.09 |
| rgba32-swapped-17x9-top | 399.45 | 419.21 | 1.049468 | 395.31–406.95 | 410.16–431.30 |
| bgra32-zero-1x1-bottom | 430.83 | 450.56 | 1.045795 | 423.77–442.50 | 440.96–466.86 |
| rgba32-swapped-511x257-bottom | 166431.72 | 173616.23 | 1.043168 | 162234.71–211919.33 | 168460.01–224322.84 |
| bgra32-zero-17x9-bottom | 964.20 | 1005.76 | 1.043103 | 934.41–1655.20 | 991.92–1086.42 |
| bgrx32-1920x1080-bottom | 21074063.40 | 21975550.20 | 1.042777 | 20237240.80–21878772.25 | 21922123.25–22401072.25 |
| bgra32-zero-1x1-top | 424.73 | 441.76 | 1.040096 | 412.37–447.45 | 430.68–457.59 |
| bgra32-1x1-top | 436.45 | 453.80 | 1.039753 | 430.99–443.96 | 435.08–464.32 |
| rgba4444-1x1-bottom | 405.75 | 421.33 | 1.038398 | 395.42–421.48 | 413.72–437.99 |
| rgb565-511x257-bottom | 1429709.83 | 1478527.97 | 1.034145 | 1398122.31–1470687.73 | 1465193.38–1481306.05 |
| bgrx32-1x1-bottom | 603.64 | 624.08 | 1.033861 | 584.62–639.09 | 582.69–636.33 |
| bgra32-zero-17x9-top | 897.67 | 927.67 | 1.033420 | 894.72–917.39 | 902.20–950.59 |
| bgra32-1x1-bottom | 438.06 | 450.68 | 1.028809 | 433.32–451.10 | 439.88–464.60 |
| rgba32-17x9-bottom | 684.32 | 702.85 | 1.027078 | 670.35–700.02 | 686.20–714.97 |
| rgba32-swapped-17x9-bottom | 492.56 | 505.77 | 1.026819 | 485.07–538.25 | 494.77–669.51 |
| rgba4444-17x9-bottom | 999.98 | 1020.81 | 1.020830 | 987.34–1070.78 | 955.24–1138.77 |
| rgb555-1920x1080-top | 24612699.00 | 25121993.50 | 1.020692 | 24212551.00–24937175.50 | 24989032.50–25953899.50 |
| rgba32-17x9-top | 693.38 | 707.05 | 1.019715 | 692.97–734.19 | 680.43–717.94 |
| bgrx32-1920x1080-top | 21542651.60 | 21952688.50 | 1.019034 | 20863123.80–21971511.25 | 21573382.60–22389384.50 |
| rgb333-17x9-top | 2002.66 | 2040.00 | 1.018645 | 1956.59–2170.73 | 1974.29–2193.72 |
| rgba4444-1920x1080-top | 12189164.12 | 12410996.25 | 1.018199 | 11551543.88–12270022.25 | 12089730.88–12486202.38 |
| rgba32-1x1-top | 439.79 | 447.77 | 1.018145 | 433.77–451.22 | 437.39–461.54 |
| rgb565-1x1-bottom | 574.04 | 584.27 | 1.017821 | 570.53–597.87 | 579.81–589.81 |
| rgba32-swapped-1x1-bottom | 381.55 | 387.77 | 1.016302 | 375.06–383.87 | 383.28–398.94 |
| rgba32-swapped-1920x1080-top | 6635187.07 | 6741300.62 | 1.015993 | 6499206.79–8217341.91 | 6540190.43–7061602.08 |
| rgba32-511x257-top | 283747.36 | 288277.87 | 1.015967 | 281447.54–356129.02 | 281742.32–349332.54 |
| bgra32-511x257-bottom | 193614.89 | 196398.12 | 1.014375 | 190414.83–195150.90 | 193965.80–206766.94 |
| rgb555-511x257-top | 1436263.94 | 1448586.84 | 1.008580 | 1415051.34–1463807.75 | 1426331.53–1464301.69 |
| rgb555-17x9-bottom | 1780.20 | 1794.36 | 1.007954 | 1768.85–1835.07 | 1706.40–1801.60 |
| bgra32-zero-511x257-bottom | 373284.45 | 375948.63 | 1.007137 | 365253.31–382434.86 | 368453.62–398105.40 |
| rgba32-swapped-511x257-top | 125065.12 | 125928.67 | 1.006905 | 107899.58–130384.94 | 107791.72–132277.18 |
| rgb565-1x1-top | 579.77 | 583.01 | 1.005588 | 573.05–597.29 | 578.19–601.05 |
| rgb333-1920x1080-top | 28074653.00 | 28211402.50 | 1.004871 | 27725319.25–28191592.50 | 27789468.50–28712081.75 |
| rgba32-swapped-1920x1080-bottom | 7565213.83 | 7597022.92 | 1.004205 | 7502546.67–7673354.42 | 7445019.33–7872422.42 |
| rgb565-1920x1080-bottom | 25533118.50 | 25582780.00 | 1.001945 | 25202002.00–25817754.25 | 24934651.00–26400801.25 |
| rgba32-1920x1080-bottom | 8077221.18 | 8091109.27 | 1.001719 | 7677487.92–8763888.50 | 7962881.36–8328129.55 |

## Provenance

- Commands: `candidate4-focus-commands.json`.
- Process records: `candidate4-focus-processes.jsonl`.
- Timing sidecars: `candidate4-focus-{before,after}-{decode,owned}-{32,rgb333,rgb555,rgb565,rgba4444}.json`.
- Parent timing SHA-256: `762925d31da68079aca897395e08faa913f56884021b62633c7887fb44ad9fc4`.
- Candidate timing SHA-256: `fab8fef6e02991e32db9a20ce1204b3f78810fcd81072d344376707332a2ab57`.
- Candidate correctness/allocation and direct native exit logs: `candidate4-alpha-dispatch-validation-summary.json`, `candidate4-alpha-dispatch-preparation-summary.json`, and `candidate4-alpha-dispatch-validation/commands.jsonl`.

No production source was changed during this timing lease. No result selects the fastest of repeated processes; all completed medians and ranges are preserved.
