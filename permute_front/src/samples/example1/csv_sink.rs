#[derive(Default)]
pub struct RowSequence {
    current: u32,
}

impl RowSequence {
    pub fn new(start: u32) -> RowSequence {
        RowSequence {
            current: start,
        }
    }

    pub fn advance(&mut self) -> u32 {
        let current = self.current;
        self.current += 1;
        current
    }
}

/// Function that transforms input into CSV writeable form and pushes the formatted value
/// to the writer.
pub type WriteFn<T> = dyn FnMut(&mut crate::Csv, T);

