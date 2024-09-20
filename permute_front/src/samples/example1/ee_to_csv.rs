use crate::EmploymentRecord;
use crate::Csv;

/// Wrapper sink to feed employment records to a CSV file.
pub struct Ee2Csv {
    sink: Csv,
}

impl Ee2Csv {
    pub fn new(sink: Csv) -> Self {
        Self { sink }
    }
}

/// Record to be written to the CSV file.
/// Derive `RecToVec` to convert the record to a vector of strings.
#[derive(serde::Serialize)]
pub struct Record {
    #[serde(rename = "#")]
    rec_num: u32,

    #[serde(rename = "Employee ID")]
    empl_id: String,

    #[serde(rename = "Hire Date")]
    hire_date: chrono::NaiveDate,

    #[serde(rename = "Termination Date")]
    term_date: Option<chrono::NaiveDate>,

    #[serde(rename = "Salary")]
    salary: crate::Monetary,

    #[serde(rename = "Department")]
    dept: Option<String>,

    #[serde(rename = "Job Title")]
    title: Option<String>,
}

impl permute::Sink<EmploymentRecord> for Ee2Csv {
    type Error = permute::SinkError;

    fn put(&mut self, ee: EmploymentRecord) -> Result<(), Self::Error> {
        let rec_num = self.csv.row_sequence.advance();

        let record = Record {
            rec_num,
            empl_id: ee.employee_id,
            hire_date: ee.hire_date,
            term_date: ee.termination_date,
            salary: ee.salary,
            dept: ee.meta.and_then(|v| v.get("department")),
            title: ee.meta.and_then(|v| v.get("job_title")),
        };

        self.csv.put(record)
    }

    fn done(&mut self) -> Result<(), Self::Error> {
        self.csv.done()
    }
}
