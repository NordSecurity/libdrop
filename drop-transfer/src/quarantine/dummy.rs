use std::{io::Result, path::Path};

impl super::PathExt for Path {
    fn quarantine(&self) -> Result<()> {
        Ok(())
    }
}
