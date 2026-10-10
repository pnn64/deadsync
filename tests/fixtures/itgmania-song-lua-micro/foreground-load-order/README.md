This control checks foreground file/Init ordering using shared globals and
native RNG calls. The second script requires the first root and its child
to be initialized. Root Init observes its screen parent before attachment.

The expected ordering comes from local ITGmania Foreground.cpp:36-62,
ActorUtil.cpp:177-245, and ActorFrame.cpp:89-94, whose checked-out and compiled
vendor bytes are identical. Harness 0.1.54 fails the unchanged control;
0.1.55 captures all 241 frames without runtime errors or dropped events.

The semantic loader is under test. This is not an independently executed
C++ actor-loading trace; the source audit and failing old binary establish
the loader contract. See provenance.json for source and binary hashes.
