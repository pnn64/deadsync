# Queued movement and Model fade

This complete native song control exercises a self-queued Move command,
an ordinary Quad fade, an actual compiled Model fade, and a timing warp.
All 241 native update frames and original source files are retained.
The native_alpha_loop test runs every semantic comparator against them.

Before repair, movement and visibility had twelve failures and the Model
had one pass-count failure at its fade endpoint. Preserving startup
self-queues closes the movement failures. Matching Model's joint 0.001
diffuse/glow draw cutoff closes the last failure. The unchanged native
capture now passes all 26,894 comparisons.
