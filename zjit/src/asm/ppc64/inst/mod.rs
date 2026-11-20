// This module contains various PPC64 instructions and the logic necessary to
// encode them into u32s.

mod d_form;
mod ds_form;

pub use d_form::DForm;
pub use ds_form::DSForm;
