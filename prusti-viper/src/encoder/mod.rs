// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub use self::encoder::Encoder;

mod borrows;
mod builtin_encoder;
mod encoder;
mod error_manager;
mod foldunfold;
mod initialisation;
mod loop_encoder;
mod mir_encoder;
mod mir_interpreter;
mod optimiser;
mod places;
mod procedure_encoder;
mod pure_function_encoder;
mod spec_encoder;
mod type_encoder;
mod utils;

pub mod vir;
