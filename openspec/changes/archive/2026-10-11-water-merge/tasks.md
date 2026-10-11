## 1. Mesher

- [x] 1.1 Buffer flat still fluid tops and merge them greedily (max 3x3) after the tint table is built
- [x] 1.2 Repeat the still sprite per block in the merged quad's UVs

## 2. Tests

- [x] 2.1 Ocean section collapses to 3x3 tiles with unchanged area; UVs repeat per block
- [x] 2.2 Differing light, tint colour, height or sprite prevents merging; flowing water unchanged
