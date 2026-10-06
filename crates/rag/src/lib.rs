pub mod chunk;
pub mod embed;
pub mod eval;
pub mod index;
pub mod retrieve;
pub mod store;

pub use chunk::{chunk_text, Chunk};
pub use embed::{cosine, embed_text, DIM};
pub use index::{build_index, index_status_report, IndexReport};
pub use retrieve::{retrieve, Retrieved};
pub use store::{config_hash, index_dir_for, INDEX_VERSION};
