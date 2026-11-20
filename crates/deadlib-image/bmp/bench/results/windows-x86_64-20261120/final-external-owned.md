## Equivalent RGBA comparison: native-owned-rgba, thread_cycles

12 exact-output cases; 0 non-equivalent native cases excluded. Image/native geometric mean **2.583789x**; worst **1.664257x**; best **3.147894x**; **0 native regressions versus image**. A ratio below 1 means native is slower or uses more allocations/bytes.

| Case | Pixels | Native | Image | Image/native | Native result |
| --- | --- | --- | --- | --- | --- |
| tests/fixtures/itgmania-primary-bitmaps\os2-1-padded.bmp | 64 | 1100.53 | 3112.52 | 2.828201 | faster |
| tests/fixtures/itgmania-primary-bitmaps\win8-full-table.bmp | 64 | 1961.65 | 3349.06 | 1.707267 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/misnamed.png | 64 | 1247.36 | 3314.95 | 2.657573 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\os2-1.bmp | 64 | 1088.87 | 3082.94 | 2.831321 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\os2-4.bmp | 64 | 1148.05 | 3312.45 | 2.885284 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\os2-8.bmp | 64 | 1969.57 | 3277.87 | 1.664257 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win1.bmp | 64 | 1094.18 | 3144.49 | 2.873832 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win4-odd.bmp | 15 | 863.05 | 2716.79 | 3.147894 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win4.bmp | 64 | 1232.17 | 3346.67 | 2.716078 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win8-both-pinks.bmp | 64 | 1259.15 | 3345.70 | 2.65711 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win8-odd.bmp | 45 | 1124.19 | 3256.60 | 2.896841 | faster |
| tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files\win8.bmp | 64 | 1257.13 | 3372.96 | 2.683064 | faster |
