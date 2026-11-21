// This module contains various PPC64 instructions and the logic necessary to
// encode them into u32s.

mod a_form;
mod b_form;
mod d_form;
mod ds_form;
mod i_form;
mod md_form;
mod x_form;
mod xfx_form;
mod xl_form;
mod xo_form;
mod xs_form;

pub use a_form::AForm;
pub use b_form::BForm;
pub use d_form::DForm;
pub use ds_form::DSForm;
pub use i_form::IForm;
pub use md_form::MDForm;
pub use x_form::XForm;
pub use xfx_form::XFXForm;
pub use xl_form::XLForm;
pub use xo_form::XOForm;
pub use xs_form::XSForm;
