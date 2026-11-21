// This module contains various PPC64 instructions and the logic necessary to
// encode them into u32s.

mod d_form;
mod ds_form;
mod i_form;
mod md_form;
mod x_form;
mod xo_form;
mod xs_form;

pub use d_form::DForm;
pub use ds_form::DSForm;
pub use i_form::IForm;
pub use md_form::MDForm;
pub use x_form::XForm;
pub use xo_form::XOForm;
pub use xs_form::XSForm;
