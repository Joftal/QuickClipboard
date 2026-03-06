pub mod paste_handler;
pub mod text;
mod file;
pub mod keyboard;
pub mod clipboard_content;

pub use text::PasteFormat;
pub use clipboard_content::{
    FilesData,
    set_clipboard_files,
    set_clipboard_from_item,
    set_clipboard_text,
};








