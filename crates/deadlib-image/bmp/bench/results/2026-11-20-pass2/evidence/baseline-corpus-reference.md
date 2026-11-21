## Equivalent RGBA comparison: native-owned-rgba, thread_cycles

12 exact-output cases; 0 non-equivalent native cases excluded. Image/native geometric mean **2.634531x**; worst **1.69417x**; best **3.156946x**; **0 native regressions versus image**. A ratio below 1 means native is slower or uses more allocations/bytes.

| Case | Pixels | Native | Image | Image/native | Native result |
| --- | --- | --- | --- | --- | --- |
| tests/fixtures/itgmania-primary-bitmaps\os2-1-padded.bmp | 64 | 994.85 | 2890.02 | 2.904981 | faster |
| tests/fixtures/itgmania-primary-bitmaps\win8-full-table.bmp | 64 | 1809.47 | 3065.55 | 1.69417 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/misnamed.png | 64 | 1085.27 | 2980.70 | 2.746505 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\os2-1.bmp | 64 | 1008.30 | 2868.60 | 2.844987 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\os2-4.bmp | 64 | 1027.64 | 3102.13 | 3.018693 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\os2-8.bmp | 64 | 1749.89 | 3018.14 | 1.72476 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win1.bmp | 64 | 973.82 | 2843.41 | 2.919852 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win4-odd.bmp | 15 | 803.78 | 2537.49 | 3.156946 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win4.bmp | 64 | 1142.28 | 3181.32 | 2.785061 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win8-both-pinks.bmp | 64 | 1163.81 | 3167.13 | 2.721346 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win8-odd.bmp | 45 | 1041.68 | 2949.30 | 2.831292 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win8.bmp | 64 | 1113.93 | 3144.54 | 2.822924 | faster |

