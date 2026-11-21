Inputs: `crates/deadlib-image/bmp/bench/results/windows-x86_64-20261120/baseline.tsv` -> `crates/deadlib-image/bmp/bench/results/windows-x86_64-20261120/final-all.tsv`. Complete key sets match (662 rows).

## Stage comparison: thread_cycles

480 paired measurements; geometric mean ratio **1.585723x**; minimum **0.898653x**; maximum **16.542438x**. Worst: `rgba32-swapped-1x1-bottom` / `native-decode`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| bgra32 | 1x1 | native-convert | 2 | 0.960254 | 0.946898 | 0.973799 |
| bgra32 | 1x1 | native-decode | 2 | 0.943576 | 0.926102 | 0.961379 |
| bgra32 | 1x1 | native-encode | 2 | 0.96499 | 0.950721 | 0.979472 |
| bgra32 | 1x1 | native-rgba | 2 | 0.949419 | 0.940404 | 0.95852 |
| bgra32 | 17x9 | native-convert | 2 | 1.020765 | 1.015533 | 1.026025 |
| bgra32 | 17x9 | native-decode | 2 | 4.229673 | 4.206691 | 4.252781 |
| bgra32 | 17x9 | native-encode | 2 | 0.977724 | 0.972252 | 0.983227 |
| bgra32 | 17x9 | native-rgba | 2 | 3.493186 | 3.432178 | 3.555279 |
| bgra32 | 511x257 | native-convert | 2 | 1.01651 | 1.013569 | 1.019461 |
| bgra32 | 511x257 | native-decode | 2 | 11.432184 | 11.105233 | 11.768761 |
| bgra32 | 511x257 | native-encode | 2 | 1.000602 | 0.992126 | 1.00915 |
| bgra32 | 511x257 | native-rgba | 2 | 7.713415 | 7.693062 | 7.733823 |
| bgra32 | 1920x1080 | native-convert | 2 | 1.005698 | 1.000636 | 1.010784 |
| bgra32 | 1920x1080 | native-decode | 2 | 4.578628 | 4.498867 | 4.659802 |
| bgra32 | 1920x1080 | native-encode | 2 | 1.015152 | 1.006699 | 1.023675 |
| bgra32 | 1920x1080 | native-rgba | 2 | 3.153352 | 3.14251 | 3.164231 |
| bgra32-zero | 1x1 | native-convert | 2 | 1.02179 | 1.006101 | 1.037724 |
| bgra32-zero | 1x1 | native-decode | 2 | 0.963781 | 0.963568 | 0.963994 |
| bgra32-zero | 1x1 | native-encode | 2 | 1.019005 | 1.016192 | 1.021826 |
| bgra32-zero | 1x1 | native-rgba | 2 | 0.990053 | 0.984443 | 0.995695 |
| bgra32-zero | 17x9 | native-convert | 2 | 1.065776 | 1.054797 | 1.076869 |
| bgra32-zero | 17x9 | native-decode | 2 | 2.98316 | 2.960433 | 3.006062 |
| bgra32-zero | 17x9 | native-encode | 2 | 1.030916 | 1.000649 | 1.062098 |
| bgra32-zero | 17x9 | native-rgba | 2 | 2.622165 | 2.604972 | 2.639472 |
| bgra32-zero | 511x257 | native-convert | 2 | 1.004128 | 1.003434 | 1.004822 |
| bgra32-zero | 511x257 | native-decode | 2 | 4.67434 | 4.632972 | 4.716077 |
| bgra32-zero | 511x257 | native-encode | 2 | 1.002216 | 0.983914 | 1.020859 |
| bgra32-zero | 511x257 | native-rgba | 2 | 3.971864 | 3.93636 | 4.007688 |
| bgra32-zero | 1920x1080 | native-convert | 2 | 1.501164 | 1.468605 | 1.534445 |
| bgra32-zero | 1920x1080 | native-decode | 2 | 3.365518 | 3.323158 | 3.408419 |
| bgra32-zero | 1920x1080 | native-encode | 2 | 1.355718 | 1.314814 | 1.397894 |
| bgra32-zero | 1920x1080 | native-rgba | 2 | 2.9426 | 2.896092 | 2.989855 |
| bgrx32 | 1x1 | native-convert | 2 | 1.051265 | 1.04344 | 1.059148 |
| bgrx32 | 1x1 | native-decode | 2 | 0.992214 | 0.979683 | 1.004906 |
| bgrx32 | 1x1 | native-encode | 2 | 1.050297 | 1.040145 | 1.060548 |
| bgrx32 | 1x1 | native-rgba | 2 | 1.042163 | 1.030736 | 1.053717 |
| bgrx32 | 17x9 | native-convert | 2 | 1.513759 | 1.498031 | 1.529652 |
| bgrx32 | 17x9 | native-decode | 2 | 2.976156 | 2.969211 | 2.983118 |
| bgrx32 | 17x9 | native-encode | 2 | 1.06082 | 1.031189 | 1.091303 |
| bgrx32 | 17x9 | native-rgba | 2 | 2.352636 | 2.287399 | 2.419734 |
| bgrx32 | 511x257 | native-convert | 2 | 1.592191 | 1.58371 | 1.600717 |
| bgrx32 | 511x257 | native-decode | 2 | 5.01203 | 4.956009 | 5.068684 |
| bgrx32 | 511x257 | native-encode | 2 | 1.023162 | 1.011548 | 1.034909 |
| bgrx32 | 511x257 | native-rgba | 2 | 3.107101 | 3.089578 | 3.124723 |
| bgrx32 | 1920x1080 | native-convert | 2 | 1.281248 | 1.273013 | 1.289537 |
| bgrx32 | 1920x1080 | native-decode | 2 | 3.099137 | 3.091836 | 3.106455 |
| bgrx32 | 1920x1080 | native-encode | 2 | 1.006216 | 0.998511 | 1.013981 |
| bgrx32 | 1920x1080 | native-rgba | 2 | 2.049187 | 2.044644 | 2.05374 |
| core24 | 1x1 | native-convert | 1 | 1.014125 | 1.014125 | 1.014125 |
| core24 | 1x1 | native-decode | 1 | 0.936799 | 0.936799 | 0.936799 |
| core24 | 1x1 | native-encode | 1 | 1.033099 | 1.033099 | 1.033099 |
| core24 | 1x1 | native-rgba | 1 | 0.997543 | 0.997543 | 0.997543 |
| core24 | 17x9 | native-convert | 1 | 1.50005 | 1.50005 | 1.50005 |
| core24 | 17x9 | native-decode | 1 | 1.064999 | 1.064999 | 1.064999 |
| core24 | 17x9 | native-encode | 1 | 1.03533 | 1.03533 | 1.03533 |
| core24 | 17x9 | native-rgba | 1 | 1.248733 | 1.248733 | 1.248733 |
| core24 | 511x257 | native-convert | 1 | 1.645 | 1.645 | 1.645 |
| core24 | 511x257 | native-decode | 1 | 1.2899 | 1.2899 | 1.2899 |
| core24 | 511x257 | native-encode | 1 | 1.017765 | 1.017765 | 1.017765 |
| core24 | 511x257 | native-rgba | 1 | 1.428631 | 1.428631 | 1.428631 |
| core24 | 1920x1080 | native-convert | 1 | 1.231057 | 1.231057 | 1.231057 |
| core24 | 1920x1080 | native-decode | 1 | 1.11306 | 1.11306 | 1.11306 |
| core24 | 1920x1080 | native-encode | 1 | 1.05801 | 1.05801 | 1.05801 |
| core24 | 1920x1080 | native-rgba | 1 | 1.165444 | 1.165444 | 1.165444 |
| core8 | 1x1 | native-convert | 1 | 1.037166 | 1.037166 | 1.037166 |
| core8 | 1x1 | native-decode | 1 | 1.036614 | 1.036614 | 1.036614 |
| core8 | 1x1 | native-encode | 1 | 0.992648 | 0.992648 | 0.992648 |
| core8 | 1x1 | native-rgba | 1 | 1.059524 | 1.059524 | 1.059524 |
| core8 | 17x9 | native-convert | 1 | 1.776763 | 1.776763 | 1.776763 |
| core8 | 17x9 | native-decode | 1 | 1.283858 | 1.283858 | 1.283858 |
| core8 | 17x9 | native-encode | 1 | 1.269769 | 1.269769 | 1.269769 |
| core8 | 17x9 | native-rgba | 1 | 1.416766 | 1.416766 | 1.416766 |
| core8 | 511x257 | native-convert | 1 | 4.054304 | 4.054304 | 4.054304 |
| core8 | 511x257 | native-decode | 1 | 9.64824 | 9.64824 | 9.64824 |
| core8 | 511x257 | native-encode | 1 | 1.303234 | 1.303234 | 1.303234 |
| core8 | 511x257 | native-rgba | 1 | 4.527518 | 4.527518 | 4.527518 |
| core8 | 1920x1080 | native-convert | 1 | 2.108068 | 2.108068 | 2.108068 |
| core8 | 1920x1080 | native-decode | 1 | 2.908871 | 2.908871 | 2.908871 |
| core8 | 1920x1080 | native-encode | 1 | 1.165437 | 1.165437 | 1.165437 |
| core8 | 1920x1080 | native-rgba | 1 | 2.305078 | 2.305078 | 2.305078 |
| indexed1 | 1x1 | native-convert | 2 | 1.019248 | 1.005434 | 1.033252 |
| indexed1 | 1x1 | native-decode | 2 | 0.99725 | 0.982518 | 1.012202 |
| indexed1 | 1x1 | native-encode | 2 | 0.989864 | 0.973815 | 1.006177 |
| indexed1 | 1x1 | native-rgba | 2 | 1.013137 | 1.010703 | 1.015577 |
| indexed1 | 17x9 | native-convert | 2 | 1.643338 | 1.639868 | 1.646816 |
| indexed1 | 17x9 | native-decode | 2 | 1.467504 | 1.45537 | 1.479739 |
| indexed1 | 17x9 | native-encode | 2 | 1.152484 | 1.152425 | 1.152542 |
| indexed1 | 17x9 | native-rgba | 2 | 1.574907 | 1.573692 | 1.576123 |
| indexed1 | 511x257 | native-convert | 2 | 2.986842 | 2.593986 | 3.439195 |
| indexed1 | 511x257 | native-decode | 2 | 2.889619 | 2.847568 | 2.93229 |
| indexed1 | 511x257 | native-encode | 2 | 1.235042 | 1.072951 | 1.42162 |
| indexed1 | 511x257 | native-rgba | 2 | 2.988068 | 2.943825 | 3.032975 |
| indexed1 | 1920x1080 | native-convert | 2 | 2.066239 | 2.04028 | 2.092528 |
| indexed1 | 1920x1080 | native-decode | 2 | 2.121297 | 2.113183 | 2.129442 |
| indexed1 | 1920x1080 | native-encode | 2 | 1.134325 | 1.132114 | 1.136541 |
| indexed1 | 1920x1080 | native-rgba | 2 | 2.088442 | 2.057583 | 2.119763 |
| indexed4 | 1x1 | native-convert | 2 | 1.012531 | 1.009264 | 1.015808 |
| indexed4 | 1x1 | native-decode | 2 | 1.038392 | 1.029196 | 1.04767 |
| indexed4 | 1x1 | native-encode | 2 | 0.985435 | 0.974038 | 0.996965 |
| indexed4 | 1x1 | native-rgba | 2 | 1.011968 | 1.006668 | 1.017295 |
| indexed4 | 17x9 | native-convert | 2 | 1.725175 | 1.698036 | 1.752746 |
| indexed4 | 17x9 | native-decode | 2 | 1.989623 | 1.969865 | 2.009579 |
| indexed4 | 17x9 | native-encode | 2 | 1.204235 | 1.185227 | 1.223547 |
| indexed4 | 17x9 | native-rgba | 2 | 1.845919 | 1.841988 | 1.849858 |
| indexed4 | 511x257 | native-convert | 2 | 4.009732 | 3.914858 | 4.106905 |
| indexed4 | 511x257 | native-decode | 2 | 16.33496 | 16.130084 | 16.542438 |
| indexed4 | 511x257 | native-encode | 2 | 1.380758 | 1.367467 | 1.394178 |
| indexed4 | 511x257 | native-rgba | 2 | 5.436448 | 5.259429 | 5.619424 |
| indexed4 | 1920x1080 | native-convert | 2 | 2.076195 | 2.05501 | 2.097598 |
| indexed4 | 1920x1080 | native-decode | 2 | 4.428939 | 4.425711 | 4.43217 |
| indexed4 | 1920x1080 | native-encode | 2 | 1.125551 | 1.120782 | 1.130341 |
| indexed4 | 1920x1080 | native-rgba | 2 | 2.557872 | 2.552344 | 2.563412 |
| indexed8 | 1x1 | native-convert | 2 | 1.042389 | 1.005893 | 1.08021 |
| indexed8 | 1x1 | native-decode | 2 | 1.117702 | 1.113996 | 1.121421 |
| indexed8 | 1x1 | native-encode | 2 | 1.007657 | 0.988165 | 1.027534 |
| indexed8 | 1x1 | native-rgba | 2 | 1.102838 | 1.101669 | 1.104008 |
| indexed8 | 17x9 | native-convert | 2 | 1.747753 | 1.714866 | 1.781272 |
| indexed8 | 17x9 | native-decode | 2 | 1.258491 | 1.248579 | 1.268483 |
| indexed8 | 17x9 | native-encode | 2 | 1.257909 | 1.245901 | 1.270033 |
| indexed8 | 17x9 | native-rgba | 2 | 1.423593 | 1.394058 | 1.453753 |
| indexed8 | 511x257 | native-convert | 2 | 4.024494 | 4.015288 | 4.033721 |
| indexed8 | 511x257 | native-decode | 2 | 10.136569 | 10.099325 | 10.173949 |
| indexed8 | 511x257 | native-encode | 2 | 1.304081 | 1.264585 | 1.344811 |
| indexed8 | 511x257 | native-rgba | 2 | 4.372136 | 4.347378 | 4.397035 |
| indexed8 | 1920x1080 | native-convert | 2 | 2.127153 | 2.093796 | 2.161041 |
| indexed8 | 1920x1080 | native-decode | 2 | 2.92916 | 2.928966 | 2.929354 |
| indexed8 | 1920x1080 | native-encode | 2 | 1.146481 | 1.139006 | 1.154006 |
| indexed8 | 1920x1080 | native-rgba | 2 | 2.289642 | 2.279323 | 2.300009 |
| indexed8-short | 1x1 | native-convert | 2 | 1.000088 | 0.996337 | 1.003854 |
| indexed8-short | 1x1 | native-decode | 2 | 1.021991 | 1.001156 | 1.043259 |
| indexed8-short | 1x1 | native-encode | 2 | 1.003009 | 0.981476 | 1.025014 |
| indexed8-short | 1x1 | native-rgba | 2 | 1.029279 | 1.025986 | 1.032583 |
| indexed8-short | 17x9 | native-convert | 2 | 1.64721 | 1.620245 | 1.674623 |
| indexed8-short | 17x9 | native-decode | 2 | 1.221233 | 1.194097 | 1.248986 |
| indexed8-short | 17x9 | native-encode | 2 | 1.196659 | 1.189125 | 1.20424 |
| indexed8-short | 17x9 | native-rgba | 2 | 1.380161 | 1.33942 | 1.422141 |
| indexed8-short | 511x257 | native-convert | 2 | 3.681665 | 3.637485 | 3.726382 |
| indexed8-short | 511x257 | native-decode | 2 | 6.291185 | 6.17609 | 6.408426 |
| indexed8-short | 511x257 | native-encode | 2 | 1.231442 | 1.198913 | 1.264853 |
| indexed8-short | 511x257 | native-rgba | 2 | 4.040917 | 3.997685 | 4.084617 |
| indexed8-short | 1920x1080 | native-convert | 2 | 2.065445 | 2.050626 | 2.08037 |
| indexed8-short | 1920x1080 | native-decode | 2 | 2.754544 | 2.740162 | 2.769001 |
| indexed8-short | 1920x1080 | native-encode | 2 | 1.17894 | 1.148506 | 1.21018 |
| indexed8-short | 1920x1080 | native-rgba | 2 | 2.235746 | 2.216315 | 2.255348 |
| rgb24 | 1x1 | native-convert | 2 | 1.028219 | 0.999011 | 1.05828 |
| rgb24 | 1x1 | native-decode | 2 | 0.956112 | 0.933989 | 0.978759 |
| rgb24 | 1x1 | native-encode | 2 | 1.067671 | 1.049837 | 1.085808 |
| rgb24 | 1x1 | native-rgba | 2 | 1.003474 | 1.000734 | 1.006221 |
| rgb24 | 17x9 | native-convert | 2 | 1.448021 | 1.428332 | 1.467982 |
| rgb24 | 17x9 | native-decode | 2 | 1.050519 | 1.037272 | 1.063934 |
| rgb24 | 17x9 | native-encode | 2 | 1.029612 | 1.023837 | 1.03542 |
| rgb24 | 17x9 | native-rgba | 2 | 1.263175 | 1.258734 | 1.267632 |
| rgb24 | 511x257 | native-convert | 2 | 1.598157 | 1.58926 | 1.607104 |
| rgb24 | 511x257 | native-decode | 2 | 1.297648 | 1.264732 | 1.33142 |
| rgb24 | 511x257 | native-encode | 2 | 1.028852 | 1.016573 | 1.041279 |
| rgb24 | 511x257 | native-rgba | 2 | 1.468282 | 1.468208 | 1.468357 |
| rgb24 | 1920x1080 | native-convert | 2 | 1.286268 | 1.274159 | 1.298492 |
| rgb24 | 1920x1080 | native-decode | 2 | 1.197048 | 1.186675 | 1.207513 |
| rgb24 | 1920x1080 | native-encode | 2 | 1.025423 | 1.015902 | 1.035035 |
| rgb24 | 1920x1080 | native-rgba | 2 | 1.267676 | 1.233668 | 1.302622 |
| rgb333 | 1x1 | native-convert | 2 | 1.025296 | 1.015903 | 1.034776 |
| rgb333 | 1x1 | native-decode | 2 | 0.944864 | 0.923045 | 0.967199 |
| rgb333 | 1x1 | native-encode | 2 | 1.049198 | 1.023761 | 1.075266 |
| rgb333 | 1x1 | native-rgba | 2 | 0.985344 | 0.978945 | 0.991784 |
| rgb333 | 17x9 | native-convert | 2 | 1.499969 | 1.480036 | 1.520171 |
| rgb333 | 17x9 | native-decode | 2 | 1.625517 | 1.623595 | 1.627442 |
| rgb333 | 17x9 | native-encode | 2 | 1.049718 | 1.036964 | 1.06263 |
| rgb333 | 17x9 | native-rgba | 2 | 1.593654 | 1.580498 | 1.60692 |
| rgb333 | 511x257 | native-convert | 2 | 1.6184 | 1.609379 | 1.627471 |
| rgb333 | 511x257 | native-decode | 2 | 1.843643 | 1.842292 | 1.844994 |
| rgb333 | 511x257 | native-encode | 2 | 1.014784 | 1.003619 | 1.026073 |
| rgb333 | 511x257 | native-rgba | 2 | 1.772137 | 1.757822 | 1.786569 |
| rgb333 | 1920x1080 | native-convert | 2 | 1.252529 | 1.238718 | 1.266494 |
| rgb333 | 1920x1080 | native-decode | 2 | 1.625236 | 1.612192 | 1.638386 |
| rgb333 | 1920x1080 | native-encode | 2 | 1.015728 | 0.996735 | 1.035083 |
| rgb333 | 1920x1080 | native-rgba | 2 | 1.471689 | 1.46245 | 1.480986 |
| rgb555 | 1x1 | native-convert | 2 | 1.038872 | 1.036419 | 1.041331 |
| rgb555 | 1x1 | native-decode | 2 | 0.941971 | 0.933544 | 0.950473 |
| rgb555 | 1x1 | native-encode | 2 | 1.030604 | 0.995896 | 1.066522 |
| rgb555 | 1x1 | native-rgba | 2 | 1.001015 | 0.974111 | 1.028662 |
| rgb555 | 17x9 | native-convert | 2 | 1.554764 | 1.512511 | 1.598198 |
| rgb555 | 17x9 | native-decode | 2 | 1.851004 | 1.823586 | 1.878835 |
| rgb555 | 17x9 | native-encode | 2 | 1.063603 | 1.041868 | 1.085792 |
| rgb555 | 17x9 | native-rgba | 2 | 1.75273 | 1.737682 | 1.767909 |
| rgb555 | 511x257 | native-convert | 2 | 1.642277 | 1.608376 | 1.676894 |
| rgb555 | 511x257 | native-decode | 2 | 2.192776 | 2.151955 | 2.234371 |
| rgb555 | 511x257 | native-encode | 2 | 1.081238 | 1.07685 | 1.085643 |
| rgb555 | 511x257 | native-rgba | 2 | 1.994983 | 1.972535 | 2.017687 |
| rgb555 | 1920x1080 | native-convert | 2 | 1.295364 | 1.289216 | 1.301542 |
| rgb555 | 1920x1080 | native-decode | 2 | 1.848652 | 1.825362 | 1.872239 |
| rgb555 | 1920x1080 | native-encode | 2 | 1.05232 | 1.042138 | 1.062601 |
| rgb555 | 1920x1080 | native-rgba | 2 | 1.587203 | 1.572817 | 1.60172 |
| rgb565 | 1x1 | native-convert | 2 | 1.054791 | 1.051831 | 1.05776 |
| rgb565 | 1x1 | native-decode | 2 | 0.96448 | 0.963493 | 0.965468 |
| rgb565 | 1x1 | native-encode | 2 | 1.051811 | 1.037004 | 1.06683 |
| rgb565 | 1x1 | native-rgba | 2 | 1.003457 | 1.001068 | 1.005851 |
| rgb565 | 17x9 | native-convert | 2 | 1.494412 | 1.487256 | 1.501603 |
| rgb565 | 17x9 | native-decode | 2 | 1.842221 | 1.828561 | 1.855983 |
| rgb565 | 17x9 | native-encode | 2 | 1.052311 | 1.041534 | 1.0632 |
| rgb565 | 17x9 | native-rgba | 2 | 1.687827 | 1.686706 | 1.688948 |
| rgb565 | 511x257 | native-convert | 2 | 1.611455 | 1.610203 | 1.612707 |
| rgb565 | 511x257 | native-decode | 2 | 2.238264 | 2.219164 | 2.257528 |
| rgb565 | 511x257 | native-encode | 2 | 1.032361 | 1.028293 | 1.036445 |
| rgb565 | 511x257 | native-rgba | 2 | 1.974094 | 1.958751 | 1.989559 |
| rgb565 | 1920x1080 | native-convert | 2 | 1.261138 | 1.212874 | 1.311322 |
| rgb565 | 1920x1080 | native-decode | 2 | 1.882887 | 1.874153 | 1.891663 |
| rgb565 | 1920x1080 | native-encode | 2 | 1.023664 | 1.003128 | 1.044621 |
| rgb565 | 1920x1080 | native-rgba | 2 | 1.605547 | 1.59451 | 1.616661 |
| rgba32 | 1x1 | native-convert | 2 | 1.02255 | 1.004807 | 1.040607 |
| rgba32 | 1x1 | native-decode | 2 | 0.963324 | 0.919093 | 1.009685 |
| rgba32 | 1x1 | native-encode | 2 | 0.98743 | 0.985565 | 0.989299 |
| rgba32 | 1x1 | native-rgba | 2 | 0.993779 | 0.98335 | 1.004318 |
| rgba32 | 17x9 | native-convert | 2 | 1.054466 | 1.039666 | 1.069477 |
| rgba32 | 17x9 | native-decode | 2 | 4.332554 | 4.31418 | 4.351005 |
| rgba32 | 17x9 | native-encode | 2 | 0.9919 | 0.985971 | 0.997866 |
| rgba32 | 17x9 | native-rgba | 2 | 3.552185 | 3.520031 | 3.584633 |
| rgba32 | 511x257 | native-convert | 2 | 0.998651 | 0.99813 | 0.999173 |
| rgba32 | 511x257 | native-decode | 2 | 11.345814 | 11.219443 | 11.473608 |
| rgba32 | 511x257 | native-encode | 2 | 0.99513 | 0.990937 | 0.99934 |
| rgba32 | 511x257 | native-rgba | 2 | 3.717133 | 1.778752 | 7.76785 |
| rgba32 | 1920x1080 | native-convert | 2 | 1.341947 | 1.329031 | 1.354989 |
| rgba32 | 1920x1080 | native-decode | 2 | 4.71731 | 4.708747 | 4.725889 |
| rgba32 | 1920x1080 | native-encode | 2 | 1.176641 | 1.17113 | 1.182177 |
| rgba32 | 1920x1080 | native-rgba | 2 | 3.509835 | 3.459114 | 3.5613 |
| rgba32-swapped | 1x1 | native-convert | 2 | 1.006802 | 0.993151 | 1.020641 |
| rgba32-swapped | 1x1 | native-decode | 2 | 0.925437 | 0.898653 | 0.953019 |
| rgba32-swapped | 1x1 | native-encode | 2 | 0.963088 | 0.959187 | 0.967005 |
| rgba32-swapped | 1x1 | native-rgba | 2 | 0.959018 | 0.953752 | 0.964312 |
| rgba32-swapped | 17x9 | native-convert | 2 | 1.000487 | 0.980568 | 1.020811 |
| rgba32-swapped | 17x9 | native-decode | 2 | 4.623823 | 4.604203 | 4.643527 |
| rgba32-swapped | 17x9 | native-encode | 2 | 1.013989 | 0.998595 | 1.02962 |
| rgba32-swapped | 17x9 | native-rgba | 2 | 3.689145 | 3.666301 | 3.712131 |
| rgba32-swapped | 511x257 | native-convert | 2 | 1.043901 | 1.022543 | 1.065705 |
| rgba32-swapped | 511x257 | native-decode | 2 | 12.772504 | 12.654139 | 12.891976 |
| rgba32-swapped | 511x257 | native-encode | 2 | 1.006268 | 1.001561 | 1.010997 |
| rgba32-swapped | 511x257 | native-rgba | 2 | 8.206218 | 8.171155 | 8.241431 |
| rgba32-swapped | 1920x1080 | native-convert | 2 | 0.992369 | 0.982011 | 1.002836 |
| rgba32-swapped | 1920x1080 | native-decode | 2 | 4.562002 | 4.547929 | 4.576118 |
| rgba32-swapped | 1920x1080 | native-encode | 2 | 0.99494 | 0.992797 | 0.997088 |
| rgba32-swapped | 1920x1080 | native-rgba | 2 | 3.067865 | 3.047859 | 3.088002 |
| rgba4444 | 1x1 | native-convert | 2 | 1.060523 | 1.036506 | 1.085096 |
| rgba4444 | 1x1 | native-decode | 2 | 0.957808 | 0.951253 | 0.964409 |
| rgba4444 | 1x1 | native-encode | 2 | 1.068476 | 1.066481 | 1.070474 |
| rgba4444 | 1x1 | native-rgba | 2 | 0.989192 | 0.985345 | 0.993055 |
| rgba4444 | 17x9 | native-convert | 2 | 1.117495 | 1.100195 | 1.135068 |
| rgba4444 | 17x9 | native-decode | 2 | 2.054101 | 2.002488 | 2.107044 |
| rgba4444 | 17x9 | native-encode | 2 | 1.072806 | 1.045724 | 1.10059 |
| rgba4444 | 17x9 | native-rgba | 2 | 1.897607 | 1.873059 | 1.922478 |
| rgba4444 | 511x257 | native-convert | 2 | 1.037287 | 1.015616 | 1.059421 |
| rgba4444 | 511x257 | native-decode | 2 | 2.509538 | 2.467121 | 2.552684 |
| rgba4444 | 511x257 | native-encode | 2 | 1.021521 | 1.013986 | 1.029112 |
| rgba4444 | 511x257 | native-rgba | 2 | 2.330642 | 2.301773 | 2.359873 |
| rgba4444 | 1920x1080 | native-convert | 2 | 1.029672 | 1.019981 | 1.039456 |
| rgba4444 | 1920x1080 | native-decode | 2 | 2.075609 | 2.053235 | 2.098227 |
| rgba4444 | 1920x1080 | native-encode | 2 | 1.018248 | 1.015229 | 1.021275 |
| rgba4444 | 1920x1080 | native-rgba | 2 | 1.816947 | 1.812264 | 1.821642 |

