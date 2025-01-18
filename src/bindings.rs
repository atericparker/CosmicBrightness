// src/bindings.rs
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

// We include the generated bindings. The "OUT_DIR" is a build-script env variable
// so we access it at compile-time via include!.
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
