//! Example of integrated Rust module into the project.

pub struct Monetary {
    dollar: u32,
    cent: u8,
}

pub struct InvalidMonetaryParts;

impl Monetary {
    pub fn new(dollar: u32, cent: u8) -> Result<Self, InvalidMonetaryParts> {
        if cent >= 100 {
            return Err(InvalidMonetaryParts);
        }
        Ok(Monetary { dollar, cent })
    }

    pub fn dollar(&self) -> u32 {
        self.dollar
    }

    pub fn cent(&self) -> u8 {
        self.cent
    }

    pub fn float(&self) -> f64 {
        self.dollar as f64 + f64::from(self.cent) / 100.0
    }
}
