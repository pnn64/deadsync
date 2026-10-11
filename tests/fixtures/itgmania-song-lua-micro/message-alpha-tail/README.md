# Native message alpha tail

This control reproduces Delightful Day's LightsOff/LightsOn overlap.
An unchanged alpha destination still queues a new 0.8 second tween behind
the unfinished 0.75 second fade. Reveal then changes that queued tail.

The song host trace and independent native Actor/ActorFrame control agree
exactly on all 648 frame alpha values. The native dispatcher runs after
the fade actor, matching ActorFrame::UpdateInternal's callback order.
The regression checks all frames and rejects missing queue delay metadata.
Actor.h DestTweenState and Actor.cpp UpdateTweening establish the behavior.

Native JSON bytes are retained; the independent result is compressed
losslessly. proof.json records both hashes and the pinned ITGmania source.
This proves queue semantics and alpha values, not GPU framebuffer pixels.
