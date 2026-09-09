# FaceTime camera compatibility relay

T1Bridge exposes the physical camera as H.264-only V4L2 capture. Many desktop
and browser applications expect a raw format, so Leapfrog decodes the 720p30
stream and publishes YUY2 through a v4l2loopback device named `FaceTime Camera`.

The stock `v4l2-relayd` forwards the source timestamps, which made the virtual
camera stall or run very slowly after consumers opened it. The Leapfrog patch:

- copies each buffer and gives it a timestamp from the output pipeline clock;
- clears its decode timestamp; and
- disables appsink clock synchronization.

`build-relay` applies that patch to upstream `v4l2-relayd` revision
`d6ec36aae87e765eddef8308f0f58c7b5be95ad7` (release 0.2.0) and builds
`leapfrog-camera-relayd`. The reconstructed source produces the same relevant
machine code as the installed working relay.
