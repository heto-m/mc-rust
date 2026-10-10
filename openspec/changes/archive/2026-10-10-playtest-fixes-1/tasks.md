## 1. Fixes

- [x] 1.1 View matrix at pitch limits (camera unit test; live shot looking straight down)
- [x] 1.2 Client-side light engine and neighbour remesh (mcr-world tests incl. randomized full-relight equivalence; live hole shots); entity light probed at the eye position
- [x] 1.3 Local destroy particles for own breaks (live shot of a broken block)
- [x] 1.4 Vanilla entity picking (unit tests in `target.rs`)
- [x] 1.5 Chicken head parts turn together (model test); airborne flag feeds wing flapping
- [x] 1.6 Block placement: quick clicks are consumed, placement is predicted (live: cobblestone placed by a tap)
- [x] 1.7 Idle stability on leaves and in water (physics tests; live 30 s checks showed no drift or corrections)
- [x] 1.8 Render distance default 32 (settings tests)
