## ADDED Requirements

### Requirement: Dynamic sprite reuse
Sprites added at runtime SHALL be freed when dropped, and a later sprite of the same size SHALL reuse the freed atlas slot, so atlas pages do not grow without bound. Runtime icon, favicon and head caches SHALL be bounded.

#### Scenario: Icon cache churn
- **WHEN** thousands of distinct dyed item icons are drawn over a session
- **THEN** the number of atlas pages stays bounded
