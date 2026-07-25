// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Decode and encode ECU calibration values from a firmware image using an
//! A2L (ASAM MCD-2 MC) description.
//!
//! The crate is deliberately free of any host or Tauri dependency: it reads the
//! image through the [`ByteSource`](model::ByteSource) trait, so the conversion
//! maths can be unit-tested against a plain map.
//!
//! Scope covers scalars and one-dimensional objects (curves, axis points, value
//! blocks). Maps and higher dimensions are recognised and reported as
//! [`Category::Unsupported`](model::Category::Unsupported) rather than being
//! silently omitted, so coverage statistics stay honest.

pub mod convert;
pub mod db;
pub mod decode;
pub mod encode;
pub mod layout;
pub mod model;
pub mod stats;

pub use db::{A2lDatabase, Endian, ObjectPlan};
pub use model::{
    A2lSummary, ByteSource, Category, CoverageStats, EncodedWrite, ObjKind, ParamDetail, ParamRow,
    PointValue, Presence,
};
