//! Runtime streaming systems that keep heavy I/O off the deterministic hot tick.

mod asset_stream_error;
mod assets;
mod assets_product;
mod texture;
mod texture_product;
mod world;
mod world_store;

pub use assets::{
    ASSET_LOD_STREAM_SCHEMA, AssetLodDistanceBand, AssetLodLoadJob, AssetLodLoadReadback,
    AssetLodLoadReceipt, AssetLodSelection, AssetLodStreamConfig, AssetLodStreamLoadJob,
    AssetLodStreamLoadRequest, AssetLodStreamSourceReadback, RuntimeAssetLod,
};
pub use texture::{
    RuntimeTextureMipStream, TEXTURE_MIP_PRODUCT_SCHEMA, TEXTURE_MIP_STREAM_SCHEMA,
    TextureMipDistanceBand, TextureMipLevelReadback, TextureMipLevelRequest,
    TextureMipResidencyJob, TextureMipResidencyPlan, TextureMipResidencyReadback,
    TextureMipResidencyReceipt, TextureMipResidentLevel, TextureMipStreamConfig,
    TextureMipStreamLoadJob, TextureMipStreamLoadRequest, TextureMipStreamReadback,
    TextureMipStreamSourceReadback, TextureMipStreamUpdate,
};
pub use world::{
    WORLD_ASTER_CHUNK_SCHEMA, WORLD_STREAM_SCHEMA, WorldChunkCoord, WorldChunkStreamConfig,
    WorldChunkStreamLoadJob, WorldChunkStreamLoadRequest, WorldChunkStreamReadback,
    WorldChunkStreamUpdate, WorldStreamAnchor, WorldStreamSourceReadback, WorldStreamer,
};