### Observed regressions (68)

Every increase is listed, including changes that may be measurement noise.

| Case | Mode | Before | After | Before/after | Increase % |
| --- | --- | --- | --- | --- | --- |
| rgba32-swapped-1x1-bottom | native-decode | 368.96 | 410.57 | 0.898653 | 11.277645 |
| rgba32-1x1-top | native-decode | 374.08 | 407.01 | 0.919093 | 8.80293 |
| rgb333-1x1-top | native-decode | 360.68 | 390.75 | 0.923045 | 8.33703 |
| bgra32-1x1-top | native-decode | 382.48 | 413.00 | 0.926102 | 7.979502 |
| rgb555-1x1-bottom | native-decode | 356.81 | 382.21 | 0.933544 | 7.118635 |
| rgb24-1x1-top | native-decode | 277.46 | 297.07 | 0.933989 | 7.067685 |
| core24-1x1-bottom | native-decode | 269.77 | 287.97 | 0.936799 | 6.746488 |
| bgra32-1x1-bottom | native-rgba | 561.12 | 596.68 | 0.940404 | 6.337325 |
| bgra32-1x1-bottom | native-convert | 216.12 | 228.24 | 0.946898 | 5.607996 |
| rgb555-1x1-top | native-decode | 364.44 | 383.43 | 0.950473 | 5.210734 |
| bgra32-1x1-bottom | native-encode | 278.78 | 293.23 | 0.950721 | 5.183299 |
| rgba4444-1x1-top | native-decode | 397.31 | 417.67 | 0.951253 | 5.124462 |
| rgba32-swapped-1x1-top | native-decode | 383.39 | 402.29 | 0.953019 | 4.929706 |
| rgba32-swapped-1x1-top | native-rgba | 561.97 | 589.22 | 0.953752 | 4.849013 |
| bgra32-1x1-top | native-rgba | 575.16 | 600.05 | 0.95852 | 4.327491 |
| rgba32-swapped-1x1-bottom | native-encode | 274.97 | 286.67 | 0.959187 | 4.25501 |
| bgra32-1x1-bottom | native-decode | 395.05 | 410.92 | 0.961379 | 4.017213 |
| rgb565-1x1-top | native-decode | 366.06 | 379.93 | 0.963493 | 3.788996 |
| bgra32-zero-1x1-bottom | native-decode | 380.86 | 395.26 | 0.963568 | 3.780917 |
| bgra32-zero-1x1-top | native-decode | 380.18 | 394.38 | 0.963994 | 3.735073 |
| rgba32-swapped-1x1-bottom | native-rgba | 560.95 | 581.71 | 0.964312 | 3.700865 |
| rgba4444-1x1-bottom | native-decode | 395.07 | 409.65 | 0.964409 | 3.690485 |
| rgb565-1x1-bottom | native-decode | 364.02 | 377.04 | 0.965468 | 3.576727 |
| rgba32-swapped-1x1-top | native-encode | 278.42 | 287.92 | 0.967005 | 3.412111 |
| rgb333-1x1-bottom | native-decode | 369.47 | 382.00 | 0.967199 | 3.391344 |
| bgra32-17x9-top | native-encode | 636.29 | 654.45 | 0.972252 | 2.854045 |
| bgra32-1x1-top | native-convert | 218.54 | 224.42 | 0.973799 | 2.690583 |
| indexed1-1x1-top | native-encode | 281.53 | 289.10 | 0.973815 | 2.688879 |
| indexed4-1x1-bottom | native-encode | 283.63 | 291.19 | 0.974038 | 2.665444 |
| rgb555-1x1-bottom | native-rgba | 564.77 | 579.78 | 0.974111 | 2.657719 |
| rgb24-1x1-bottom | native-decode | 283.38 | 289.53 | 0.978759 | 2.170231 |
| rgb333-1x1-top | native-rgba | 575.13 | 587.50 | 0.978945 | 2.150818 |
| bgra32-1x1-top | native-encode | 277.70 | 283.52 | 0.979472 | 2.095787 |
| bgrx32-1x1-bottom | native-decode | 376.11 | 383.91 | 0.979683 | 2.073861 |
| rgba32-swapped-17x9-bottom | native-convert | 231.11 | 235.69 | 0.980568 | 1.98174 |
| indexed8-short-1x1-top | native-encode | 288.23 | 293.67 | 0.981476 | 1.887382 |
| rgba32-swapped-1920x1080-top | native-convert | 6538999.00 | 6658783.43 | 0.982011 | 1.831847 |
| indexed1-1x1-top | native-decode | 477.16 | 485.65 | 0.982518 | 1.779277 |
| bgra32-17x9-bottom | native-encode | 645.98 | 657.00 | 0.983227 | 1.705935 |
| rgba32-1x1-top | native-rgba | 571.71 | 581.39 | 0.98335 | 1.693166 |
| bgra32-zero-511x257-top | native-encode | 382312.30 | 388562.56 | 0.983914 | 1.634857 |
| bgra32-zero-1x1-top | native-rgba | 576.46 | 585.57 | 0.984443 | 1.580335 |
| rgba4444-1x1-bottom | native-rgba | 586.28 | 595.00 | 0.985345 | 1.487344 |
| rgba32-1x1-top | native-encode | 277.20 | 281.26 | 0.985565 | 1.464646 |
| rgba32-17x9-top | native-encode | 650.80 | 660.06 | 0.985971 | 1.422864 |
| indexed8-1x1-top | native-encode | 281.39 | 284.76 | 0.988165 | 1.197626 |
| rgba32-1x1-bottom | native-encode | 275.49 | 278.47 | 0.989299 | 1.081709 |
| rgba32-511x257-bottom | native-encode | 381997.67 | 385491.38 | 0.990937 | 0.914589 |
| rgb333-1x1-bottom | native-rgba | 560.14 | 564.78 | 0.991784 | 0.828364 |
| bgra32-511x257-bottom | native-encode | 376588.83 | 379577.68 | 0.992126 | 0.793664 |
| core8-1x1-bottom | native-encode | 295.68 | 297.87 | 0.992648 | 0.740666 |
| rgba32-swapped-1920x1080-bottom | native-encode | 11193950.50 | 11275165.75 | 0.992797 | 0.725528 |
| rgba4444-1x1-top | native-rgba | 589.10 | 593.22 | 0.993055 | 0.699372 |
| rgba32-swapped-1x1-bottom | native-convert | 216.07 | 217.56 | 0.993151 | 0.689591 |
| bgra32-zero-1x1-bottom | native-rgba | 566.71 | 569.16 | 0.995695 | 0.43232 |
| rgb555-1x1-bottom | native-encode | 279.06 | 280.21 | 0.995896 | 0.412098 |
| indexed8-short-1x1-top | native-convert | 231.19 | 232.04 | 0.996337 | 0.367663 |
| rgb333-1920x1080-top | native-encode | 10440123.00 | 10474320.60 | 0.996735 | 0.327559 |
| indexed4-1x1-top | native-encode | 282.53 | 283.39 | 0.996965 | 0.304392 |
| rgba32-swapped-1920x1080-top | native-encode | 11140502.25 | 11173043.50 | 0.997088 | 0.292099 |
| core24-1x1-bottom | native-rgba | 479.08 | 480.26 | 0.997543 | 0.246305 |
| rgba32-17x9-bottom | native-encode | 640.48 | 641.85 | 0.997866 | 0.213902 |
| rgba32-511x257-top | native-convert | 93070.79 | 93245.18 | 0.99813 | 0.187374 |
| bgrx32-1920x1080-top | native-encode | 10331733.00 | 10347142.00 | 0.998511 | 0.149142 |
| rgba32-swapped-17x9-top | native-encode | 639.72 | 640.62 | 0.998595 | 0.140687 |
| rgb24-1x1-top | native-convert | 222.25 | 222.47 | 0.999011 | 0.098988 |
| rgba32-511x257-bottom | native-convert | 94892.60 | 94971.12 | 0.999173 | 0.082746 |
| rgba32-511x257-top | native-encode | 382193.39 | 382445.83 | 0.99934 | 0.06605 |

