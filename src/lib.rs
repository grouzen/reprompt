pub mod app;
pub mod ollama;
pub mod prompt;

#[macro_export]
macro_rules! assign_if_some {
    ($target:ident, $expr:expr) => {
        if let val @ Some(_) = $expr {
            $target = val;
        }
    };
}
