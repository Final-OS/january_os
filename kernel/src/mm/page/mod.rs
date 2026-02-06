//! 物理页管理
//!
//! 包含页帧描述符、伙伴系统、Memblock、Zone 管理等。

pub mod page;
pub use page::*;

pub mod buddy;
pub use buddy::*;

pub mod memblock;
pub use memblock::*;

pub mod zone;
pub use zone::*;

pub mod numa;
pub use numa::*;

pub mod pcp;
pub use pcp::*;

pub mod physical;
pub use physical::*;
