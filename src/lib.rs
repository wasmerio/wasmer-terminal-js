use console_error_panic_hook::set_once as set_panic_hook;
use wasm_bindgen::prelude::*;
use xterm_js_sys::xterm::{LogLevel, Terminal, TerminalOptions, Theme};
use wasmer::{Instance, Module, Store};
use wasmer_wasi::{Stdin, Stdout, WasiState};

#[path = "../../common.rs"]
mod common;
use common::log;

#[macro_export]
#[doc(hidden)]
macro_rules! csi {
    ($( $l:expr ),*) => { concat!("\x1B[", $( $l ),*) };
}

pub(crate) const ENABLE_MOUSE_MODE_CSI_SEQUENCE: &str = concat!(
    csi!("?1000h"),
    csi!("?1002h"),
    csi!("?1015h"),
    csi!("?1006h")
);

#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    set_panic_hook();

    // Use `web_sys`'s global `window` function to get a handle on the global
    // window object.
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let terminal_div = document
        .get_element_by_id("terminal")
        .expect("should have a terminal div");

    let term_orig = Terminal::new(Some(
        TerminalOptions::default()
            .with_log_level(LogLevel::Debug)
            .with_theme(Theme::nord())
            .with_font_family("'Fira Mono', monospace")
            .with_font_size(11.0),
    ));

    term_orig.open(terminal_div);
    term_orig.write(ENABLE_MOUSE_MODE_CSI_SEQUENCE.to_string());

    let term = term_orig.clone();
    let l = term_orig.attach_key_event_listener(move |e| {
        // A port of the xterm.js echo demo:
        let key = e.key();
        let ev = e.dom_event();

        let printable = matches!(
            (ev.alt_key(), ev.ctrl_key(), ev.meta_key()),
            (false, false, false)
        );

        const ENTER_ASCII_KEY_CODE: u32 = 13;
        const BACKSPACE_ASCII_KEY_CODE: u32 = 8;

        match ev.key_code() {
            ENTER_ASCII_KEY_CODE => {
                let store = Store::default();
                let module = Module::new(&store, br#"
                (module
                    ;; Import the required fd_write WASI function which will write the given io vectors to stdout
                    ;; The function signature for fd_write is:
                    ;; (File Descriptor, *iovs, iovs_len, nwritten) -> Returns number of bytes written
                    (import "wasi_unstable" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
            
                    (memory 1)
                    (export "memory" (memory 0))
            
                    ;; Write 'hello world\n' to memory at an offset of 8 bytes
                    ;; Note the trailing newline which is required for the text to appear
                    (data (i32.const 8) "hello world\n")
            
                    (func $main (export "_start")
                        ;; Creating a new io vector within linear memory
                        (i32.store (i32.const 0) (i32.const 8))  ;; iov.iov_base - This is a pointer to the start of the 'hello world\n' string
                        (i32.store (i32.const 4) (i32.const 12))  ;; iov.iov_len - The length of the 'hello world\n' string
            
                        (call $fd_write
                            (i32.const 1) ;; file_descriptor - 1 for stdout
                            (i32.const 0) ;; *iovs - The pointer to the iov array, which is stored at memory location 0
                            (i32.const 1) ;; iovs_len - We're printing 1 string stored in an iov - so one.
                            (i32.const 20) ;; nwritten - A place in memory to store the number of bytes written
                        )
                        drop ;; Discard the number of bytes written from the top of the stack
                    )
                )
                "#).unwrap();
            
                // Create the `WasiEnv`.
                // let stdout = Stdout::default();
                let mut wasi_env = WasiState::new("command-name")
                    .args(&["Gordon"])
                    // .stdout(Box::new(stdout))
                    .finalize()
                    .unwrap();
            
                // Generate an `ImportObject`.
                let import_object = wasi_env.import_object(&module).unwrap();
            
                // Let's instantiate the module with the imports.
                let instance = Instance::new(&module, &import_object).unwrap();
            
                // Let's call the `_start` function, which is our `main` function in Rust.
                let start = instance.exports.get_function("_start").unwrap();
                start.call(&[]).unwrap();
            
                let state = wasi_env.state();
                let stdout = state.fs.stdout().unwrap().as_ref().unwrap();
                let stdout = stdout.downcast_ref::<Stdout>().unwrap();
                let stdout_as_str = std::str::from_utf8(&stdout.buf).unwrap();
                term.write("\n\r".to_string());
                term.write(stdout_as_str.to_owned());
                if !stdout_as_str.ends_with("\n") {
                    term.write("%\r\n".to_string());
                }
                term.write("\r\x1B[1;3;31m$ \x1B[0m".to_string())
            }
            BACKSPACE_ASCII_KEY_CODE => {
                term.write("\u{0008} \u{0008}".to_string())
            }
            _ if printable => term.write(key),
            _ => {}
        }

        log!("[key event] got {:?}", e);
    });

    let b = term_orig.attach_binary_event_listener(move |s| {
        log!("[binary event] bin: {:?}", s);
    });

    let d = term_orig.attach_data_event_listener(move |s| {
        log!("[data event] data: {:?}", s);
    });

    let r = term_orig.attach_resize_event_listener(move |r| {
        log!("[resize event] resize: {:?}", r);
    });

    // Don't drop!
    Box::leak(Box::new(l));
    Box::leak(Box::new(b));
    Box::leak(Box::new(d));
    Box::leak(Box::new(r));

    let term = term_orig;

    term.focus();

    term.write(String::from("Wasmer-JS Terminal. Enjoy!"));
    term.write(String::from("\n\r\x1B[1;3;31m$ \x1B[0m"));

    Ok(())
}
