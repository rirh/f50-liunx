//! Releases: their files' names ([`assets`], the one module that knows them), the list of them ([`index`]), where
//! they come from ([`source`]), and whether a release's files are the project's ([`verify`], [`sig`]).

pub mod assets;
pub mod index;
pub mod sig;
pub mod source;
pub mod verify;
