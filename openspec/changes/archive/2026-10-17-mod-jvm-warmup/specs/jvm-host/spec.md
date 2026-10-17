## MODIFIED Requirements

### Requirement: Orderly shutdown
`System.exit` and the VM abort path SHALL be intercepted through JNI hooks and turned into an mcr shutdown request. After the boot call returns, the host SHALL call `DestroyJavaVM` and SHALL not block process exit for more than 10 seconds, or 120 seconds while `DestroyJavaVM` writes an AOT recording.

#### Scenario: Java calls System.exit
- **WHEN** Java code calls `System.exit(3)`
- **THEN** the host records exit code 3 and signals shutdown to the Rust side

#### Scenario: Recording session quits
- **WHEN** a session that records an AOT cache quits and `DestroyJavaVM` takes 5 seconds to write it
- **THEN** the shutdown is reported clean and the recording is kept