## Equivalent RGBA comparison: native-rgba, thread_cycles

98 exact-output cases; 22 non-equivalent native cases excluded. Image/native geometric mean **2.193693x**; worst **1.025106x**; best **9.570183x**; **0 native regressions versus image**. A ratio below 1 means native is slower or uses more allocations/bytes.

| Case | Pixels | Native | Image | Image/native | Native result |
| --- | --- | --- | --- | --- | --- |
| bgra32-zero-17x9-bottom | 153 | 1190.70 | 2934.01 | 2.464105 | faster |
| bgra32-zero-17x9-top | 153 | 1190.42 | 2741.25 | 2.302759 | faster |
| bgra32-zero-1920x1080-bottom | 2073600 | 18881024.33 | 37641176.50 | 1.993598 | faster |
| bgra32-zero-1920x1080-top | 2073600 | 18586017.00 | 32070816.50 | 1.725535 | faster |
| bgra32-zero-1x1-bottom | 1 | 569.16 | 767.03 | 1.347653 | faster |
| bgra32-zero-1x1-top | 1 | 585.57 | 738.34 | 1.260891 | faster |
| bgra32-zero-511x257-bottom | 131327 | 575070.78 | 1532946.78 | 2.665666 | faster |
| bgra32-zero-511x257-top | 131327 | 580909.46 | 1563265.31 | 2.691065 | faster |
| bgrx32-17x9-bottom | 153 | 1399.45 | 3275.82 | 2.340791 | faster |
| bgrx32-17x9-top | 153 | 1345.83 | 3122.89 | 2.320419 | faster |
| bgrx32-1920x1080-bottom | 2073600 | 20610391.67 | 44152423.00 | 2.142241 | faster |
| bgrx32-1920x1080-top | 2073600 | 21556510.00 | 38763370.50 | 1.798221 | faster |
| bgrx32-1x1-bottom | 1 | 551.78 | 823.00 | 1.491536 | faster |
| bgrx32-1x1-top | 1 | 554.08 | 809.79 | 1.461504 | faster |
| bgrx32-511x257-bottom | 131327 | 749942.34 | 1975527.54 | 2.634239 | faster |
| bgrx32-511x257-top | 131327 | 743117.23 | 2001538.25 | 2.693435 | faster |
| core24-17x9-bottom | 153 | 1325.48 | 2685.31 | 2.025915 | faster |
| core24-1920x1080-bottom | 2073600 | 22920630.00 | 34269658.50 | 1.495145 | faster |
| core24-1x1-bottom | 1 | 480.26 | 725.94 | 1.511556 | faster |
| core24-511x257-bottom | 131327 | 810746.77 | 1350808.48 | 1.666129 | faster |
| core8-17x9-bottom | 153 | 2095.71 | 3748.28 | 1.788549 | faster |
| core8-1920x1080-bottom | 2073600 | 10416328.60 | 26874263.00 | 2.580013 | faster |
| core8-1x1-bottom | 1 | 1537.69 | 2370.91 | 1.541865 | faster |
| core8-511x257-bottom | 131327 | 263938.29 | 957577.92 | 3.628037 | faster |
| indexed1-17x9-bottom | 153 | 1415.48 | 3634.84 | 2.56792 | faster |
| indexed1-17x9-top | 153 | 1403.45 | 3553.82 | 2.532203 | faster |
| indexed1-1920x1080-bottom | 2073600 | 12515122.00 | 23465317.00 | 1.874957 | faster |
| indexed1-1920x1080-top | 2073600 | 12518304.50 | 18368857.67 | 1.46736 | faster |
| indexed1-1x1-bottom | 1 | 650.95 | 2394.84 | 3.678992 | faster |
| indexed1-1x1-top | 1 | 668.96 | 2370.81 | 3.544024 | faster |
| indexed1-511x257-bottom | 131327 | 644729.43 | 857892.21 | 1.330624 | faster |
| indexed1-511x257-top | 131327 | 528605.88 | 850593.88 | 1.609127 | faster |
| indexed4-17x9-bottom | 153 | 1326.34 | 3783.51 | 2.852594 | faster |
| indexed4-17x9-top | 153 | 1310.66 | 3750.24 | 2.861337 | faster |
| indexed4-1920x1080-bottom | 2073600 | 10464091.60 | 28010724.00 | 2.676842 | faster |
| indexed4-1920x1080-top | 2073600 | 10672221.80 | 22954761.50 | 2.150889 | faster |
| indexed4-1x1-bottom | 1 | 695.57 | 2393.68 | 3.441322 | faster |
| indexed4-1x1-top | 1 | 721.31 | 2358.81 | 3.270175 | faster |
| indexed4-511x257-bottom | 131327 | 266391.98 | 961826.12 | 3.610567 | faster |
| indexed4-511x257-top | 131327 | 264794.09 | 952817.54 | 3.598334 | faster |
| indexed8-17x9-bottom | 153 | 2170.77 | 3941.64 | 1.81578 | faster |
| indexed8-17x9-top | 153 | 2076.37 | 3779.27 | 1.820133 | faster |
| indexed8-1920x1080-bottom | 2073600 | 10370190.00 | 27487766.50 | 2.650652 | faster |
| indexed8-1920x1080-top | 2073600 | 10663310.20 | 23566618.50 | 2.210066 | faster |
| indexed8-1x1-bottom | 1 | 1519.54 | 2346.18 | 1.544007 | faster |
| indexed8-1x1-top | 1 | 1499.60 | 2323.30 | 1.54928 | faster |
| indexed8-511x257-bottom | 131327 | 268951.52 | 983483.46 | 3.656731 | faster |
| indexed8-511x257-top | 131327 | 273903.07 | 953151.35 | 3.479886 | faster |
| indexed8-short-17x9-bottom | 153 | 1525.07 | 3932.46 | 2.578544 | faster |
| indexed8-short-17x9-top | 153 | 1466.12 | 3845.07 | 2.622616 | faster |
| indexed8-short-1920x1080-bottom | 2073600 | 10832807.80 | 28603265.00 | 2.640429 | faster |
| indexed8-short-1920x1080-top | 2073600 | 10848392.60 | 22785636.00 | 2.10037 | faster |
| indexed8-short-1x1-bottom | 1 | 712.64 | 2438.29 | 3.421489 | faster |
| indexed8-short-1x1-top | 1 | 706.14 | 2390.83 | 3.385773 | faster |
| indexed8-short-511x257-bottom | 131327 | 289416.51 | 989199.58 | 3.41791 | faster |
| indexed8-short-511x257-top | 131327 | 290440.38 | 971058.83 | 3.343402 | faster |
| rgb24-17x9-bottom | 153 | 1326.96 | 2596.46 | 1.956698 | faster |
| rgb24-17x9-top | 153 | 1329.96 | 2630.92 | 1.978195 | faster |
| rgb24-1920x1080-bottom | 2073600 | 20666290.33 | 33461897.50 | 1.619154 | faster |
| rgb24-1920x1080-top | 2073600 | 21524974.67 | 29917630.50 | 1.389903 | faster |
| rgb24-1x1-bottom | 1 | 464.54 | 739.85 | 1.592651 | faster |
| rgb24-1x1-top | 1 | 476.67 | 715.10 | 1.500199 | faster |
| rgb24-511x257-bottom | 131327 | 794872.20 | 1353190.12 | 1.7024 | faster |
| rgb24-511x257-top | 131327 | 777626.78 | 1341051.68 | 1.724544 | faster |
| rgb333-17x9-bottom | 153 | 1880.11 | 4045.30 | 2.151629 | faster |
| rgb333-17x9-top | 153 | 1873.74 | 3833.58 | 2.045951 | faster |
| rgb333-1920x1080-bottom | 2073600 | 26043235.50 | 49693043.00 | 1.908098 | faster |
| rgb333-1920x1080-top | 2073600 | 26675834.50 | 45616932.00 | 1.710047 | faster |
| rgb333-1x1-bottom | 1 | 564.78 | 827.40 | 1.464995 | faster |
| rgb333-1x1-top | 1 | 587.50 | 842.62 | 1.434247 | faster |
| rgb333-511x257-bottom | 131327 | 1114697.88 | 2403104.38 | 2.155835 | faster |
| rgb333-511x257-top | 131327 | 1109298.15 | 2425017.71 | 2.186083 | faster |
| rgb565-1x1-bottom | 1 | 552.39 | 828.65 | 1.500118 | faster |
| rgb565-1x1-top | 1 | 555.47 | 817.35 | 1.471457 | faster |
| rgba32-17x9-bottom | 153 | 864.91 | 2436.55 | 2.817114 | faster |
| rgba32-17x9-top | 153 | 849.20 | 2273.91 | 2.677708 | faster |
| rgba32-1920x1080-bottom | 2073600 | 14473391.25 | 30753487.00 | 2.124829 | faster |
| rgba32-1920x1080-top | 2073600 | 14636406.33 | 23425479.00 | 1.600494 | faster |
| rgba32-1x1-bottom | 1 | 574.30 | 626.36 | 1.090649 | faster |
| rgba32-1x1-top | 1 | 581.39 | 626.52 | 1.077624 | faster |
| rgba32-511x257-bottom | 131327 | 1201515.60 | 1231680.38 | 1.025106 | faster |
| rgba32-511x257-top | 131327 | 279013.32 | 1229540.18 | 4.406744 | faster |
| rgba32-swapped-17x9-bottom | 153 | 820.05 | 3523.44 | 4.296616 | faster |
| rgba32-swapped-17x9-top | 153 | 815.65 | 3505.59 | 4.29791 | faster |
| rgba32-swapped-1920x1080-bottom | 2073600 | 14164993.75 | 49057594.00 | 3.463298 | faster |
| rgba32-swapped-1920x1080-top | 2073600 | 14135250.50 | 42174071.00 | 2.98361 | faster |
| rgba32-swapped-1x1-bottom | 1 | 581.71 | 624.90 | 1.074247 | faster |
| rgba32-swapped-1x1-top | 1 | 589.22 | 614.98 | 1.043719 | faster |
| rgba32-swapped-511x257-bottom | 131327 | 253336.66 | 2424478.08 | 9.570183 | faster |
| rgba32-swapped-511x257-top | 131327 | 260797.36 | 2427432.12 | 9.307733 | faster |
| rgba4444-17x9-bottom | 153 | 1492.50 | 4004.11 | 2.682821 | faster |
| rgba4444-17x9-top | 153 | 1453.00 | 3802.96 | 2.617316 | faster |
| rgba4444-1920x1080-bottom | 2073600 | 22471312.50 | 51576575.00 | 2.295219 | faster |
| rgba4444-1920x1080-top | 2073600 | 22496006.50 | 44827605.00 | 1.992692 | faster |
| rgba4444-1x1-bottom | 1 | 595.00 | 634.81 | 1.066908 | faster |
| rgba4444-1x1-top | 1 | 593.22 | 634.95 | 1.070345 | faster |
| rgba4444-511x257-bottom | 131327 | 832273.46 | 2616494.83 | 3.143792 | faster |
| rgba4444-511x257-top | 131327 | 839822.71 | 2617318.04 | 3.116513 | faster |

