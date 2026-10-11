## ADDED Requirements

### Requirement: Cutout mip coverage
Mip levels of textures with fully transparent pixels (or whose `.mcmeta` `texture.mipmap_strategy` is `cutout`, `strict_cutout` or `dark_cutout`) SHALL keep the base level's alpha-test coverage like vanilla's `MipmapGenerator.scaleAlphaToCoverage`: the alpha scale is searched over five bisection steps against a 4x4 bilinear coverage estimate at cutoff 0.5 (0.3 for strict), then `alpha_cutoff_bias + 0.025` is added. `mean` textures keep a plain alpha average.

#### Scenario: Far kelp
- **WHEN** kelp is drawn at the third mip level
- **THEN** it keeps most of its base coverage instead of vanishing below the cutout
