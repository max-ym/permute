//! Extension to standard library to supplement types that are needed for
//! describing the world for the file parsing/generation project.

// List crates here so that they are forced to be built even
// despite being potentially unused in this particular sub-project.
extern crate lazy_regex;
extern crate smallvec;
extern crate serde;
extern crate serde_derive;
extern crate chrono;
extern crate log;
extern crate compact_str;

/// A sink to feed to the values of a given type. 
pub trait Sink<T> {
    /// The error type that can be returned by the sink.
    type Error;

    /// Put a value into the sink.
    fn put(&mut self, value: T) -> Result<(), Self::Error>;

    /// Indicate that no more values will be put into the sink.
    /// After this call, the sink should be considered closed.
    /// Any calls to [Self::put] after this call should return an error.
    fn done(&mut self) -> Result<(), Self::Error>;
}

/// A source to get values of a given type.
pub trait Source {
    /// The type of values that the source produces.
    type Item;

    /// The error type that can be returned by the source.
    type Error;

    fn next(&mut self) -> Option<Result<Self::Item, Self::Error>>;
}

impl<I, E> std::iter::Iterator for dyn Source<Item = I, Error = E> {
    type Item = Result<I, E>;

    fn next(&mut self) -> Option<Self::Item> {
        Source::next(self)
    }
}
