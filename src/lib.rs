//! # wapc-guest
//!
//! The `wapc-guest` library provides WebAssembly module developers with access to a
//! [waPC](https://github.com/wapc)-compliant host runtime. Each guest module registers
//! function handlers with `register_function`. Inside this call handler, the guest
//! module should check the operation of the delivered message and handle it accordingly,
//! returning any binary payload in response.
//!
//! # Example
//! ```
//! extern crate wapc_guest as guest;
//!
//! use guest::prelude::*;
//!
//! #[no_mangle]
//! pub extern "C" fn wapc_init() {
//!   register_function("sample:Guest!Hello", hello_world);
//! }
//!
//! fn hello_world(
//!    _msg: &[u8]) -> CallResult {
//!    let _res = host_call("myBinding", "sample:Host", "Call", b"hello")?;
//!     Ok(vec![])
//! }
//! ```

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::RwLock;

pub type CallResult = std::result::Result<Vec<u8>, Box<dyn std::error::Error + Sync + Send>>;
pub type HandlerResult<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;

#[link(wasm_import_module = "wapc")]
extern "C" {
    pub fn __console_log(ptr: *const u8, len: usize);
    pub fn __host_call(
        bd_ptr: *const u8,
        bd_len: usize,
        ns_ptr: *const u8,
        ns_len: usize,
        op_ptr: *const u8,
        op_len: usize,
        ptr: *const u8,
        len: usize,
    ) -> usize;
    pub fn __host_response(ptr: *mut u8);
    pub fn __host_response_len() -> usize;
    pub fn __host_error_len() -> usize;
    pub fn __host_error(ptr: *mut u8);
    pub fn __guest_response(ptr: *const u8, len: usize);
    pub fn __guest_error(ptr: *const u8, len: usize);
    pub fn __guest_request(op_ptr: *mut u8, ptr: *mut u8);
}

lazy_static! {
    static ref REGISTRY: RwLock<HashMap<String, fn(&[u8]) -> CallResult>> =
        RwLock::new(HashMap::new());
}

pub fn register_function(name: &str, f: fn(&[u8]) -> CallResult) {
    REGISTRY.write().unwrap().insert(name.to_string(), f);
}

#[no_mangle]
pub extern "C" fn __guest_call(op_len: i32, req_len: i32) -> i32 {
    let mut buf: Vec<u8> = Vec::with_capacity(req_len as _);
    let mut opbuf: Vec<u8> = Vec::with_capacity(op_len as _);

    unsafe {
        __guest_request(opbuf.as_mut_ptr(), buf.as_mut_ptr());
        // The two buffers have now been initialized
        buf.set_len(req_len as usize);
        opbuf.set_len(op_len as usize);
    };

    let opstr = ::std::str::from_utf8(&opbuf).unwrap();

    match REGISTRY.read().unwrap().get(opstr) {
        Some(handler) => match handler(&buf) {
            Ok(result) => {
                unsafe {
                    __guest_response(result.as_ptr(), result.len() as _);
                }
                1
            }
            Err(e) => {
                let errmsg = format!("Guest call failed: {}", e);
                unsafe {
                    __guest_error(errmsg.as_ptr(), errmsg.len() as _);
                }
                0
            }
        },
        None => {
            let errmsg = format!("No handler registered for function \"{}\"", opstr);
            unsafe {
                __guest_error(errmsg.as_ptr(), errmsg.len() as _);
            }
            0
        }
    }
}

/// The function through which all host calls take place.
pub fn host_call(binding: &str, ns: &str, op: &str, msg: &[u8]) -> CallResult {
    let callresult = unsafe {
        __host_call(
            binding.as_ptr() as _,
            binding.len() as _,
            ns.as_ptr() as _,
            ns.len() as _,
            op.as_ptr() as _,
            op.len() as _,
            msg.as_ptr() as _,
            msg.len() as _,
        )
    };
    if callresult != 1 {
        // call was not successful
        let errlen = unsafe { __host_error_len() };
        let mut buf = Vec::with_capacity(errlen as _);
        let retptr = buf.as_mut_ptr();
        unsafe {
            __host_error(retptr);
            buf.set_len(errlen);
        }
        Err(Box::new(errors::new(errors::ErrorKind::HostError(
            String::from_utf8(buf).unwrap(),
        ))))
    } else {
        // call succeeded
        let len = unsafe { __host_response_len() };
        let mut buf = Vec::with_capacity(len as _);
        let retptr = buf.as_mut_ptr();
        unsafe {
            __host_response(retptr);
            buf.set_len(len);
        }
        Ok(buf)
    }
}

#[cold]
#[inline(never)]
pub fn console_log(s: &str) {
    unsafe {
        __console_log(s.as_ptr(), s.len());
    }
}

pub mod errors;
pub mod prelude;
