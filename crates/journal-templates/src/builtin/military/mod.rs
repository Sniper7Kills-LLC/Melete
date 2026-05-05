//! Military page templates.
//!
//! One file per form. Shared widget-construction helpers and paper-size
//! constants live in `crate::builtin`  -  each form module pulls them in
//! via `use crate::builtin::{...}`.

pub mod gotwa;
pub mod medevac;
pub mod opord;
pub mod pace;
pub mod pcc_pci;
pub mod range_card;
pub mod salute;
pub mod uxo;

pub use gotwa::{builtin_military_gotwa, BUILTIN_MILITARY_GOTWA_ID};
pub use medevac::{builtin_military_medevac, BUILTIN_MILITARY_MEDEVAC_ID};
pub use opord::{builtin_military_opord, BUILTIN_MILITARY_OPORD_ID};
pub use pace::{builtin_military_pace, BUILTIN_MILITARY_PACE_ID};
pub use pcc_pci::{builtin_military_pcc_pci, BUILTIN_MILITARY_PCC_PCI_ID};
pub use range_card::{builtin_military_range_card, BUILTIN_MILITARY_RANGE_CARD_ID};
pub use salute::{builtin_military_salute, BUILTIN_MILITARY_SALUTE_ID};
pub use uxo::{builtin_military_uxo, BUILTIN_MILITARY_UXO_ID};
