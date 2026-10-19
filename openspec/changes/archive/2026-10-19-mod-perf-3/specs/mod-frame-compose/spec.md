## ADDED Requirements

### Requirement: Java's encoders finish off its thread
In mod mode, a command encoder Java queues for a later submit SHALL be finished on a worker thread, not on Java's render thread. The render thread SHALL wait for the finished buffer only when it drains that submission. When the worker is unavailable or its queue is full, the encoder SHALL be finished on the calling thread. A buffer whose recording failed SHALL be dropped with a warning and SHALL NOT reach the queue.

#### Scenario: Queued work
- **WHEN** Java queues a frame's encoders while mcr composes the previous frame
- **THEN** Java's frame does not include the encoders' finish, and mcr's batch submits the finished buffers in slot order

#### Scenario: Invalid encoder
- **WHEN** a queued encoder recorded a validation error
- **THEN** its buffer is dropped with a warning and the rest of the batch is submitted

### Requirement: Slow frame profile
While `MCR_MOD_PROFILE` is set and after the first 100 Java frames, mcr SHALL collect the Java frames that took more than twice the running mean, and the bench summary SHALL print their count and their phase means. The `--bench-timeline` summary SHALL print the mean, p50, p99, p99.9 and maximum frame time over its last 31 seconds.

#### Scenario: Spike phases
- **WHEN** a bench runs with `MCR_MOD_PROFILE=frame` and some Java frames take over twice the mean
- **THEN** the summary prints how many there were and which Java phases they spent their time in