Allocation subset: 662 matching keys, each present in both timing datasets.

## Allocation comparison: alloc_calls

480 paired measurements; geometric mean ratio **1x**; minimum **1x**; maximum **1x**. Worst: `bgra32-17x9-bottom` / `native-convert`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| bgra32 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| core24 | 1x1 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 1x1 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 1x1 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 1x1 | native-rgba | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-rgba | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-rgba | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-rgba | 1 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |

### Observed regressions (0)

Every increase is listed, including changes that may be measurement noise.

None.

## Allocation comparison: requested_bytes

480 paired measurements; geometric mean ratio **1x**; minimum **1x**; maximum **1x**. Worst: `bgra32-17x9-bottom` / `native-convert`.

Ratios greater than 1 mean less work after the change. Orientations are grouped; sizes and modes remain separate. Groups are equally weighted per measurement, not weighted by pixel count.

| Layout | Size | Mode | Cases | Geomean ratio | Min ratio | Max ratio |
| --- | --- | --- | --- | --- | --- | --- |
| bgra32 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| bgra32 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| bgra32-zero | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| bgrx32 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| core24 | 1x1 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 1x1 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 1x1 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 1x1 | native-rgba | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 17x9 | native-rgba | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 511x257 | native-rgba | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-convert | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-decode | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-encode | 1 | 1 | 1 | 1 |
| core24 | 1920x1080 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 1x1 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 17x9 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 511x257 | native-rgba | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-convert | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-decode | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-encode | 1 | 1 | 1 | 1 |
| core8 | 1920x1080 | native-rgba | 1 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed1 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed4 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed8 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| indexed8-short | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb24 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb333 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb555 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgb565 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgba32 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgba32-swapped | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 1x1 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 17x9 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 511x257 | native-rgba | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-convert | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-decode | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-encode | 2 | 1 | 1 | 1 |
| rgba4444 | 1920x1080 | native-rgba | 2 | 1 | 1 | 1 |

### Observed regressions (0)

Every increase is listed, including changes that may be measurement noise.

None.
