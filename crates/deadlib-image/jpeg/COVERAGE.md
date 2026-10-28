# Standards coverage and outstanding work

This crate is under development. No complete-conformance or competitive-speed
claim is made. The complete user scope is retained in PLAN.md.

| Requirement | Current evidence / remaining work |
| --- | --- |
| Independent package, workspace registration | Cargo manifest; no application dependency |
| Native bounded marker parsing | Zero-copy segments/scans; stuffing/restart/fill boundaries; every truncated prefix tested |
| Sequential Huffman SOF0/SOF1, 8/12-bit | Pending |
| Progressive Huffman SOF2, 8/12-bit | Pending |
| Arithmetic SOF9/SOF10 | Pending: Annex D coder and contexts |
| Predictive lossless SOF3/SOF11, 2–16-bit | Pending: predictors, point transform, entropy |
| Hierarchical SOF5/6/7/13/14/15, DHP/EXP | Pending: differential frames and expansion |
| DNL, abbreviated tables/image streams | Pending |
| Gray, YCbCr, RGB, CMYK, YCCK | Pending |
| All legal sampling factors and scan layouts | Pending |
| Restart intervals and sequence validation | Pending |
| Original coefficients, tables, metadata | Pending |
| JFIF/JFXX, Exif, Adobe, ICC assembly | Pending |
| IDCT, chroma upsampling, color numerics | Pending reference error characterization |
| Native configurable encoder | Pending |
| Optimized Huffman, progressive encoding | Pending |
| Streaming and allocation limits | Pending |
| Independent corpus and three C oracles | Pending |
| Fuzzing and malformed corpus | Pending |
| Throughput/cycles/allocations/memory | Pending |
| Independent out-of-workspace build | Pending |
