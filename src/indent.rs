use std::sync::atomic::{AtomicI32, Ordering};

pub(crate) static INDENT_LEVEL: AtomicI32 =  AtomicI32::new(0);

pub struct Indent;

impl Indent {
    pub fn new() -> Self {
        INDENT_LEVEL.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for Indent {
    fn drop(&mut self) {
        INDENT_LEVEL.fetch_sub(1, Ordering::Relaxed);
    }
}

#[macro_export]
macro_rules! in_n_println {
    ($indent:expr, $($args:tt)*) => {{
        let indent_str = " ".repeat($indent * 4); // 4 spaces per indent level
        let output = format!($($args)*); // Format the arguments just like println!
        for line in output.lines() {
            println!("{}{}", indent_str, line); // Print each line with indentation
        }
    }};
}

#[macro_export]
macro_rules! in_println {
    ($($args:tt)*) => {{
        let indent = $crate::indent::INDENT_LEVEL.load(std::sync::atomic::Ordering::Relaxed);
        let indent_str = " ".repeat(indent as usize * 4);
        let output = format!($($args)*);

        for line in output.lines() {
            println!("{}{}", indent_str, line);
        }
    }};
}

/* 
#[macro_export]
macro_rules! indent_push {
    () => {{
        INDENT_LEVEL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }};
}

#[macro_export]
macro_rules! indent_pop {
    () => {{
            INDENT_LEVEL.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    };
}
    */