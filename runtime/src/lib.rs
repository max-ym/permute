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
