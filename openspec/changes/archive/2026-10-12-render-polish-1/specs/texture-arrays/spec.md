## ADDED Requirements

### Requirement: Dark cutout mips
Textures whose `.mcmeta` `texture.mipmap_strategy` is `dark_cutout` SHALL fill fully transparent texels with 3/4 of their darkest opaque colour and build each mip as `MipmapGenerator.darkenedAlphaBlend`: the linear-light channels of the non-empty texels, alpha included, summed and divided by four, followed by the cutout coverage rescale.

#### Scenario: Distant leaves
- **WHEN** leaves are drawn at a high mip level
- **THEN** their colour is darker than the plain average where the texture has holes
