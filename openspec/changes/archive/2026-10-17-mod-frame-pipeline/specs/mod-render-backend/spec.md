## MODIFIED Requirements

### Requirement: Readback and fences
`copyTextureToBuffer` callbacks SHALL run on the Java render thread after the GPU copy completes. A fence wait SHALL block only while a readback that was recorded at or before the fence's submission is still incomplete. Otherwise a fence wait SHALL return at once, since host writes reach the GPU as copies in queue order. A zero-timeout wait on the open recording's fence SHALL report not done without submitting it. Polling for readbacks SHALL NOT poll the device while no readback is pending.

#### Scenario: Screenshot
- **WHEN** Java takes a screenshot through vanilla's screenshot path
- **THEN** the callback runs within two frames and the saved image matches the offscreen image

#### Scenario: Ring buffer reuse
- **WHEN** a mapped ring buffer waits on the fence of the slot it reuses, and no readback is pending
- **THEN** the wait returns without polling the device, and the earlier draws still see the earlier data
