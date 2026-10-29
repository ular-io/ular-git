//! Feature-agnostic UI primitives shared by every pane and dialog: text input,
//! list movement, modal framing, scrollbar, and width-aware text helpers.
//! Nothing here may know about repositories or git.

pub(crate) mod input;
pub(crate) mod list_nav;
pub(crate) mod modal;
pub(crate) mod scrollbar;
pub(crate) mod text;
