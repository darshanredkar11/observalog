pub mod classify;
pub mod rules;
pub mod walker;

pub use classify::{classify_function, FunctionClass};
pub use walker::{parse_go_file, GoFile, GoFunction};
