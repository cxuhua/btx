#![allow(dead_code)]

pub mod account;
pub mod accpool;
pub mod block;
pub mod bytes;
pub mod config;
pub mod consts;
pub mod crypto;
pub mod errors;
pub mod hasher;
pub mod index;
pub mod iobuf;
pub mod leveldb;
pub mod merkle;
pub mod pubsub;
pub mod script;
pub mod store;
pub mod util;

#[macro_use]
extern crate lazy_static;

/// 模块初始化
pub fn init() {
    env_logger::builder()
        .filter(Some("btx"), log::LevelFilter::Trace)
        .init();
}
