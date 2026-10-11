## 1. Culling

- [x] 1.1 Pure `in_view` entity test and pickup culling
- [x] 1.2 `mcr-block-entity` cull module with render boxes and view distances, wired into the scene
- [x] 1.3 `beyond_render_fog` on CPU and in the section cull shader and translucent sort

## 2. Tests

- [x] 2.1 Unit tests: frustum vs AABB, distance rules, infinite-height boxes, fog range, hostile input
- [x] 2.2 Existing GPU occlusion and culling tests still pass
