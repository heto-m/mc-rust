use std::collections::HashMap;
use std::sync::Arc;

use mcr_assets::{
    BakedItemModel, IDENTITY4, Identifier, ItemBaker, ItemModels, ResourceManager, SpecialKind,
};

use super::raster::LayerDraw;
use crate::error::Result;

pub type Specials = HashMap<SpecialKind, Arc<BakedItemModel>>;

/// Builds an approximation of every special model kind the definitions use.
pub fn bake_all(
    resources: &ResourceManager,
    baker: &mut ItemBaker,
    models: &ItemModels,
    baked: &HashMap<Identifier, Arc<BakedItemModel>>,
) -> Result<Specials> {
    let mut out = Specials::new();
    for def in models.ids().filter_map(|id| models.get(id)) {
        for (base, kind) in def.specials() {
            if out.contains_key(&kind) {
                continue;
            }
            let Some(base) = baked.get(&base) else {
                continue;
            };
            out.insert(kind.clone(), baker.bake_special(resources, base, &kind)?);
        }
    }
    Ok(out)
}

pub fn resolve<'a>(kind: &SpecialKind, specials: &'a Specials) -> Option<LayerDraw<'a>> {
    specials.get(kind).map(|model| LayerDraw {
        model,
        tints: &model.tints,
        local: IDENTITY4,
    })
}
